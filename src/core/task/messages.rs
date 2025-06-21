use actix::{Addr, Message};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;
use crate::core::error::DownloadError;
use super::util::FileInfo;

/// 启动任务
pub struct StartTask {
    pub manager_addr: Addr<crate::core::actor_manager::DownloadManagerActor>,
    pub permit: OwnedSemaphorePermit,
}
impl Message for StartTask { type Result = (); }

/// 启动分块下载的消息
pub struct StartChunkedDownload {
    pub url: String,
    pub file: String,
    pub total_size: u64,
    pub task_id: Uuid,
    pub file_info: FileInfo,
}
impl Message for StartChunkedDownload { type Result = (); }

/// 暂停任务
pub struct PauseTask;
impl Message for PauseTask { type Result = (); }

/// 取消任务
pub struct CancelTask;
impl Message for CancelTask { type Result = (); }

/// 查询进度百分比
pub struct QueryProgress;
impl Message for QueryProgress { type Result = f32; }

/// 查询任务状态
pub struct QueryStatus;
impl Message for QueryStatus { type Result = Result<super::state::TaskStatus, ()>; }

/// 查询详细进度
pub struct QueryDetail;
impl Message for QueryDetail { type Result = Result<(), ()>; }

/// 内部用于更新进度
pub struct UpdateProgress {
    pub progress: f32,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
}
impl Message for UpdateProgress { type Result = (); }

/// 标记任务为完成
pub struct MarkCompleted;
impl Message for MarkCompleted { type Result = (); }

/// 标记任务为失败
pub struct MarkFailed {
    pub error: DownloadError,
}
impl Message for MarkFailed { type Result = (); }

/// 下载单个块的消息
pub struct DownloadChunkMsg {
    pub chunk_index: usize,
    pub url: String,
    pub file: String,
    pub start: u64,
    pub end: u64,
    pub task_id: Uuid,
}
impl Message for DownloadChunkMsg { type Result = Result<(), DownloadError>; } 