use reqwest::Client;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::Result;
use crate::core::task::{DownloadTask, TaskManager, TaskId, TaskStatus, TaskEvent, FileProgress};
use crate::ui::ProgressManager;
use tokio::sync::Semaphore;
use std::time::Duration;
use tokio::sync::Mutex;
use futures::StreamExt;
use crate::config::Config;
use tokio::fs::OpenOptions as TokioOpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

// 结构体：Downloader
// 下载器，负责管理下载任务和进度
pub struct Downloader {
    client: Client,  // HTTP 客户端
    is_running: Arc<AtomicBool>,  // 控制下载器是否运行
    max_concurrent: Arc<Mutex<Semaphore>>,  // 最大并发下载数
    progress_manager: Arc<ProgressManager>,  // 进度管理器
    config: Arc<Config>,  // 配置信息
    chunk_size: usize,  // 分片大小
    max_chunks: usize,  // 最大分片数
    min_chunks: usize,
}

impl Downloader {
    // 构造函数：创建 Downloader 实例
    pub fn new(chunk_size: u64, max_chunks: usize) -> Self {
        let config = Config::default();
        Downloader {
            client: Client::builder()
                .timeout(Duration::from_secs(config.timeout))
                .user_agent(&config.user_agent)
                .redirect(reqwest::redirect::Policy::limited(config.max_redirects))
                .build()
                .unwrap_or_default(),
            is_running: Arc::new(AtomicBool::new(true)),
            max_concurrent: Arc::new(Mutex::new(Semaphore::new(10))),
            progress_manager: Arc::new(ProgressManager::new()),
            config: Arc::new(config),
            chunk_size: chunk_size as usize,
            max_chunks,
            min_chunks: 1,
        }
    }

    // 方法：下载多个任务
    pub async fn download_multiple(&self, tasks: Vec<DownloadTask>, output_dir: &str) -> Result<()> {
        let mut all_handles = vec![];
        let progress_manager = self.progress_manager.clone();
        
        // 为每个任务创建任务管理
        for (task_index, task) in tasks.iter().enumerate() {
            if !self.is_running.load(Ordering::SeqCst) {
                break;
            }
            
            // 为每个任务创建一个事件通道
            let (event_sender, _event_receiver) = tokio::sync::mpsc::unbounded_channel::<TaskEvent>();
            
            // 为每个任务创建一个任务管理器
            let task_manager = TaskManager::new();
            let task_id = task_manager.add_task(task.urls.clone(), Some(event_sender)).await;
            
            // 为任务中的每个URL创建独立的下载任务
            for (url_index, url) in task.urls.iter().enumerate() {
                let filename = TaskManager::get_filename_from_url(url);
                let output_file = Path::new(output_dir).join(&filename);
                let output_file_str = output_file.to_str().unwrap_or("");
                
                // 注册进度条，获取索引
                let pb_index = progress_manager.add_progress_bar(0, &filename).await;
                
                // 克隆必要的字段用于异步任务
                let downloader = self.clone();
                let url = url.clone();
                let file_path = output_file_str.to_string();
                let task_manager = task_manager.clone();
                let task_id = task_id.clone();
                let progress_manager = progress_manager.clone();
                
                // 为每个URL创建独立的下载任务
                let handle = tokio::spawn(async move {
                    // 下载单个文件，传递全局进度管理器和进度条索引
                    downloader.download_file(&url, &file_path, pb_index, progress_manager, task_manager, task_id).await
                });
                
                all_handles.push(handle);
            }
        }
        
        // 等待所有下载任务完成
        for handle in all_handles {
            handle.await??;
        }
        
        Ok(())
    }

    // 方法：停止下载
    pub async fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    // 统一的下载方法 - 使用TaskManager进行任务管理
    pub async fn download_file(
        &self, 
        url: &str, 
        file: &str, 
        task_index: usize, 
        progress_manager: Arc<ProgressManager>,
        task_manager: TaskManager,
        task_id: TaskId
    ) -> Result<()> {
        // 更新任务状态为运行中
        task_manager.update_task_status(&task_id, TaskStatus::Running).await;

        let response = self.client.head(url).send().await?;
        let total_size = response.content_length().unwrap_or(0);
        let supports_range = response.headers().get("accept-ranges").map_or(false, |v| v == "bytes");

        if total_size > 10 * 1024 * 1024 && supports_range {
            // 大文件且支持 Range，使用分片多线程下载
            self.download_chunked(url, file, total_size, task_index, progress_manager, task_manager.clone(), task_id.clone()).await?;
        } else {
            // 小文件或不支持 Range，使用顺序下载
            self.download_sequential(url, file, total_size, task_index, progress_manager, task_manager.clone(), task_id.clone()).await?;
        }

        // 更新任务状态为完成
        task_manager.update_task_status(&task_id, TaskStatus::Completed).await;

        Ok(())
    }

