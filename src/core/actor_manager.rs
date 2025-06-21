use crate::config::Config;
use crate::core::error::DownloadError;
use crate::core::task::{
    messages as task_messages,
    state::TaskStatus,
    DownloadTaskActor,
};
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;
use futures::future::LocalBoxFuture;

/// ================== 任务元数据结构体 ==================
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadTaskMeta {
    pub id: Uuid,
    pub url: String,
    pub file: String,
    pub status: TaskStatus,
    pub progress: f32,
    pub downloaded: u64,
    pub total: u64,
}

/// 添加下载任务
#[derive(Message)]
#[rtype(result = "Result<Uuid, DownloadError>")]
pub struct CreateTask {
    pub url: String,
    pub file: String,
}

/// 启动指定任务
#[derive(Message)]
#[rtype(result = "()")]
pub struct StartTaskFromMeta {
    pub task_id: Uuid,
}

/// 暂停指定任务
#[derive(Message)]
#[rtype(result = "()")]
pub struct PauseTask(pub Uuid);

/// 取消指定任务
#[derive(Message)]
#[rtype(result = "()")]
pub struct CancelTask(pub Uuid);

/// 查询指定任务进度百分比
#[derive(Message)]
#[rtype(result = "Result<f32, ()>")]
pub struct QueryTaskProgress(pub Uuid);

/// 查询指定任务状态
#[derive(Message)]
#[rtype(result = "Result<TaskStatus, ()>")]
pub struct QueryTaskStatus(pub Uuid);

/// 查询指定任务详细信息
#[derive(Message)]
#[rtype(result = "Option<DownloadTaskMeta>")]
pub struct QueryTaskDetail(pub Uuid);

/// 获取任务统计信息
#[derive(Message)]
#[rtype(result = "TaskStats")]
pub struct GetStats;

/// 内部消息：更新任务进度
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateTaskProgress {
    pub task_id: Uuid,
    pub progress: f32,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
}

/// 内部消息：标记任务完成
#[derive(Message)]
#[rtype(result = "()")]
pub struct MarkTaskCompleted {
    pub task_id: Uuid,
}

/// 内部消息：标记任务失败
#[derive(Message)]
#[rtype(result = "()")]
pub struct MarkTaskFailed {
    pub task_id: Uuid,
    pub error: DownloadError,
}

