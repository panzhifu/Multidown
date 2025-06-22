use actix::Addr;
use futures::StreamExt;
use std::time::Instant;
use std::sync::{Arc, Mutex};
use std::borrow::Cow;

use crate::config::Config;
use crate::core::error::DownloadError;
use super::actor::DownloadTaskActor;
use super::messages::{MarkCompleted, MarkFailed, UpdateProgress};
use super::retry::RetryContext;
use super::util::{BufferManager, SpeedLimiter};

/// 带重试的单线程下载函数
pub async fn start_single_download_with_retry(
    actor_addr: Addr<DownloadTaskActor>,
    url: String,
    file: String,
    _total_size: u64,
    config: Config,
) {
    let progress_addr = actor_addr.clone();
    let error_addr = actor_addr.clone();
    
    // 创建重试上下文
    let mut retry_context = RetryContext::new(
        config.retry_count as u32,
        std::time::Duration::from_secs(config.retry_delay),
        std::time::Duration::from_secs(config.retry_max_delay)
    );
    
    // 在单独的线程中运行 awc 下载
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            loop {
                match perform_single_download(&url, &file, &progress_addr, &config).await {
                    Ok(()) => {
                        println!("[actor_task] 单线程下载完成");
                        actor_addr.do_send(MarkCompleted);
                        break;
                    },
                    Err(error) => {
                        println!("[actor_task] 单线程下载失败: {:?}", error);
                        log::error!("单线程下载失败: {:?}", error);
                        if retry_context.should_retry(&error) {
                            retry_context.record_retry();
                            let delay = retry_context.get_next_delay();
                            println!("[actor_task] 将在 {} 秒后重试下载 (第 {} 次重试)", delay.as_secs(), retry_context.current_retries());
                            tokio::time::sleep(delay).await;
                        } else {
                            actor_addr.do_send(MarkFailed { error });
                            break;
                        }
                    }
                }
            }
        });
    });
    
    // 等待下载线程完成
    if let Err(e) = handle.join() {
        println!("[actor_task] 下载线程异常: {:?}", e);
        log::error!("下载线程异常: {:?}", e);
        error_addr.do_send(MarkFailed { error: DownloadError::Unknown(Cow::Borrowed("下载线程异常")) });
    }
}

/// 执行单次单线程下载
async fn perform_single_download(
    url: &str,
    file: &str,
    progress_addr: &Addr<DownloadTaskActor>,
    config: &Config,
) -> Result<(), DownloadError> {
    let client = awc::Client::default();
    let mut response = client.get(url).send().await
        .map_err(|e| DownloadError::NetworkError(format!("{:?}", e).into()))?;
    
    if !response.status().is_success() {
        return Err(DownloadError::ServerError(format!("服务器错误: {}", response.status()).into()));
    }
    
    let total = response.headers().get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
        
    let mut buffer_manager = BufferManager::new(file, 1024 * 1024)?;
    
    let mut downloaded = 0u64;
    let mut last_update = Instant::now();
    let mut limiter = if config.speed_limit_kb > 0 {
        Some(SpeedLimiter::new(config.speed_limit_kb * 1024))
    } else {
        None
    };
    
    while let Some(chunk) = response.next().await {
        match chunk {
            Ok(bytes) => {
                if let Some(ref mut limiter) = limiter {
                    let wait = limiter.wait_if_needed(bytes.len() as u64);
                    if !wait.is_zero() {
                        tokio::time::sleep(wait).await;
                    }
                }
                buffer_manager.write(bytes.as_ref())?;
                downloaded += bytes.len() as u64;
                let progress = if total > 0 { (downloaded as f32 / total as f32) * 100.0 } else { 0.0 };
                let now = Instant::now();
                if now.duration_since(last_update).as_secs_f64() >= 1.0 {
                    let speed = (downloaded as f64 / now.duration_since(last_update).as_secs_f64()) as u64;
                    progress_addr.do_send(UpdateProgress { progress, downloaded, total, speed });
                    last_update = now;
                }
            },
            Err(e) => {
                println!("[download] 网络流错误: {:?}", e);
                log::error!("网络流错误: {:?}", e);
                return Err(DownloadError::Unknown(format!("网络流错误: {:?}", e).into()));
            }
        }
    }
    
    buffer_manager.flush()?;
    
    let final_written = buffer_manager.get_total_written();
    if final_written >= total && total > 0 {
        Ok(())
    } else {
        println!("[download] 文件大小不匹配: 预期 {} 实际 {}", total, final_written);
        log::error!("文件大小不匹配: 预期 {} 实际 {}", total, final_written);
        Err(DownloadError::SizeMismatch{ expected: total, actual: final_written })
    }
}

/// 执行单次块下载
pub async fn perform_chunk_download(
    url: &str,
    file: &str,
    chunk_index: usize,
    start: u64,
    end: u64,
    limiter: Option<Arc<Mutex<SpeedLimiter>>>,
) -> Result<(), DownloadError> {
    let client = awc::Client::default();
    let range_header = format!("bytes={}-{}", start, end);
    
    let mut response = client.get(url)
        .insert_header(("Range", range_header))
        .send()
        .await
        .map_err(|e| DownloadError::NetworkError(format!("{:?}", e).into()))?;
    
    if !response.status().is_success() && response.status() != 206 {
        return Err(DownloadError::ServerError(format!("服务器错误: {}", response.status()).into()));
    }
    
    let chunk_path = format!("downloads/temp/{}/chunk_{:04}", 
        file.replace("/", "_").replace("\\", "_"), chunk_index);
    
    let mut buffer_manager = BufferManager::new(&chunk_path, 256 * 1024)?;
    
    while let Some(chunk) = response.next().await {
        match chunk {
            Ok(bytes) => {
                if let Some(ref limiter) = limiter {
                    let mut limiter = limiter.lock().unwrap();
                    let wait = limiter.wait_if_needed(bytes.len() as u64);
                    drop(limiter);
                    if !wait.is_zero() {
                        tokio::time::sleep(wait).await;
                    }
                }
                buffer_manager.write(bytes.as_ref())?;
            }
            Err(e) => return Err(DownloadError::Unknown(format!("网络流错误: {:?}", e).into())),
        }
    }
    
    buffer_manager.flush()?;
    
    let final_written = buffer_manager.get_total_written();
    let expected_size = end - start + 1;
    if final_written != expected_size {
        return Err(DownloadError::SizeMismatch { 
            expected: expected_size, 
            actual: final_written 
        });
    }
    
    Ok(())
} 