use actix::prelude::*;
use std::collections::HashMap;
use futures::FutureExt;
use crate::core::actor_task::{DownloadTaskActor, StartTask, PauseTask, CancelTask, QueryProgress, QueryStatus, TaskStatus};

/// 消息：添加下载任务
pub struct AddTask {
    pub urls: Vec<String>,
}
impl Message for AddTask {
    type Result = usize; // 返回任务ID
}

/// 消息：查询所有任务
pub struct ListTasks;
impl Message for ListTasks {
    type Result = Vec<usize>;
}

/// 消息：启动指定任务
pub struct StartTaskById {
    pub task_id: usize,
}
impl Message for StartTaskById {
    type Result = ();
}

/// 消息：暂停指定任务
pub struct PauseTaskById {
    pub task_id: usize,
}
impl Message for PauseTaskById {
    type Result = ();
}

/// 消息：取消指定任务
pub struct CancelTaskById {
    pub task_id: usize,
}
impl Message for CancelTaskById {
    type Result = ();
}

/// 消息：查询指定任务进度
pub struct QueryTaskProgressById {
    pub task_id: usize,
}
impl Message for QueryTaskProgressById {
    type Result = Option<f32>;
}

/// 消息：查询指定任务状态
pub struct QueryTaskStatusById {
    pub task_id: usize,
}
impl Message for QueryTaskStatusById {
    type Result = Option<TaskStatus>;
}

/// 全局任务管理器 Actor
pub struct DownloadManagerActor {
    pub task_counter: usize,
    pub tasks: HashMap<usize, Addr<DownloadTaskActor>>, // 存储任务ID和Actor地址
}

impl DownloadManagerActor {
    pub fn new() -> Self {
        Self {
            task_counter: 0,
            tasks: HashMap::new(),
        }
    }
}

impl Actor for DownloadManagerActor {
    type Context = Context<Self>;
}

impl Handler<AddTask> for DownloadManagerActor {
    type Result = usize;
    fn handle(&mut self, msg: AddTask, _ctx: &mut Self::Context) -> Self::Result {
        let id = self.task_counter;
        let task_actor = DownloadTaskActor::new(msg.urls).start();
        self.tasks.insert(id, task_actor);
        self.task_counter += 1;
        id
    }
}

impl Handler<ListTasks> for DownloadManagerActor {
    type Result = MessageResult<ListTasks>;
    fn handle(&mut self, _msg: ListTasks, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.tasks.keys().cloned().collect())
    }
}

impl Handler<StartTaskById> for DownloadManagerActor {
    type Result = ();
    fn handle(&mut self, msg: StartTaskById, _ctx: &mut Self::Context) {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            addr.do_send(StartTask);
        }
    }
}

impl Handler<PauseTaskById> for DownloadManagerActor {
    type Result = ();
    fn handle(&mut self, msg: PauseTaskById, _ctx: &mut Self::Context) {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            addr.do_send(PauseTask);
        }
    }
}

impl Handler<CancelTaskById> for DownloadManagerActor {
    type Result = ();
    fn handle(&mut self, msg: CancelTaskById, _ctx: &mut Self::Context) {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            addr.do_send(CancelTask);
        }
    }
}

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

impl Handler<QueryTaskStatusById> for DownloadManagerActor {
    type Result = ResponseFuture<Option<TaskStatus>>;
    fn handle(&mut self, msg: QueryTaskStatusById, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(addr) = self.tasks.get(&msg.task_id) {
            let fut = addr.send(QueryStatus).map(|res| match res {
                Ok(Ok(status)) => Some(status),
                _ => None,
            });
            Box::pin(fut)
        } else {
            Box::pin(async { None })
        }
    }
} 