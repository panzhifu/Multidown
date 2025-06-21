//! Core: 下载任务的actor管理、任务调度、错误处理等核心逻辑模块

pub mod actor_manager;
pub mod actor_task;
pub mod error;

// 只导出主流程和其它模块实际用到的类型
pub use actor_manager::{
    DownloadManagerActor, AddTask, StartTaskById, QueryTaskStatusById, QueryTaskDetailById
};
pub use actor_task::{
    TaskStatus
}; 