use actix::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use tokio::io::SeekFrom;
use std::fs;
use serde::{Serialize, Deserialize};
use crate::core::error::DownloadError;
use std::path::Path;
use crate::config::Config;

/// 下载任务状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Paused,
    Cancelled,
}

/// 单任务 Actor
pub struct DownloadTaskActor {
    pub url: String,
    pub file: String,
    pub progress: f32,
    pub is_paused: Arc<AtomicBool>,
    pub is_cancelled: Arc<AtomicBool>,
    pub status: TaskStatus,
    pub total_size: u64,
    pub downloaded: u64,
}

impl DownloadTaskActor {
    pub fn new(_config: Config, url: String, file: String) -> Self {
        Self {
            url,
            file,
            progress: 0.0,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            status: TaskStatus::Pending,
            total_size: 0,
            downloaded: 0,
        }
    }
}

impl Actor for DownloadTaskActor {
    type Context = Context<Self>;
}

#[derive(Clone, Debug)]
struct Chunk {
    start: u64,
    end: u64,
    downloaded: bool,
}

// ================== 消息与Handler定义 ==================

/// 启动任务
pub struct StartTask;
impl Message for StartTask { type Result = (); }
impl Handler<StartTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: StartTask, ctx: &mut Self::Context) {
        self.is_paused.store(false, Ordering::SeqCst);
        self.status = TaskStatus::Running;
        let url = self.url.clone();
        let file = self.file.clone();
        let is_paused = self.is_paused.clone();
        let is_cancelled = self.is_cancelled.clone();
        let actor_addr = ctx.address();
        actix::spawn(async move {
            // URL 校验
            if !crate::utils::validator::is_valid_url(&url) {
                actor_addr.do_send(MarkFailed { error: DownloadError::InvalidUrl(url.clone()) });
                return;
            }
            // 文件已存在
            if Path::new(&file).exists() {
                actor_addr.do_send(MarkFailed { error: DownloadError::FileExists(file.clone()) });
                return;
            }
            let min_chunk = 512 * 1024;
            let max_chunk = 16 * 1024 * 1024;
            let min_conc = 1;
            let max_conc = 8;
            let mut chunk_size = 2 * 1024 * 1024; // 2MB
            let mut max_concurrent = 4;
            let max_retry = 3;
            // 1. 获取远程文件总大小
            let client = Arc::new(reqwest::Client::new());
            let total = match client.head(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_server_error() {
                        actor_addr.do_send(MarkFailed { error: DownloadError::ServerError(format!("服务器错误: {}", resp.status())) });
                        return;
                    }
                    resp.content_length().unwrap_or(0)
                },
                Err(_) => {
                    actor_addr.do_send(MarkFailed { error: DownloadError::Unknown("网络错误".to_string()) });
                    return;
                }
            };
            // 2. 构建分片
            let mut chunks = vec![];
            let mut start = 0;
            while start < total {
                let end = (start + chunk_size - 1).min(total - 1);
                chunks.push(Chunk { start, end, downloaded: false });
                start = end + 1;
            }
            // 3. 检查本地文件，标记已完成分片
            let file_downloaded = match fs::metadata(&file) {
                Ok(meta) => meta.len(),
                Err(_) => 0,
            };
            for c in chunks.iter_mut() {
                if c.end < file_downloaded {
                    c.downloaded = true;
                }
            }
            // 4. 打开文件
            let file_handle = match tokio::fs::OpenOptions::new().create(true).append(false).write(true).open(&file).await {
                Ok(f) => Arc::new(tokio::sync::Mutex::new(f)),
                Err(e) => {
                    actor_addr.do_send(MarkFailed { error: DownloadError::IoError(e) });
                    return;
                }
            };
            // 5. 分片并发下载
            let progress_addr = actor_addr.clone();
            let downloaded_counter = Arc::new(tokio::sync::Mutex::new(file_downloaded));
            let mut chunk_idx = 0;
            while chunk_idx < chunks.len() {
                // 动态调整并发数
                let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
                let mut batch_handles: Vec<tokio::task::JoinHandle<(usize, f64)>> = vec![];
                let batch_end = (chunk_idx + max_concurrent).min(chunks.len());
                for i in chunk_idx..batch_end {
                    if chunks[i].downloaded { continue; }
                    let permit = semaphore.clone().acquire_owned().await.unwrap();
                    let url = url.clone();
                    let file_handle = file_handle.clone();
                    let is_paused = is_paused.clone();
                    let is_cancelled = is_cancelled.clone();
                    let progress_addr = progress_addr.clone();
                    let downloaded_counter = downloaded_counter.clone();
                    let chunk = chunks[i].clone();
                    let client = client.clone();
                    let handle = tokio::spawn(async move {
                        let mut retry = 0;
                        'retry: loop {
                            let range_header = format!("bytes={}-{}", chunk.start, chunk.end);
                            let t0 = std::time::Instant::now();
                            let resp = client.get(&url).header("Range", range_header).send().await;
                            let mut resp = match resp {
                                Ok(r) => {
                                    if r.status().is_server_error() {
                                        // 服务器错误
                                        drop(permit);
                                        return (i, 0f64);
                                    }
                                    r
                                },
                                Err(_) => {
                                    retry += 1;
                                    if retry < max_retry {
                                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                        continue 'retry;
                                    } else {
                                        drop(permit);
                                        return (i, 0f64);
                                    }
                                }
                            };
                            let mut offset = chunk.start;
                            let mut failed = false;
                            let mut _bytes = 0u64;
                            while let Some(chunk_data) = resp.chunk().await.unwrap_or(None) {
                                if is_cancelled.load(Ordering::SeqCst) { drop(permit); return (i, 0f64); }
                                while is_paused.load(Ordering::SeqCst) {
                                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                }
                                let mut file = file_handle.lock().await;
                                if let Err(_) = file.seek(SeekFrom::Start(offset)).await {
                                    failed = true; break;
                                }
                                if let Err(_) = file.write_all(&chunk_data).await {
                                    failed = true; break;
                                }
                                offset += chunk_data.len() as u64;
                                _bytes += chunk_data.len() as u64;
                                // 更新全局进度
                                let mut downloaded = downloaded_counter.lock().await;
                                *downloaded += chunk_data.len() as u64;
                                let progress = (*downloaded as f32 / total as f32) * 100.0;
                                progress_addr.do_send(UpdateProgress { progress, downloaded: *downloaded, total });
                            }
                            let _elapsed = t0.elapsed().as_secs_f64();
                            if failed {
                                retry += 1;
                                if retry < max_retry {
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                    continue 'retry;
                                } else {
                                    drop(permit);
                                    return (i, 0f64);
                                }
                            }
                            break;
                        }
                        drop(permit);
                        (i, 0f64)
                    });
                    batch_handles.push(handle);
                }
                // 等待本批次分片完成，收集速度
                let mut speeds = vec![];
                for h in batch_handles {
                    if let Ok((idx, speed)) = h.await {
                        speeds.push(speed);
                        chunks[idx].downloaded = true;
                    }
                }
                // 动态调整chunk_size和max_concurrent
                if !speeds.is_empty() {
                    let avg_speed = speeds.iter().sum::<f64>() / speeds.len() as f64;
                    if avg_speed > 10.0 * 1024.0 * 1024.0 {
                        chunk_size = (chunk_size * 2).min(max_chunk);
                        max_concurrent = (max_concurrent + 1).min(max_conc);
                    } else if avg_speed < 500.0 * 1024.0 {
                        chunk_size = (chunk_size / 2).max(min_chunk);
                        max_concurrent = (max_concurrent - 1).max(min_conc);
                    }
                }
                // 重新生成后续分片（只对未分配的部分）
                let mut new_chunks: Vec<Chunk> = vec![];
                let mut next_start = if batch_end < chunks.len() { chunks[batch_end].start } else { total };
                while next_start < total {
                    let end = (next_start + chunk_size - 1).min(total - 1);
                    new_chunks.push(Chunk { start: next_start, end, downloaded: false });
                    next_start = end + 1;
                }
                if !new_chunks.is_empty() {
                    chunks.truncate(batch_end);
                    chunks.extend(new_chunks);
                }
                chunk_idx = batch_end;
            }
            // 6. 检查是否全部完成
            let downloaded = downloaded_counter.lock().await;
            if *downloaded >= total && total > 0 {
                actor_addr.do_send(MarkCompleted);
            }
        });
    }
}

