use actix::prelude::*;
use std::collections::HashMap;
use futures::FutureExt;
use crate::core::actor_task::{DownloadTaskActor, StartTask, PauseTask, CancelTask, QueryProgress, TaskStatus};
use serde::{Serialize, Deserialize};
use std::fs;
use std::io::Write;
use uuid::Uuid;
use crate::core::error::DownloadError;
use crate::config::Config;

/// ================== 任务元数据结构体 ==================
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DownloadTaskMeta {
    pub id: Uuid,
    pub url: String,
    pub file: String,
    pub tags: Vec<String>,
    pub status: TaskStatus,
    pub progress: f32,
    pub size: u64,
}

/// 添加下载任务
pub struct AddTask {
    pub url: String,
    pub file: String,
    pub tags: Vec<String>,
}
impl Message for AddTask { type Result = Result<Uuid, DownloadError>; }
/// 处理添加下载任务消息
impl Handler<AddTask> for DownloadManagerActor {
    type Result = Result<Uuid, DownloadError>;
    fn handle(&mut self, msg: AddTask, _ctx: &mut Self::Context) -> Self::Result {
        let id = Uuid::new_v4();
        let task_actor = DownloadTaskActor::new(self.config.clone(), msg.url.clone(), msg.file.clone()).start();
        self.tasks.insert(id, task_actor);
        let meta = DownloadTaskMeta {
            id,
            url: msg.url,
            file: msg.file,
            tags: msg.tags,
            status: TaskStatus::Pending,
            progress: 0.0,
            size: 0,
        };
        self.metas.insert(id, meta);
        self.save_tasks_to_file();
        Ok(id)
    }
}

/// 启动指定任务
pub struct StartTaskById { pub task_id: Uuid }
impl Message for StartTaskById { type Result = Result<(), DownloadError>; }
impl Handler<StartTaskById> for DownloadManagerActor {
    type Result = Result<(), DownloadError>;
    fn handle(&mut self, msg: StartTaskById, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            addr.do_send(StartTask);
            Ok(())
        } else {
            Err(DownloadError::Unknown(format!("任务ID不存在: {}", msg.task_id)))
        }
    }
}

/// 暂停指定任务
pub struct PauseTaskById { pub task_id: Uuid }
impl Message for PauseTaskById { type Result = Result<(), DownloadError>; }
impl Handler<PauseTaskById> for DownloadManagerActor {
    type Result = Result<(), DownloadError>;
    fn handle(&mut self, msg: PauseTaskById, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            addr.do_send(PauseTask);
            Ok(())
        } else {
            Err(DownloadError::Unknown(format!("任务ID不存在: {}", msg.task_id)))
        }
    }
}

/// 取消指定任务
pub struct CancelTaskById { pub task_id: Uuid }
impl Message for CancelTaskById { type Result = (); }
impl Handler<CancelTaskById> for DownloadManagerActor {
    type Result = ();
    fn handle(&mut self, msg: CancelTaskById, _ctx: &mut Self::Context) {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            addr.do_send(CancelTask);
        }
    }
}

/// 查询所有任务ID
pub struct ListTasks;
impl Message for ListTasks { type Result = Vec<Uuid>; }
impl Handler<ListTasks> for DownloadManagerActor {
    type Result = MessageResult<ListTasks>;
    fn handle(&mut self, _msg: ListTasks, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.tasks.keys().cloned().collect())
    }
}

/// 查询指定任务进度百分比
pub struct QueryTaskProgressById { pub task_id: Uuid }
impl Message for QueryTaskProgressById { type Result = Option<f32>; }
impl Handler<QueryTaskProgressById> for DownloadManagerActor {
    type Result = ResponseFuture<Option<f32>>;
    fn handle(&mut self, msg: QueryTaskProgressById, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            let fut = addr.send(QueryProgress).map(|res| res.ok());
            Box::pin(fut)
        } else {
            Box::pin(async { None })
        }
    }
}

/// 查询指定任务状态
pub struct QueryTaskStatusById { pub task_id: Uuid }
impl Message for QueryTaskStatusById { type Result = Result<Option<TaskStatus>, DownloadError>; }
impl Handler<QueryTaskStatusById> for DownloadManagerActor {
    type Result = Result<Option<TaskStatus>, DownloadError>;
    fn handle(&mut self, msg: QueryTaskStatusById, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(meta) = self.metas.get(&msg.task_id) {
            Ok(Some(meta.status.clone()))
        } else {
            Err(DownloadError::Unknown(format!("任务ID不存在: {}", msg.task_id)))
        }
    }
}

/// 查询指定任务元数据
pub struct QueryTaskMetaById { pub task_id: Uuid }
impl Message for QueryTaskMetaById { type Result = Option<DownloadTaskMeta>; }
impl Handler<QueryTaskMetaById> for DownloadManagerActor {
    type Result = Option<DownloadTaskMeta>;
    fn handle(&mut self, msg: QueryTaskMetaById, _ctx: &mut Self::Context) -> Self::Result {
        self.metas.get(&msg.task_id).cloned()
    }
}

/// 查询指定任务详细进度
pub struct QueryTaskDetailById { pub task_id: Uuid }
impl Message for QueryTaskDetailById { type Result = Result<Option<DownloadTaskMeta>, DownloadError>; }
impl Handler<QueryTaskDetailById> for DownloadManagerActor {
    type Result = Result<Option<DownloadTaskMeta>, DownloadError>;
    fn handle(&mut self, msg: QueryTaskDetailById, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(meta) = self.metas.get(&msg.task_id) {
            Ok(Some(meta.clone()))
        } else {
            Err(DownloadError::Unknown(format!("任务ID不存在: {}", msg.task_id)))
        }
    }
}

/// 移除指定任务
pub struct RemoveTaskById { pub task_id: Uuid }
impl Message for RemoveTaskById { type Result = Result<bool, DownloadError>; }
impl Handler<RemoveTaskById> for DownloadManagerActor {
    type Result = Result<bool, DownloadError>;
    fn handle(&mut self, msg: RemoveTaskById, _ctx: &mut Self::Context) -> Self::Result {
        let removed = self.tasks.remove(&msg.task_id).is_some();
        self.metas.remove(&msg.task_id);
        self.save_tasks_to_file();
        Ok(removed)
    }
}

/// 消息类型，用于添加下载任务
impl Actor for DownloadManagerActor {
    type Context = Context<Self>;
}

/// 全局任务管理器 Actor
pub struct DownloadManagerActor {
    pub config: Config,
    pub tasks: HashMap<Uuid, Addr<DownloadTaskActor>>,
    pub metas: HashMap<Uuid, DownloadTaskMeta>,
}

impl DownloadManagerActor {
    // 创建一个新的任务管理器
    pub fn new(config: Config) -> Self {
        let mut mgr = Self {
            config,
            tasks: HashMap::new(),
            metas: HashMap::new(),
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
                    if meta.size == 0 { meta.size = 0; } // 兼容老数据
                    self.metas.insert(meta.id, meta);
                }
            }
        }
    }
} 