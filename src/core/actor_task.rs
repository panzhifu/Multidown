use actix::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 下载任务状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Paused,
    Cancelled,
}

/// 下载任务进度
#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub progress: f32,
    pub speed: u64,
    pub size: u64,
}

/// 消息：启动任务
pub struct StartTask;
impl Message for StartTask {
    type Result = ();
}

/// 消息：暂停任务
pub struct PauseTask;
impl Message for PauseTask {
    type Result = ();
}

/// 消息：取消任务
pub struct CancelTask;
impl Message for CancelTask {
    type Result = ();
}

/// 消息：查询进度
pub struct QueryProgress;
impl Message for QueryProgress {
    type Result = f32; // 进度百分比
}

/// 消息：查询状态
pub struct QueryStatus;
impl Message for QueryStatus {
    type Result = Result<TaskStatus, ()>;
}

/// 单任务 Actor
pub struct DownloadTaskActor {
    pub urls: Vec<String>,
    pub progress: f32,
    pub is_paused: Arc<AtomicBool>,
    pub is_cancelled: Arc<AtomicBool>,
    pub status: TaskStatus,
}

impl DownloadTaskActor {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            urls,
            progress: 0.0,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            status: TaskStatus::Pending,
        }
    }
}

impl Actor for DownloadTaskActor {
    type Context = Context<Self>;
}

impl Handler<StartTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: StartTask, _ctx: &mut Self::Context) {
        self.is_paused.store(false, Ordering::SeqCst);
        self.status = TaskStatus::Running;
        // 这里后续可启动分片下载
    }
}

impl Handler<PauseTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: PauseTask, _ctx: &mut Self::Context) {
        self.is_paused.store(true, Ordering::SeqCst);
        self.status = TaskStatus::Paused;
        // 这里后续可暂停分片下载
    }
}

impl Handler<CancelTask> for DownloadTaskActor {
    type Result = ();
    fn handle(&mut self, _msg: CancelTask, _ctx: &mut Self::Context) {
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.status = TaskStatus::Cancelled;
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