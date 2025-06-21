use actix::{ActorFutureExt, AsyncContext, Handler, ResponseActFuture, WrapFuture};
use std::sync::atomic::Ordering;
use std::time::Instant;
use std::path::Path;

use crate::core::error::DownloadError;
use super::actor::DownloadTaskActor;
use super::chunk_manager::ChunkedDownloadManager;
use super::download::{start_single_download_with_retry, perform_chunk_download};
use super::messages::*;
use super::state::TaskStatus;
use super::util::FileInfo;

async fn get_file_info(url: &str) -> Result<FileInfo, DownloadError> {
    let client = awc::Client::default();
    let response = client.head(url).send().await
        .map_err(|e| DownloadError::NetworkError(format!("{:?}", e)))?;
    
    if !response.status().is_success() {
        return Err(DownloadError::ServerError(format!("服务器错误: {}", response.status())));
    }
    
    Ok(FileInfo {
        size: response.headers().get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0),
        supports_range: response.headers().get("accept-ranges")
            .and_then(|v| v.to_str().ok())
            .map(|s| s == "bytes")
            .unwrap_or(false),
        last_modified: response.headers().get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        etag: response.headers().get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    })
}

impl Handler<StartTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, msg: StartTask, ctx: &mut Self::Context) {
        self.is_paused.store(false, Ordering::SeqCst);
        self.status = TaskStatus::Running;
        self.start_time = Some(Instant::now());
        self.permit = Some(msg.permit);
        self.manager_addr = Some(msg.manager_addr);
        
        let url = self.url.clone();
        let file = self.file.clone();
        let actor_addr = ctx.address();
        let config = self.config.clone();
        let task_id = self.id;
        
        actix::spawn(async move {
            if !crate::utils::validator::is_valid_url(&url) {
                actor_addr.do_send(MarkFailed { error: DownloadError::InvalidUrl(url.clone()) });
                return;
            }
            if Path::new(&file).exists() {
                actor_addr.do_send(MarkFailed { error: DownloadError::FileExists(file.clone()) });
                return;
            }
            
            let file_info = match get_file_info(&url).await {
                Ok(info) => info,
                Err(e) => {
                    actor_addr.do_send(MarkFailed { error: e });
                    return;
                }
            };
            
            let total_size = file_info.size;
            let use_chunked = config.enable_chunked_download && total_size > config.min_chunk_size as u64;
            
            if use_chunked {
                actor_addr.do_send(StartChunkedDownload { 
                    url, file, total_size, task_id, file_info,
                });
            } else {
                start_single_download_with_retry(actor_addr, url, file, total_size, config).await;
            }
        });
    }
}

impl Handler<StartChunkedDownload> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, msg: StartChunkedDownload, ctx: &mut Self::Context) {
        let chunk_size = self.config.chunk_size as u64;
        let mut chunk_manager = ChunkedDownloadManager::new(msg.total_size, chunk_size, msg.file.clone());
        
        if self.config.enable_resume {
            if let Err(e) = chunk_manager.load_and_validate_resume_info(self.id, &msg.file_info) {
                println!("[actor_task] 恢复下载失败: {}, 将重新开始下载", e);
                chunk_manager.cleanup_temp_files();
                chunk_manager = ChunkedDownloadManager::new(msg.total_size, chunk_size, msg.file.clone());
            }
        }
        
        self.chunk_manager = Some(chunk_manager);
        self.file_info = Some(msg.file_info);
        self.total_size = msg.total_size;
        
        let url = self.url.clone();
        let file = self.file.clone();
        let id = self.id;
        self.schedule_chunk_downloads(ctx, url, file, id);
    }
}

impl Handler<PauseTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: PauseTask, _ctx: &mut Self::Context) {
        self.is_paused.store(true, Ordering::SeqCst);
        self.status = TaskStatus::Paused;
    }
}

impl Handler<CancelTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: CancelTask, _ctx: &mut Self::Context) {
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.status = TaskStatus::Cancelled;
        if let Some(cm) = &self.chunk_manager {
            cm.cleanup_temp_files();
        }
    }
}

impl Handler<QueryProgress> for DownloadTaskActor {
    type Result = f32;
    fn handle(&mut self, _msg: QueryProgress, _ctx: &mut Self::Context) -> f32 {
        self.progress
    }
}

impl Handler<QueryStatus> for DownloadTaskActor {
    type Result = Result<TaskStatus, ()>;
    fn handle(&mut self, _msg: QueryStatus, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.status.clone())
    }
}

impl Handler<QueryDetail> for DownloadTaskActor {
    type Result = Result<(), ()>;
    fn handle(&mut self, _msg: QueryDetail, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}

impl Handler<UpdateProgress> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, msg: UpdateProgress, _ctx: &mut Self::Context) {
        self.progress = msg.progress;
        self.downloaded = msg.downloaded;
        self.total_size = msg.total;
        self.speed = msg.speed;
        self.notify_manager_progress();
    }
}

impl Handler<MarkCompleted> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: MarkCompleted, _ctx: &mut Self::Context) {
        self.status = TaskStatus::Completed;
        if let Some(permit) = self.permit.take() {
            drop(permit);
        }
        self.notify_manager_completed();
    }
}

impl Handler<MarkFailed> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, msg: MarkFailed, _ctx: &mut Self::Context) {
        self.status = TaskStatus::Failed(msg.error.to_string());
        if let Some(permit) = self.permit.take() {
            drop(permit);
        }
        self.notify_manager_failed(msg.error);
    }
}

impl Handler<DownloadChunkMsg> for DownloadTaskActor {
    type Result = ResponseActFuture<Self, Result<(), DownloadError>>;
    
    fn handle(&mut self, msg: DownloadChunkMsg, _ctx: &mut Self::Context) -> Self::Result {
        let config = self.config.clone();
        let is_paused = self.is_paused.clone();
        let limiter = self.global_limiter.clone();
        Box::pin(async move {
            if is_paused.load(Ordering::SeqCst) {
                return Err(DownloadError::Paused);
            }
            let mut retry_context = super::retry::RetryContext::new(config.retry_strategy());
            loop {
                if is_paused.load(Ordering::SeqCst) {
                    return Err(DownloadError::Paused);
                }
                match perform_chunk_download(&msg.url, &msg.file, msg.chunk_index, msg.start, msg.end, limiter.clone()).await {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        if retry_context.should_retry(&e) {
                            retry_context.increment_retry(e);
                            tokio::time::sleep(retry_context.get_delay()).await;
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
        }.into_actor(self).map(move |result, act, ctx| {
            match result {
                Ok(()) => {
                    if let Some(cm) = &mut act.chunk_manager {
                        cm.mark_chunk_completed(msg.chunk_index);
                        if act.config.enable_resume {
                            if let Some(fi) = &act.file_info {
                                cm.save_resume_info(act.id, &act.url, fi).ok();
                            }
                        }
                        if cm.is_completed() {
                            act.merge_chunks_and_complete();
                        }
                    }
                },
                Err(e) => {
                    if let Some(cm) = &mut act.chunk_manager {
                        cm.mark_chunk_failed(msg.chunk_index);
                        act.check_download_status_and_retry(ctx);
                    }
                    if let DownloadError::Paused = e {
                        act.status = TaskStatus::Paused;
                    }
                }
            }
            Ok(())
        }))
    }
} 