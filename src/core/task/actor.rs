use actix::prelude::*;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::time::{Instant, Duration};
use uuid::Uuid;
use tokio::sync::OwnedSemaphorePermit;

use crate::config::Config;
use crate::core::error::DownloadError;
use super::chunk_manager::ChunkedDownloadManager;
use super::state::TaskStatus;
use super::util::FileInfo;
use super::util::SpeedLimiter;

/// 单任务 Actor
pub struct DownloadTaskActor {
    pub id: Uuid,
    pub url: String,
    pub file: String,
    pub progress: f32,
    pub is_paused: Arc<AtomicBool>,
    pub is_cancelled: Arc<AtomicBool>,
    pub status: TaskStatus,
    pub total_size: u64,
    pub downloaded: u64,
    pub speed: u64, // B/s
    pub start_time: Option<Instant>,
    pub manager_addr: Option<Addr<crate::core::actor_manager::DownloadManagerActor>>,
    pub permit: Option<OwnedSemaphorePermit>,
    pub config: Config,
    pub chunk_manager: Option<ChunkedDownloadManager>,
    pub file_info: Option<FileInfo>,
    pub global_limiter: Option<Arc<Mutex<SpeedLimiter>>>,
}

impl Actor for DownloadTaskActor {
    type Context = Context<Self>;
}

impl DownloadTaskActor {
    pub fn new(config: Config, url: String, file: String) -> Self {
        let global_limiter = if config.speed_limit_kb > 0 {
            Some(Arc::new(Mutex::new(SpeedLimiter::new(config.speed_limit_kb * 1024))))
        } else {
            None
        };
        Self {
            id: Uuid::new_v4(),
            url,
            file,
            progress: 0.0,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            status: TaskStatus::Pending,
            total_size: 0,
            downloaded: 0,
            speed: 0,
            start_time: None,
            manager_addr: None,
            permit: None,
            config,
            chunk_manager: None,
            file_info: None,
            global_limiter,
        }
    }

    pub fn notify_manager_progress(&self) {
        if let Some(manager_addr) = &self.manager_addr {
            let _ = manager_addr.do_send(crate::core::actor_manager::UpdateTaskProgress {
                task_id: self.id,
                progress: self.progress,
                downloaded: self.downloaded,
                total: self.total_size,
                speed: self.speed,
            });
        }
    }

    pub fn notify_manager_completed(&self) {
        if let Some(manager_addr) = &self.manager_addr {
            let _ = manager_addr.do_send(crate::core::actor_manager::MarkTaskCompleted {
                task_id: self.id,
            });
        }
    }

    pub fn notify_manager_failed(&self, error: DownloadError) {
        if let Some(manager_addr) = &self.manager_addr {
            let _ = manager_addr.do_send(crate::core::actor_manager::MarkTaskFailed {
                task_id: self.id,
                error,
            });
        }
    }
    
    /// 启动所有可用的块下载
    pub fn start_available_chunks(&mut self, ctx: &mut Context<Self>, url: &str, file: &str, task_id: Uuid) {
        if self.is_paused.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Some(chunk_manager) = &mut self.chunk_manager {
            while let Some((chunk_index, chunk)) = chunk_manager.get_next_available_chunk() {
                ctx.address().do_send(super::messages::DownloadChunkMsg {
                    chunk_index,
                    url: url.to_string(),
                    file: file.to_string(),
                    start: chunk.start,
                    end: chunk.end,
                    task_id,
                });
            }
        }
    }

    /// 定期调度块下载（处理失败和新块）
    pub fn schedule_chunk_downloads(&mut self, ctx: &mut Context<Self>, url: String, file: String, task_id: Uuid) {
        ctx.run_interval(Duration::from_secs(1), move |act, ctx| {
            if act.status == TaskStatus::Running {
                if act.is_paused.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                act.start_available_chunks(ctx, &url, &file, task_id);
                act.check_download_status_and_retry(ctx);
            }
        });
    }

    /// 合并块并完成任务
    pub fn merge_chunks_and_complete(&mut self) {
        if let Some(chunk_manager) = &self.chunk_manager {
            match chunk_manager.merge_chunks(&self.file) {
                Ok(_) => {
                    self.status = TaskStatus::Completed;
                    println!("[actor_task] merge_chunks_and_complete: 任务已完成，通知 manager");
                    self.notify_manager_completed();
                },
                Err(e) => {
                    self.status = TaskStatus::Failed(e.to_string());
                    self.notify_manager_failed(e);
                }
            }
        }
    }
    
    /// 调度重试失败的块
    pub fn schedule_retry_failed_chunks(&mut self, ctx: &mut Context<Self>) {
        if let Some(chunk_manager) = &mut self.chunk_manager {
            if chunk_manager.should_retry_failed_chunks() {
                let delay = chunk_manager.retry_context.get_delay();
                ctx.run_later(delay, move |act, ctx| {
                    if let Some(chunk_manager) = &mut act.chunk_manager {
                        chunk_manager.retry_failed_chunks(ctx, &act.url, &act.file, act.id);
                    }
                });
            }
        }
    }

    /// 检查下载状态并处理重试
    pub fn check_download_status_and_retry(&mut self, ctx: &mut Context<Self>) {
        if let Some(chunk_manager) = &mut self.chunk_manager {
            let stats = chunk_manager.get_stats();
            let should_retry = chunk_manager.should_retry_failed_chunks();
            
            if stats.failed_chunks == stats.total_chunks && !should_retry {
                let retry_stats = chunk_manager.get_retry_stats();
                println!("[chunked_download] 所有块都失败了，重试统计: {:?}", retry_stats);
                self.notify_manager_failed(DownloadError::Unknown("所有块下载失败".to_string()));
                return;
            }
            
            if stats.failed_chunks > 0 && should_retry {
                self.schedule_retry_failed_chunks(ctx);
            }
        }
    }
} 