/// 暂停任务
pub struct PauseTask;
impl Message for PauseTask { type Result = (); }
impl Handler<PauseTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: PauseTask, _ctx: &mut Self::Context) {
        self.is_paused.store(true, Ordering::SeqCst);
        self.status = TaskStatus::Paused;
    }
}

/// 取消任务
pub struct CancelTask;
impl Message for CancelTask { type Result = (); }
impl Handler<CancelTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: CancelTask, _ctx: &mut Self::Context) {
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.status = TaskStatus::Cancelled;
    }
}

/// 查询进度百分比
pub struct QueryProgress;
impl Message for QueryProgress { type Result = f32; }
impl Handler<QueryProgress> for DownloadTaskActor {
    type Result = f32;
    fn handle(&mut self, _msg: QueryProgress, _ctx: &mut Self::Context) -> f32 {
        self.progress
    }
}

/// 查询任务状态
pub struct QueryStatus;
impl Message for QueryStatus { type Result = Result<TaskStatus, ()>; }
impl Handler<QueryStatus> for DownloadTaskActor {
    type Result = Result<TaskStatus, ()>;
    fn handle(&mut self, _msg: QueryStatus, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.status.clone())
    }
}

/// 查询详细进度
pub struct QueryDetail;
impl Message for QueryDetail { type Result = Result<(), ()>; }
impl Handler<QueryDetail> for DownloadTaskActor {
    type Result = Result<(), ()>;
    fn handle(&mut self, _msg: QueryDetail, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}

/// 内部用于更新进度
pub struct UpdateProgress {
    pub progress: f32,
    pub downloaded: u64,
    pub total: u64,
}
impl Message for UpdateProgress { type Result = (); }
impl Handler<UpdateProgress> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, msg: UpdateProgress, _ctx: &mut Self::Context) {
        self.progress = msg.progress;
        self.downloaded = msg.downloaded;
        self.total_size = msg.total;
    }
}

/// 内部用于标记完成
pub struct MarkCompleted;
impl Message for MarkCompleted { type Result = (); }
impl Handler<MarkCompleted> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: MarkCompleted, _ctx: &mut Self::Context) {
        self.status = TaskStatus::Completed;
        self.progress = 100.0;
    }
}

/// 内部用于标记失败
pub struct MarkFailed {
    pub error: DownloadError,
}
impl Message for MarkFailed { type Result = (); }
impl Handler<MarkFailed> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, msg: MarkFailed, _ctx: &mut Self::Context) {
        self.status = TaskStatus::Failed(msg.error.to_string());
    }
} 