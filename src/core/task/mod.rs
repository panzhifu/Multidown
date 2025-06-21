//! `task` 模块包含了与单个下载任务相关的所有逻辑
//!
//! 主要包括：
//! - `actor`: `DownloadTaskActor` 的定义
//! - `state`: 任务状态 `TaskStatus`
//! - `messages`: Actor 之间传递的消息
//! - `handlers`: 消息处理器
//! - `download`: 实际的下载逻辑
//! - `chunk_manager`: 分块下载管理器
//! - `retry`: 重试逻辑
//! - `util`: 工具类，如 `BufferManager`

pub mod actor;
pub mod state;
pub mod messages;
pub mod handlers;
pub mod download;
pub mod chunk_manager;
pub mod retry;
pub mod util;

// 导出核心组件，方便外部使用
pub use actor::DownloadTaskActor;
pub use messages::{StartTask, PauseTask, CancelTask};
pub use state::TaskStatus;
pub use self::util::{FileInfo, BufferManager};
pub use self::retry::{RetryStrategy, RetryContext, RetryStats}; 