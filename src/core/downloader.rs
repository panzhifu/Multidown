use reqwest::Client;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::{Result, Context};
use crate::core::task::DownloadTask;
use crate::ui::{self, ProgressManager};
use tokio::sync::Semaphore;
use std::sync::atomic::AtomicUsize;
use std::time::{Instant, Duration};
use tokio::sync::Mutex;
use futures_util::StreamExt;
use crate::config::Config;

pub struct Downloader {
    client: Client,
    should_stop: Arc<AtomicBool>,
    active_downloads: Arc<AtomicUsize>,
    max_concurrent: Arc<Semaphore>,
    progress_manager: Arc<ProgressManager>,
    is_running: Arc<Mutex<bool>>,
    config: Arc<Config>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DownloadProgress {
    url: String,
    downloaded_size: u64,
    total_size: u64,
    chunks: Vec<(u64, u64)>,  // (start, end) for each chunk
}

impl Downloader {
    pub fn new(max_concurrent: usize) -> Self {
        let config = Config::default();
        Downloader {
            client: Client::builder()
                .timeout(Duration::from_secs(config.timeout))
                .user_agent(&config.user_agent)
                .redirect(reqwest::redirect::Policy::limited(config.max_redirects))
                .build()
                .unwrap_or_default(),
            should_stop: Arc::new(AtomicBool::new(false)),
            active_downloads: Arc::new(AtomicUsize::new(0)),
            max_concurrent: Arc::new(Semaphore::new(max_concurrent)),
            progress_manager: Arc::new(ProgressManager::new()),
            is_running: Arc::new(Mutex::new(true)),
            config: Arc::new(config),
        }
    }

    pub async fn download_multiple(&self, tasks: Vec<DownloadTask>, output_dir: &str, _threads: u32) -> Result<()> {
        let mut handles = vec![];
        let mut success_count = 0;
        let mut failed_count = 0;
        let mut total_size = 0;
        
        for (index, task) in tasks.into_iter().enumerate() {
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            let output_path = output_dir.to_string();
            let downloader = Arc::new(self.clone());
            let active_downloads = Arc::clone(&self.active_downloads);
            let max_concurrent = Arc::clone(&self.max_concurrent);
            let progress_manager = Arc::clone(&self.progress_manager);

            // 添加进度条
            progress_manager.add_task(&task).await;

            let handle = tokio::spawn(async move {
                // 获取并发下载许可
                let _permit = max_concurrent.acquire().await?;
                active_downloads.fetch_add(1, Ordering::SeqCst);

                let result = downloader.download_with_retry(&task, &output_path).await;

                active_downloads.fetch_sub(1, Ordering::SeqCst);
                
                // 更新任务状态
                let success = result.is_ok();
                progress_manager.finish_task(index, success).await;
                
                result
            });

            handles.push((index, handle));
        }

        // 等待所有下载完成
        for (index, handle) in handles {
            match handle.await? {
                Ok(size) => {
                    success_count += 1;
                    total_size += size;
                    self.progress_manager.finish_task(index, true).await;
                }
                Err(e) => {
                    failed_count += 1;
                    log::error!("下载任务失败: {}", e);
                    self.progress_manager.finish_task(index, false).await;
                }
            }
        }

        // 显示下载摘要
        let summary = ui::DownloadSummary {
            total_files: success_count + failed_count,
            total_size,
            elapsed_time: self.progress_manager.elapsed_time(),
            success_count,
            failed_count,
        };
        println!("{}", summary);

        Ok(())
    }

    async fn download_with_retry(&self, task: &DownloadTask, output_dir: &str) -> Result<u64> {
        let mut retries = 0;
        let mut last_error = None;

        while retries < self.config.retry_count as u32 {
            match self.download(task, output_dir).await {
                Ok(size) => return Ok(size),
                Err(e) => {
                    last_error = Some(e);
                    retries += 1;
                    if retries < self.config.retry_count as u32 {
                        log::error!("下载失败，正在重试 ({}/{})", retries, self.config.retry_count);
                        tokio::time::sleep(Duration::from_secs(self.config.retry_delay)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("下载失败")))
    }

    pub async fn download(&self, task: &DownloadTask, output_dir: &str) -> Result<u64> {
        log::info!("开始下载: {}", task.url);
        
        // 确保输出目录存在
        fs::create_dir_all(output_dir)
            .with_context(|| format!("创建输出目录失败: {}", output_dir))?;

        // 从URL获取文件名
        let filename = Self::get_filename_from_url(&task.url);
        let output_file = Path::new(output_dir).join(&filename);
        
        log::info!("文件将保存到: {}", output_file.display());

        // 获取文件大小
        let response = self.client.head(&task.url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        // 下载文件
        let response = self.client.get(&task.url).send().await?;
        if !response.status().is_success() {
            let error_msg = format!("HTTP错误: {}", response.status());
            self.progress_manager.update_error(0, &error_msg).await;
            return Err(anyhow::anyhow!(error_msg));
        }

        let mut file = File::create(&output_file)?;
        let mut downloaded = 0;
        let mut last_update = Instant::now();
        let mut last_downloaded = 0;

        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            if self.should_stop.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("下载被用户取消"));
            }

            let chunk = chunk_result?;
            file.write_all(&chunk)?;
            
            downloaded += chunk.len() as u64;
            
            // 更新进度和速度
            let now = Instant::now();
            if now.duration_since(last_update) >= Duration::from_secs(1) {
                let speed = (downloaded - last_downloaded) / now.duration_since(last_update).as_secs();
                self.progress_manager.update_progress(0, downloaded, total_size, speed).await;
                last_update = now;
                last_downloaded = downloaded;
            }
        }

        // 标记任务完成
        self.progress_manager.finish_task(0, true).await;

        log::info!("下载完成: {}", filename);
        Ok(downloaded)
    }

    pub async fn stop(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
        self.should_stop.store(true, Ordering::SeqCst);
    }

    fn get_filename_from_url(url: &str) -> String {
        url.split('/')
            .last()
            .unwrap_or("downloaded_file")
            .to_string()
    }
}

impl Clone for Downloader {
    fn clone(&self) -> Self {
        Downloader {
            client: self.client.clone(),
            should_stop: Arc::clone(&self.should_stop),
            active_downloads: Arc::clone(&self.active_downloads),
            max_concurrent: Arc::clone(&self.max_concurrent),
            progress_manager: Arc::clone(&self.progress_manager),
            is_running: Arc::clone(&self.is_running),
            config: Arc::clone(&self.config),
        }
    }
} 