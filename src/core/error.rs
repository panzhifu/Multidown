use thiserror::Error;
use actix::prelude::*;
use anyhow;

/// 下载相关错误类型
#[derive(Error, Debug, Clone)]
pub enum DownloadError {
    // ===== 网络与IO =====
    #[error("网络错误: {0}")]
    NetworkError(String),
    #[error("IO错误: {0}")]
    IoError(String),
    // ===== 协议与参数 =====
    #[error("无效的URL: {0}")]
    InvalidUrl(String),
    #[error("不支持的协议: {0}")]
    UnsupportedProtocol(String),
    // ===== 文件与资源 =====
    #[error("文件已存在: {0}")]
    FileExists(String),
    #[error("磁盘空间不足: 需要 {required} 字节, 可用 {available} 字节")]
    #[allow(dead_code)]
    InsufficientSpace { required: u64, available: u64 },
    #[error("权限错误: {0}")]
    PermissionError(String),
    // ===== 下载流程 =====
    #[error("下载超时")]
    #[allow(dead_code)]
    Timeout,
    #[error("下载被取消")]
    Cancelled,
    #[error("下载暂停")]
    #[allow(dead_code)]
    Paused,
    #[error("重试次数超过限制: {0}")]
    #[allow(dead_code)]
    MaxRetriesExceeded(u32),
    #[error("文件大小不匹配: 预期 {expected} 字节, 实际 {actual} 字节")]
    #[allow(dead_code)]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("校验和不匹配: 预期 {expected}, 实际 {actual}")]
    #[allow(dead_code)]
    ChecksumMismatch { expected: String, actual: String },
    #[error("服务器错误: {0}")]
    ServerError(String),
    // ===== actix相关 =====
    #[error("Actix邮箱错误: {0}")]
    MailboxError(String),
    #[error("Actix消息发送失败: {0}")]
    SendError(String),
    // ===== 其它 =====
    #[error("未知错误: {0}")]
    Unknown(String),
    #[error("续传失败: {0}")]
    ResumeFailed(String),
}

// 兼容actix错误类型
impl From<MailboxError> for DownloadError {
    fn from(e: MailboxError) -> Self {
        DownloadError::MailboxError(e.to_string())
    }
}
impl<T> From<SendError<T>> for DownloadError {
    fn from(e: SendError<T>) -> Self {
        DownloadError::SendError(e.to_string())
    }
}

impl From<String> for DownloadError {
    fn from(error: String) -> Self {
        DownloadError::Unknown(error)
    }
}
impl From<&str> for DownloadError {
    fn from(error: &str) -> Self {
        DownloadError::Unknown(error.to_string())
    }
}

impl From<anyhow::Error> for DownloadError {
    fn from(error: anyhow::Error) -> Self {
        DownloadError::Unknown(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_error_conversion() {
        let error_str = "测试错误";
        let error: DownloadError = error_str.into();
        assert!(matches!(error, DownloadError::Unknown(_)));
        let error_string = "测试错误".to_string();
        let error: DownloadError = error_string.into();
        assert!(matches!(error, DownloadError::Unknown(_)));
    }
} 