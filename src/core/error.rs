use thiserror::Error;
use actix::prelude::*;
use anyhow;
use std::borrow::Cow;
use crate::utils::logger::LoggerExt;

/// 下载相关错误类型
#[derive(Error, Debug, Clone)]
pub enum DownloadError {
    // ===== 网络与IO =====
    #[error("网络错误: {0}")]
    NetworkError(Cow<'static, str>),
    #[error("IO错误: {0}")]
    IoError(Cow<'static, str>),
    // ===== 协议与参数 =====
    #[error("无效的URL: {0}")]
    InvalidUrl(Cow<'static, str>),
    #[error("不支持的协议: {0}")]
    UnsupportedProtocol(Cow<'static, str>),
    // ===== 文件与资源 =====
    #[error("文件已存在: {0}")]
    FileExists(Cow<'static, str>),
    #[error("磁盘空间不足: 需要 {required} 字节, 可用 {available} 字节")]
    #[allow(dead_code)]
    InsufficientSpace { required: u64, available: u64 },
    #[error("权限错误: {0}")]
    PermissionError(Cow<'static, str>),
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
    ServerError(Cow<'static, str>),
    // ===== actix相关 =====
    #[error("Actix邮箱错误: {0}")]
    MailboxError(Cow<'static, str>),
    #[error("Actix消息发送失败: {0}")]
    SendError(Cow<'static, str>),
    // ===== 其它 =====
    #[error("未知错误: {0}")]
    Unknown(Cow<'static, str>),
    #[error("续传失败: {0}")]
    ResumeFailed(Cow<'static, str>),
}

// 兼容actix错误类型
impl From<MailboxError> for DownloadError {
    fn from(e: MailboxError) -> Self {
        DownloadError::MailboxError(Cow::Owned(e.to_string()))
    }
}
impl<T> From<SendError<T>> for DownloadError {
    fn from(e: SendError<T>) -> Self {
        DownloadError::SendError(Cow::Owned(e.to_string()))
    }
}

impl From<String> for DownloadError {
    fn from(error: String) -> Self {
        DownloadError::Unknown(Cow::Owned(error))
    }
}
impl From<&'static str> for DownloadError {
    fn from(error: &'static str) -> Self {
        DownloadError::Unknown(Cow::Borrowed(error))
    }
}

impl From<anyhow::Error> for DownloadError {
    fn from(error: anyhow::Error) -> Self {
        DownloadError::Unknown(Cow::Owned(error.to_string()))
    }
}

// 便捷构造函数
impl DownloadError {
    /// 创建网络错误
    pub fn network_error(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::NetworkError(msg.into())
    }
    
    /// 创建IO错误
    pub fn io_error(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::IoError(msg.into())
    }
    
    /// 创建无效URL错误
    pub fn invalid_url(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::InvalidUrl(msg.into())
    }
    
    /// 创建文件存在错误
    pub fn file_exists(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::FileExists(msg.into())
    }
    
    /// 创建权限错误
    pub fn permission_error(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::PermissionError(msg.into())
    }
    
    /// 创建服务器错误
    pub fn server_error(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::ServerError(msg.into())
    }
    
    /// 创建未知错误
    pub fn unknown(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::Unknown(msg.into())
    }
    
    /// 创建续传失败错误
    pub fn resume_failed(msg: impl Into<Cow<'static, str>>) -> Self {
        DownloadError::ResumeFailed(msg.into())
    }

    // ===== 新增优化方法 =====

    /// 创建带上下文的IO错误
    pub fn io_error_with_context(context: &str, error: impl std::error::Error) -> Self {
        DownloadError::IoError(format!("{}: {}", context, error).into())
    }
    
    /// 创建带上下文的网络错误
    pub fn network_error_with_context(context: &str, error: impl std::error::Error) -> Self {
        DownloadError::NetworkError(format!("{}: {}", context, error).into())
    }
    
    /// 创建带上下文的服务器错误
    pub fn server_error_with_context(context: &str, status: u16) -> Self {
        DownloadError::ServerError(format!("{}: HTTP {}", context, status).into())
    }

    /// 判断错误是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(self, 
            DownloadError::NetworkError(_) |
            DownloadError::Timeout |
            DownloadError::ServerError(_) |
            DownloadError::IoError(_) // 某些IO错误可能可重试
        )
    }
    
    /// 判断错误是否为致命错误（不可重试）
    pub fn is_fatal(&self) -> bool {
        matches!(self,
            DownloadError::InvalidUrl(_) |
            DownloadError::FileExists(_) |
            DownloadError::SizeMismatch { .. } |
            DownloadError::ChecksumMismatch { .. } |
            DownloadError::ResumeFailed(_) |
            DownloadError::PermissionError(_) |
            DownloadError::InsufficientSpace { .. }
        )
    }

    /// 判断错误是否为临时错误（可重试）
    pub fn is_temporary(&self) -> bool {
        matches!(self,
            DownloadError::Timeout |
            DownloadError::NetworkError(_) |
            DownloadError::ServerError(_)
        )
    }

    /// 获取错误严重程度
    pub fn severity(&self) -> ErrorSeverity {
        if self.is_fatal() {
            ErrorSeverity::Fatal
        } else if self.is_temporary() {
            ErrorSeverity::Temporary
        } else {
            ErrorSeverity::Retryable
        }
    }

    /// 记录错误到日志并返回自身（链式调用）
    pub fn log_and_return(self, logger: &Addr<crate::utils::logger::LoggerActor>) -> Self {
        let severity = self.severity();
        let message = format!("[{}] {}", severity, self);
        
        match severity {
            ErrorSeverity::Fatal => logger.error(&message),
            ErrorSeverity::Temporary => logger.warn(&message),
            ErrorSeverity::Retryable => logger.info(&message),
        }
        
        self
    }

    /// 获取错误建议的解决方案
    pub fn get_suggestion(&self) -> Option<&'static str> {
        match self {
            DownloadError::NetworkError(_) => Some("检查网络连接，稍后重试"),
            DownloadError::Timeout => Some("网络超时，请检查网络连接或增加超时时间"),
            DownloadError::ServerError(_) => Some("服务器暂时不可用，请稍后重试"),
            DownloadError::InvalidUrl(_) => Some("请检查URL格式是否正确"),
            DownloadError::FileExists(_) => Some("文件已存在，请删除或重命名"),
            DownloadError::PermissionError(_) => Some("权限不足，请检查文件权限或使用管理员权限"),
            DownloadError::InsufficientSpace { .. } => Some("磁盘空间不足，请清理磁盘空间"),
            DownloadError::SizeMismatch { .. } => Some("文件大小不匹配，可能是下载不完整"),
            DownloadError::ResumeFailed(_) => Some("断点续传失败，将重新下载"),
            _ => None,
        }
    }
}

/// 错误严重程度
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorSeverity {
    /// 致命错误，不可重试
    Fatal,
    /// 临时错误，可重试
    Temporary,
    /// 可重试错误
    Retryable,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSeverity::Fatal => write!(f, "FATAL"),
            ErrorSeverity::Temporary => write!(f, "TEMP"),
            ErrorSeverity::Retryable => write!(f, "RETRY"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        // 测试静态字符串
        let error = DownloadError::unknown("测试错误");
        assert!(matches!(error, DownloadError::Unknown(Cow::Borrowed("测试错误"))));
        
        // 测试动态字符串
        let error = DownloadError::unknown("测试错误".to_string());
        assert!(matches!(error, DownloadError::Unknown(Cow::Owned(_))));
    }

    #[test]
    fn test_error_classification() {
        // 测试可重试错误
        let network_error = DownloadError::network_error("连接失败");
        assert!(network_error.is_retryable());
        assert!(!network_error.is_fatal());
        assert!(network_error.is_temporary());

        // 测试致命错误
        let invalid_url = DownloadError::invalid_url("无效URL");
        assert!(!invalid_url.is_retryable());
        assert!(invalid_url.is_fatal());
        assert!(!invalid_url.is_temporary());

        // 测试临时错误
        let timeout = DownloadError::Timeout;
        assert!(timeout.is_retryable());
        assert!(!timeout.is_fatal());
        assert!(timeout.is_temporary());
    }

    #[test]
    fn test_error_severity() {
        assert_eq!(DownloadError::network_error("test").severity(), ErrorSeverity::Temporary);
        assert_eq!(DownloadError::invalid_url("test").severity(), ErrorSeverity::Fatal);
        assert_eq!(DownloadError::IoError("test".into()).severity(), ErrorSeverity::Retryable);
    }

    #[test]
    fn test_error_suggestions() {
        assert!(DownloadError::network_error("test").get_suggestion().is_some());
        assert!(DownloadError::invalid_url("test").get_suggestion().is_some());
        assert!(DownloadError::Timeout.get_suggestion().is_some());
    }

    #[test]
    fn test_error_with_context() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "文件不存在");
        let error = DownloadError::io_error_with_context("写入文件", io_error);
        assert!(matches!(error, DownloadError::IoError(_)));
        assert!(error.to_string().contains("写入文件"));
    }
} 