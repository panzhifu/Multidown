use serde::{Serialize, Deserialize};

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