    // 统一的分片下载方法
    async fn download_chunked(
        &self, 
        url: &str, 
        file: &str, 
        total_size: u64, 
        task_index: usize, 
        progress_manager: Arc<ProgressManager>,
        task_manager: TaskManager,
        task_id: TaskId
    ) -> Result<()> {
        // 检查是否有进度文件，实现断点续传
        let progress_path = format!("{}.progress", file);
        let progress = if std::path::Path::new(&progress_path).exists() {
            match FileProgress::load_from_file(&progress_path).await {
                Ok(p) => {
                    println!("发现进度文件，恢复下载: {}", file);
                    p
                }
                Err(_) => {
                    println!("进度文件损坏，重新开始下载: {}", file);
                    FileProgress::new(url, file, total_size, self.chunk_size as u64)
                }
            }
        } else {
            FileProgress::new(url, file, total_size, self.chunk_size as u64)
        };

        let progress_arc = Arc::new(Mutex::new(progress.clone()));
        let max_retry = 3;
        let url_speed_history = Arc::new(Mutex::new(Vec::new()));
        let speed_check_interval = Duration::from_secs(2);
        let high_speed_threshold = 1024 * 1024; // 1MB/s
        let low_speed_threshold = 100 * 1024;   // 100KB/s
        let progress_arc_for_speed = progress_arc.clone();

        // 动态调整分片数的监控任务
        let speed_history_for_adjust = url_speed_history.clone();
        let task_manager_for_adjust = task_manager.clone();
        let adjust_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(speed_check_interval).await;
                let history = speed_history_for_adjust.lock().await;
                if history.is_empty() { continue; }
                let avg_speed: u64 = history.iter().sum::<u64>() / history.len() as u64;
                if avg_speed > high_speed_threshold {
                    task_manager_for_adjust.increase_chunks();
                } else if avg_speed < low_speed_threshold {
                    task_manager_for_adjust.decrease_chunks();
                }
                // 任务全部完成时退出
                let progress = progress_arc_for_speed.lock().await;
                if !progress.has_incomplete_chunks() { break; }
            }
        });

        // 分片下载主循环
        let mut chunk_indices: Vec<_> = progress.chunks.iter().filter(|c| !c.downloaded).map(|c| c.index).collect();
        while !chunk_indices.is_empty() {
            let cur_chunks = task_manager.get_current_chunks();
            let batch: Vec<_> = chunk_indices.drain(..cur_chunks.min(chunk_indices.len())).collect();
            let mut batch_handles = vec![];
            for chunk_idx in batch {
                let chunk = progress.chunks[chunk_idx].clone();
                // 检查任务状态
                if let Some(is_cancelled) = task_manager.is_task_cancelled(&task_id).await {
                    if is_cancelled {
                        return Err(anyhow::anyhow!("Task cancelled"));
                    }
                }
                
                if let Some(is_paused) = task_manager.is_task_paused(&task_id).await {
                    if is_paused {
                        // 等待恢复
                        while task_manager.is_task_paused(&task_id).await.unwrap_or(false) {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            if task_manager.is_task_cancelled(&task_id).await.unwrap_or(false) {
                                return Err(anyhow::anyhow!("Task cancelled while paused"));
                            }
                        }
                    }
                }

                if !self.is_running.load(Ordering::SeqCst) {
                    break;
                }
                
                // 获取并发许可
                {
                    let semaphore = self.max_concurrent.lock().await;
                    let _permit = semaphore.acquire().await?;
                }
                
                let client = self.client.clone();
                let progress_arc = progress_arc.clone();
                let speed_history = url_speed_history.clone();
                let url = url.to_string();
                let file_path = file.to_string();
                let progress_manager = progress_manager.clone();
                let task_manager = task_manager.clone();
                let task_id = task_id.clone();
                let progress_path = progress_path.clone();
                let total_size = total_size;
                let task_index = task_index;
                let max_retry = max_retry;
                let handle = tokio::spawn(async move {
                    let mut retry = 0;
                    loop {
                        if let Some(is_cancelled) = task_manager.is_task_cancelled(&task_id).await {
                            if is_cancelled {
                                break Err(anyhow::anyhow!("Task cancelled"));
                            }
                        }
                        let start_time = std::time::Instant::now();
                        let range = format!("bytes={}-{}", chunk.start, chunk.end);
                        let resp = client.get(&url).header("Range", range).send().await;
                        match resp {
                            Ok(resp) => {
                                let bytes = resp.bytes().await?;
                                let elapsed = start_time.elapsed().as_secs_f64();
                                let speed = if elapsed > 0.0 { (bytes.len() as f64 / elapsed) as u64 } else { bytes.len() as u64 };
                                {
                                    let mut history = speed_history.lock().await;
                                    history.push(speed);
                                    if history.len() > 10 { history.remove(0); }
                                }
                                let mut file = TokioOpenOptions::new().create(true).write(true).open(&file_path).await?;
                                file.seek(SeekFrom::Start(chunk.start)).await?;
                                file.write_all(&bytes).await?;
                                let mut progress = progress_arc.lock().await;
                                if let Some(c) = progress.chunks.iter_mut().find(|c| c.index == chunk.index) {
                                    c.downloaded = true;
                                    c.retry_count = retry;
                                    c.last_speed = Some(speed);
                                }
                                if let Err(e) = progress.save_to_file(&progress_path).await {
                                    eprintln!("保存进度文件失败: {}", e);
                                }
                                let downloaded_chunks = progress.chunks.iter().filter(|c| c.downloaded).count();
                                let downloaded_size = downloaded_chunks as u64 * chunk.size();
                                let progress_percent = (downloaded_size as f32 / total_size as f32) * 100.0;
                                task_manager.update_task_progress(&task_id, progress_percent, speed, total_size).await;
                                progress_manager.update_progress(task_index, downloaded_size, total_size, speed).await;
                                break Ok::<(), anyhow::Error>(());
                            }
                            Err(_) if retry < max_retry => {
                                retry += 1;
                                continue;
                            }
                            Err(e) => {
                                let mut progress = progress_arc.lock().await;
                                if let Some(c) = progress.chunks.iter_mut().find(|c| c.index == chunk.index) {
                                    c.retry_count = retry;
                                    c.last_speed = None;
                                }
                                if let Err(save_err) = progress.save_to_file(&progress_path).await {
                                    eprintln!("保存进度文件失败: {}", save_err);
                                }
                                task_manager.update_task_status(&task_id, TaskStatus::Failed(e.to_string())).await;
                                break Err(anyhow::anyhow!("Chunk {} failed after {} retries: {}", chunk.index, retry, e));
                            }
                        }
                    }
                });
                batch_handles.push(handle);
            }
            for h in batch_handles {
                h.await??;
            }
        }
        // 等待动态调整分片数的监控任务结束
        adjust_handle.await.ok();
        // 下载完成后删除进度文件
        if let Err(e) = tokio::fs::remove_file(&progress_path).await {
            eprintln!("删除进度文件失败: {}", e);
        }
        Ok(())
    }

    // 统一的顺序下载方法
    async fn download_sequential(
        &self, 
        url: &str, 
        file: &str, 
        total_size: u64, 
        task_index: usize, 
        progress_manager: Arc<ProgressManager>,
        task_manager: TaskManager,
        task_id: TaskId
    ) -> Result<()> {
        // 检查是否有进度文件，实现断点续传
        let progress_path = format!("{}.progress", file);
        let mut downloaded = 0;
        
        // 如果存在进度文件，尝试恢复下载位置
        if std::path::Path::new(&progress_path).exists() {
            if let Ok(progress) = FileProgress::load_from_file(&progress_path).await {
                downloaded = progress.get_downloaded_bytes();
                println!("发现进度文件，从位置 {} 恢复下载: {}", downloaded, file);
            }
        }
        
        let mut file_handle = TaskManager::create_file(file).await?;
        
        // 如果有已下载的部分，设置文件指针位置
        if downloaded > 0 {
            file_handle.seek(SeekFrom::Start(downloaded)).await?;
        }
        
        let response = self.client.get(url).send().await?;
        let mut stream = response.bytes_stream();
        
        while let Some(chunk) = stream.next().await {
            // 检查任务状态
            if let Some(is_cancelled) = task_manager.is_task_cancelled(&task_id).await {
                if is_cancelled {
                    return Err(anyhow::anyhow!("Task cancelled"));
                }
            }
            
            if let Some(is_paused) = task_manager.is_task_paused(&task_id).await {
                if is_paused {
                    // 等待恢复
                    while task_manager.is_task_paused(&task_id).await.unwrap_or(false) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if task_manager.is_task_cancelled(&task_id).await.unwrap_or(false) {
                            return Err(anyhow::anyhow!("Task cancelled while paused"));
                        }
                    }
                }
            }
            
            if !self.is_running.load(Ordering::SeqCst) {
                break;
            }
            
            let chunk = chunk?;
            file_handle.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            
            // 保存进度到文件，实现断点续传
            let progress = FileProgress::new(url, file, total_size, total_size); // file参数应为路径字符串
            if let Err(e) = progress.save_to_file(&progress_path).await {
                eprintln!("保存进度文件失败: {}", e);
            }
            
            // 更新任务进度
            let progress_percent = (downloaded as f32 / total_size as f32) * 100.0;
            task_manager.update_task_progress(&task_id, progress_percent, 0, total_size).await;
            
            // 更新URL级别的进度
            progress_manager.update_progress(task_index, downloaded, total_size, 0).await;
        }
        
        // 下载完成后删除进度文件
        if let Err(e) = tokio::fs::remove_file(&progress_path).await {
            eprintln!("删除进度文件失败: {}", e);
        }
        
        Ok(())
    }
}

impl Clone for Downloader {
    // 实现 Clone trait，用于克隆 Downloader 实例
    fn clone(&self) -> Self {
        Downloader {
            client: self.client.clone(),
            is_running: Arc::clone(&self.is_running),
            max_concurrent: Arc::clone(&self.max_concurrent),
            progress_manager: Arc::clone(&self.progress_manager),
            config: Arc::clone(&self.config),
            chunk_size: self.chunk_size,
            max_chunks: self.max_chunks,
            min_chunks: self.min_chunks,
        }
    }
} 