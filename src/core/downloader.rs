use reqwest::Client;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::Result;
use crate::ui::ProgressManager;
use tokio::sync::Semaphore;
use std::time::Duration;
use tokio::sync::Mutex;
use futures::StreamExt;
use crate::config::Config;
use tokio::fs::OpenOptions as TokioOpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

// 结构体：Downloader
// 只负责底层下载实现，不再管理任务状态
#[derive(Clone)]
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

    pub async fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    // 只保留底层下载方法，任务调度交由actor管理
    pub async fn download_file_raw(
        &self,
        url: &str,
        file: &str,
        total_size: u64,
    ) -> Result<()> {
        let mut file_handle = TokioOpenOptions::new().create(true).write(true).open(file).await?;
        let response = self.client.get(url).send().await?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file_handle.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
        }
        Ok(())
    }

    // 你可以根据需要扩展更多底层下载方法
} 