/// 断点续传信息
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResumeInfo {
    pub task_id: Uuid,
    pub url: String,
    pub file: String,
    pub downloaded_chunks: Vec<(u64, u64)>, // (start, end) 已下载的块
    pub total_size: u64,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

/// 性能指标
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PerformanceMetrics {
    pub task_id: Uuid,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub average_speed: f64, // B/s
    pub peak_speed: u64,    // B/s
    pub retry_count: usize,
    pub error_count: usize,
    pub network_errors: usize,
    pub io_errors: usize,
    pub timeouts: usize,
}

#[allow(dead_code)]
impl PerformanceMetrics {
    pub fn new(task_id: Uuid) -> Self {
        Self {
            task_id,
            start_time: chrono::Utc::now(),
            end_time: None,
            total_bytes: 0,
            downloaded_bytes: 0,
            average_speed: 0.0,
            peak_speed: 0,
            retry_count: 0,
            error_count: 0,
            network_errors: 0,
            io_errors: 0,
            timeouts: 0,
        }
    }
    
    pub fn update_speed(&mut self, speed: u64) {
        if speed > self.peak_speed {
            self.peak_speed = speed;
        }
    }
    
    pub fn calculate_average_speed(&mut self) {
        if let Some(end_time) = self.end_time {
            let duration = end_time.signed_duration_since(self.start_time);
            let duration_secs = duration.num_seconds() as f64;
            if duration_secs > 0.0 {
                self.average_speed = self.downloaded_bytes as f64 / duration_secs;
            }
        }
    }
    
    pub fn get_duration(&self) -> Option<chrono::Duration> {
        self.end_time.map(|end| end.signed_duration_since(self.start_time))
    }
}

/// 任务优先级
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// 任务依赖关系
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TaskDependency {
    pub task_id: Uuid,
    pub depends_on: Vec<Uuid>,
    pub priority: TaskPriority,
}

/// 全局任务管理器 Actor
pub struct DownloadManagerActor {
    pub config: Config,
    pub tasks: HashMap<Uuid, Addr<DownloadTaskActor>>,
    pub metas: HashMap<Uuid, DownloadTaskMeta>,
    pub semaphore: Arc<Semaphore>, // 并发控制
}

impl DownloadManagerActor {
    // 创建一个新的任务管理器
    pub fn new(config: Config) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_downloads));
        let mut mgr = Self {
            config,
            tasks: HashMap::new(),
            metas: HashMap::new(),
            semaphore,
        };
        mgr.load_tasks_from_file();
        mgr
    }
    pub fn save_tasks_to_file(&self) {
        let path = "downloads/tasks.json";
        if let Ok(json) = serde_json::to_string_pretty(&self.metas.values().collect::<Vec<_>>()) {
            let _ = fs::create_dir_all("downloads");
            let _ = fs::File::create(path).and_then(|mut f| f.write_all(json.as_bytes()));
        }
    }
    pub fn load_tasks_from_file(&mut self) {
        let path = "downloads/tasks.json";
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(list) = serde_json::from_str::<Vec<DownloadTaskMeta>>(&data) {
                for mut meta in list {
                    // 只恢复未完成任务
                    match meta.status {
                        TaskStatus::Pending | TaskStatus::Paused | TaskStatus::Running => {
                            let addr = DownloadTaskActor::new(self.config.clone(), meta.url.clone(), meta.file.clone()).start();
                            self.tasks.insert(meta.id, addr);
                        },
                        _ => {}
                    }
                    if meta.total == 0 { meta.total = 0; } // 兼容老数据
                    self.metas.insert(meta.id, meta);
                }
            }
        }
    }

    /// 获取所有任务统计信息
    pub fn get_stats(&self) -> TaskStats {
        let mut stats = TaskStats {
            total: self.metas.len(),
            running: 0,
            completed: 0,
            failed: 0,
            paused: 0,
        };
        for meta in self.metas.values() {
            match meta.status {
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Completed => stats.completed += 1,
                TaskStatus::Failed(_) => stats.failed += 1,
                TaskStatus::Paused => stats.paused += 1,
                _ => {}
            }
        }
        stats
    }

    /// 保存断点续传信息
    #[allow(dead_code)]
    pub fn save_resume_info(&self, resume_info: &ResumeInfo) -> Result<(), DownloadError> {
        let path = format!("downloads/resume_{}.json", resume_info.task_id);
        let json = serde_json::to_string_pretty(resume_info)
            .map_err(|e| DownloadError::Unknown(format!("序列化失败: {}", e)))?;
        
        std::fs::write(path, json)
            .map_err(|e| DownloadError::IoError(e.to_string()))?;
        Ok(())
    }
    
    /// 加载断点续传信息
    #[allow(dead_code)]
    pub fn load_resume_info(&self, task_id: Uuid) -> Option<ResumeInfo> {
        let path = format!("downloads/resume_{}.json", task_id);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    }
    
    /// 检查是否可以断点续传
    #[allow(dead_code)]
    pub fn can_resume(&self, task_id: Uuid) -> bool {
        self.load_resume_info(task_id).is_some()
    }

    /// 智能任务调度
    #[allow(dead_code)]
    pub fn schedule_tasks(&mut self) -> Vec<Uuid> {
        let mut ready_tasks = Vec::new();
        let mut pending_tasks = Vec::new();
        
        // 收集所有待处理任务
        for (id, meta) in &self.metas {
            if meta.status == TaskStatus::Pending {
                pending_tasks.push((*id, meta.clone()));
            }
        }
        
        // 按优先级排序
        pending_tasks.sort_by(|a, b| {
            // 这里可以根据实际需求实现更复杂的排序逻辑
            a.1.file.cmp(&b.1.file) // 简单按文件名排序
        });
        
        // 添加到就绪队列
        for (id, _) in pending_tasks {
            ready_tasks.push(id);
        }
        
        ready_tasks
    }
    
    /// 批量启动任务
    #[allow(dead_code)]
    pub fn start_multiple_tasks(&mut self, task_ids: Vec<Uuid>) {
        for task_id in task_ids {
            let _ = self.start_task_by_id(task_id);
        }
    }
    
    /// 内部启动任务方法
    fn start_task_by_id(&mut self, _task_id: Uuid) -> Result<(), DownloadError> {
        // 这里可以添加更多的启动逻辑
        Ok(())
    }

    /// 从 resume_*.json 文件加载并恢复任务
    fn load_tasks_from_resume_files(&mut self) {
        let resume_dir = "downloads/";
        if let Ok(entries) = fs::read_dir(resume_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() && path.to_str().map_or(false, |s| s.ends_with(".json") && s.contains("resume_")) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(resume_info) = serde_json::from_str::<ResumeInfo>(&content) {
                            // 避免重复添加已存在的任务
                            if self.tasks.contains_key(&resume_info.task_id) {
                                continue;
                            }

                            println!("[actor_manager] 正在恢复任务: {}", resume_info.task_id);

                            // 创建 Actor 和 Meta
                            let task_actor = DownloadTaskActor::new(
                                self.config.clone(), 
                                resume_info.url.clone(), 
                                resume_info.file.clone()
                            ).start();
                            
                            let meta = DownloadTaskMeta {
                                id: resume_info.task_id,
                                url: resume_info.url,
                                file: resume_info.file,
                                status: TaskStatus::Paused, // 恢复后默认为暂停状态
                                progress: 0.0, // 进度将在任务启动后更新
                                downloaded: 0, // 同样，将在启动后更新
                                total: resume_info.total_size,
                            };

                            self.tasks.insert(resume_info.task_id, task_actor);
                            self.metas.insert(resume_info.task_id, meta);
                        }
                    }
                }
            }
        }
    }
}

