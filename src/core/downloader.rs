use reqwest::Client;
use std::fs::{self, File, OpenOptions};
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
use futures::StreamExt;
use crate::config::Config;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use url::Url;

#[derive(Serialize, Deserialize, Debug)]
struct DownloadProgress {
    url: String,
    downloaded_size: u64,
    total_size: u64,
    chunks: Vec<(u64, u64)>,  // (start, end) for each chunk
}

#[derive(Serialize, Deserialize, Debug)]
struct ProgressFile {
    tasks: HashMap<String, DownloadProgress>,
    timestamp: i64,
}

pub struct Downloader {
    client: Client,
    should_stop: Arc<AtomicBool>,
    active_downloads: Arc<AtomicUsize>,
    max_concurrent: Arc<Semaphore>,
    progress_manager: Arc<ProgressManager>,
    is_running: Arc<Mutex<bool>>,
    config: Arc<Config>,
    progress_file: String,
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
            progress_file: "download_progress.json".to_string(),
        }
    }

    fn save_progress(&self, url: &str, downloaded: u64, total: u64) -> Result<()> {
        let mut progress = if Path::new(&self.progress_file).exists() {
            let content = fs::read_to_string(&self.progress_file)?;
            serde_json::from_str(&content).unwrap_or_else(|_| ProgressFile {
                tasks: HashMap::new(),
                timestamp: chrono::Local::now().timestamp(),
            })
        } else {
            ProgressFile {
                tasks: HashMap::new(),
                timestamp: chrono::Local::now().timestamp(),
            }
        };

        progress.tasks.insert(url.to_string(), DownloadProgress {
            url: url.to_string(),
            downloaded_size: downloaded,
            total_size: total,
            chunks: vec![(0, downloaded)],
        });

        let content = serde_json::to_string_pretty(&progress)?;
        fs::write(&self.progress_file, content)?;
        Ok(())
    }

    fn load_progress(&self, url: &str) -> Result<Option<u64>> {
        if !Path::new(&self.progress_file).exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&self.progress_file)?;
        let progress: ProgressFile = serde_json::from_str(&content)?;
        Ok(progress.tasks.get(url).map(|p| p.downloaded_size))
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

            progress_manager.add_task(&task).await;

            let handle = tokio::spawn(async move {
                let _permit = max_concurrent.acquire().await?;
                active_downloads.fetch_add(1, Ordering::SeqCst);

                let result = downloader.download_with_retry(&task, &output_path).await;

                active_downloads.fetch_sub(1, Ordering::SeqCst);
                
                let success = result.is_ok();
                progress_manager.finish_task(index, success).await;
                
                result
            });

            handles.push((index, handle));
        }

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
        
        // 处理GitHub下载链接
        let url = if task.url.contains("github.com") {
            self.process_github_url(&task.url)?
        } else {
            task.url.clone()
        };

        fs::create_dir_all(output_dir)
            .with_context(|| format!("创建输出目录失败: {}", output_dir))?;

        let filename = Self::get_filename_from_url(&url);
        let output_file = Path::new(output_dir).join(&filename);
        
        log::info!("文件将保存到: {}", output_file.display());

        let mut downloaded = self.load_progress(&url)?.unwrap_or(0);
        let mut file = if downloaded > 0 {
            log::info!("发现已下载的部分: {} 字节", downloaded);
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&output_file)?
        } else {
            File::create(&output_file)?
        };

        let response = self.client.head(&url).send().await?;
        let total_size = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        if downloaded >= total_size {
            log::info!("文件已完整下载: {}", filename);
            return Ok(downloaded);
        }

        let mut request = self.client.get(&url);
        if downloaded > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded));
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(anyhow::anyhow!("下载失败: HTTP {}", status));
        }

        let mut stream = response.bytes_stream();
        let mut last_update = Instant::now();
        let mut last_downloaded = downloaded;

        while let Some(chunk) = stream.next().await {
            if self.should_stop.load(Ordering::SeqCst) {
                self.save_progress(&url, downloaded, total_size)?;
                return Err(anyhow::anyhow!("下载被用户中断"));
            }

            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;

            let now = Instant::now();
            if now.duration_since(last_update) >= Duration::from_secs(1) {
                let speed = (downloaded - last_downloaded) / now.duration_since(last_update).as_secs();
                self.progress_manager.update_progress(0, downloaded, total_size, speed).await;
                last_update = now;
                last_downloaded = downloaded;
            }
        }

        if downloaded >= total_size {
            if Path::new(&self.progress_file).exists() {
                fs::remove_file(&self.progress_file)?;
            }
        }

        Ok(downloaded)
    }

    fn process_github_url(&self, url: &str) -> Result<String> {
        let parsed_url = Url::parse(url)?;
        if parsed_url.host_str().unwrap_or("").contains("github.com") {
            // 如果是GitHub下载链接，尝试使用镜像
            if let Some(path) = parsed_url.path().strip_prefix("/") {
                let mirror_url = format!("https://ghproxy.com/https://github.com/{}", path);
                log::info!("使用GitHub镜像: {}", mirror_url);
                return Ok(mirror_url);
            }
        }
        Ok(url.to_string())
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
            progress_file: self.progress_file.clone(),
        }
    }
} 