/// 任务统计信息
#[derive(Debug, Clone, Serialize)]
pub struct TaskStats {
    pub total: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub paused: usize,
}

impl Actor for DownloadManagerActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        if self.config.auto_resume_on_startup {
            println!("[actor_manager] 启动时自动恢复任务...");
            self.load_tasks_from_resume_files();
        }
    }
}

impl Handler<CreateTask> for DownloadManagerActor {
    type Result = Result<Uuid, DownloadError>;

    fn handle(&mut self, msg: CreateTask, _ctx: &mut Self::Context) -> Self::Result {
        let config = self.config.clone();
        let id = Uuid::new_v4();
        let actor = DownloadTaskActor::new(config, msg.url.clone(), msg.file.clone());
        let addr = actor.start();
        self.tasks.insert(id, addr);

        let meta = DownloadTaskMeta {
            id,
            url: msg.url,
            file: msg.file,
            status: TaskStatus::Pending,
            progress: 0.0,
            downloaded: 0,
            total: 0,
        };
        self.metas.insert(id, meta);
        self.save_tasks_to_file();
        Ok(id)
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct InternalStartTask {
    task_id: Uuid,
    permit: tokio::sync::OwnedSemaphorePermit,
}

impl Handler<StartTaskFromMeta> for DownloadManagerActor {
    type Result = ();

    fn handle(&mut self, msg: StartTaskFromMeta, ctx: &mut Self::Context) -> Self::Result {
        let sem = self.semaphore.clone();
        let addr = ctx.address();
        
        async move {
            let permit = sem.acquire_owned().await.unwrap();
            addr.do_send(InternalStartTask { task_id: msg.task_id, permit });
        }
        .into_actor(self)
        .spawn(ctx);
    }
}

impl Handler<InternalStartTask> for DownloadManagerActor {
    type Result = ();
    fn handle(&mut self, msg: InternalStartTask, ctx: &mut Self::Context) {
        if let Some(task_addr) = self.tasks.get(&msg.task_id) {
            if let Some(meta) = self.metas.get_mut(&msg.task_id) {
                meta.status = TaskStatus::Running;
            }
            task_addr.do_send(task_messages::StartTask {
                manager_addr: ctx.address(),
                permit: msg.permit,
            });
        }
    }
}

impl Handler<PauseTask> for DownloadManagerActor {
    type Result = ();

    fn handle(&mut self, msg: PauseTask, _ctx: &mut Self::Context) {
        if let Some(addr) = self.tasks.get(&msg.0) {
            addr.do_send(task_messages::PauseTask);
        }
    }
}

impl Handler<CancelTask> for DownloadManagerActor {
    type Result = ();

    fn handle(&mut self, msg: CancelTask, _ctx: &mut Self::Context) {
        if let Some(addr) = self.tasks.get(&msg.0) {
            if let Some(meta) = self.metas.get_mut(&msg.0) {
                meta.status = TaskStatus::Cancelled;
            }
            addr.do_send(task_messages::CancelTask);
        }
    }
}

impl Handler<QueryTaskProgress> for DownloadManagerActor {
    type Result = LocalBoxFuture<'static, Result<f32, ()>>;

    fn handle(&mut self, msg: QueryTaskProgress, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.tasks.get(&msg.0) {
            let addr = addr.clone();
            Box::pin(async move {
                match addr.send(task_messages::QueryProgress).await {
                    Ok(val) => Ok(val),
                    Err(_) => Err(()),
                }
            })
        } else {
            Box::pin(async { Err(()) })
        }
    }
}

impl Handler<QueryTaskStatus> for DownloadManagerActor {
    type Result = LocalBoxFuture<'static, Result<TaskStatus, ()>>;

    fn handle(&mut self, msg: QueryTaskStatus, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.tasks.get(&msg.0) {
            let addr = addr.clone();
            Box::pin(async move {
                addr.send(task_messages::QueryStatus)
                    .await
                    .unwrap_or(Err(()))
            })
        } else {
            Box::pin(async { Err(()) })
        }
    }
}

impl Handler<QueryTaskDetail> for DownloadManagerActor {
    type Result = Option<DownloadTaskMeta>;

    fn handle(&mut self, msg: QueryTaskDetail, _ctx: &mut Self::Context) -> Self::Result {
        self.metas.get(&msg.0).cloned()
    }
}

impl Handler<GetStats> for DownloadManagerActor {
    type Result = MessageResult<GetStats>;

    fn handle(&mut self, _msg: GetStats, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.get_stats())
    }
}

impl Handler<UpdateTaskProgress> for DownloadManagerActor {
    type Result = ();

    fn handle(&mut self, msg: UpdateTaskProgress, _ctx: &mut Self::Context) {
        if let Some(meta) = self.metas.get_mut(&msg.task_id) {
            meta.progress = msg.progress;
            meta.downloaded = msg.downloaded;
            meta.total = msg.total;
        }
    }
}

impl Handler<MarkTaskCompleted> for DownloadManagerActor {
    type Result = ();

    fn handle(&mut self, msg: MarkTaskCompleted, _ctx: &mut Self::Context) {
        if let Some(meta) = self.metas.get_mut(&msg.task_id) {
            meta.status = TaskStatus::Completed;
            meta.progress = 100.0;
            println!("[actor_manager] MarkTaskCompleted: 任务 {:?} 状态已设为 Completed", msg.task_id);
        }
        self.save_tasks_to_file();
    }
}

impl Handler<MarkTaskFailed> for DownloadManagerActor {
    type Result = ();

    fn handle(&mut self, msg: MarkTaskFailed, _ctx: &mut Self::Context) {
        if let Some(meta) = self.metas.get_mut(&msg.task_id) {
            meta.status = TaskStatus::Failed(msg.error.to_string());
        }
        self.save_tasks_to_file();
    }
} 