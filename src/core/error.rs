use thiserror::Error;
use std::io;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("网络错误: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("IO错误: {0}")]
    IoError(#[from] io::Error),

    #[error("无效的URL: {0}")]
    InvalidUrl(String),

    #[error("文件已存在: {0}")]
    FileExists(String),

    #[error("下载超时")]
    Timeout,

    #[error("下载被取消")]
    Cancelled,

    #[error("下载暂停")]
    Paused,

    #[error("重试次数超过限制: {0}")]
    MaxRetriesExceeded(u32),

    #[error("文件大小不匹配: 预期 {expected} 字节, 实际 {actual} 字节")]
    SizeMismatch {
        expected: u64,
        actual: u64,
    },

    #[error("校验和不匹配: 预期 {expected}, 实际 {actual}")]
    ChecksumMismatch {
        expected: String,
        actual: String,
    },

    #[error("不支持的协议: {0}")]
    UnsupportedProtocol(String),

    #[error("服务器错误: {0}")]
    ServerError(String),

    #[error("权限错误: {0}")]
    PermissionError(String),

    #[error("磁盘空间不足: 需要 {required} 字节, 可用 {available} 字节")]
    InsufficientSpace {
        required: u64,
        available: u64,
    },

    #[error("未知错误: {0}")]
    Unknown(String),
}

#[allow(dead_code)]
impl DownloadError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DownloadError::NetworkError(_) |
            DownloadError::Timeout |
            DownloadError::ServerError(_)
        )
    }

    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            DownloadError::InvalidUrl(_) |
            DownloadError::UnsupportedProtocol(_) |
            DownloadError::PermissionError(_) |
            DownloadError::InsufficientSpace { .. }
        )
    }

    pub fn to_string(&self) -> String {
        match self {
            DownloadError::NetworkError(e) => format!("网络错误: {}", e),
            DownloadError::IoError(e) => format!("IO错误: {}", e),
            DownloadError::InvalidUrl(url) => format!("无效的URL: {}", url),
            DownloadError::FileExists(path) => format!("文件已存在: {}", path),
            DownloadError::Timeout => "下载超时".to_string(),
            DownloadError::Cancelled => "下载被取消".to_string(),
            DownloadError::Paused => "下载暂停".to_string(),
            DownloadError::MaxRetriesExceeded(count) => format!("重试次数超过限制: {}", count),
            DownloadError::SizeMismatch { expected, actual } => {
                format!("文件大小不匹配: 预期 {} 字节, 实际 {} 字节", expected, actual)
            },
            DownloadError::ChecksumMismatch { expected, actual } => {
                format!("校验和不匹配: 预期 {}, 实际 {}", expected, actual)
            },
            DownloadError::UnsupportedProtocol(proto) => format!("不支持的协议: {}", proto),
            DownloadError::ServerError(msg) => format!("服务器错误: {}", msg),
            DownloadError::PermissionError(msg) => format!("权限错误: {}", msg),
            DownloadError::InsufficientSpace { required, available } => {
                format!("磁盘空间不足: 需要 {} 字节, 可用 {} 字节", required, available)
            },
            DownloadError::Unknown(msg) => format!("未知错误: {}", msg),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        let network_error = DownloadError::NetworkError(
            reqwest::Error::status(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
        );
        assert!(network_error.is_retryable());

        let timeout_error = DownloadError::Timeout;
        assert!(timeout_error.is_retryable());

        let server_error = DownloadError::ServerError("500 Internal Server Error".to_string());
        assert!(server_error.is_retryable());
    }

    #[test]
    fn test_error_fatal() {
        let invalid_url = DownloadError::InvalidUrl("invalid://url".to_string());
        assert!(invalid_url.is_fatal());

        let unsupported_protocol = DownloadError::UnsupportedProtocol("ftp".to_string());
        assert!(unsupported_protocol.is_fatal());

        let permission_error = DownloadError::PermissionError("Access denied".to_string());
        assert!(permission_error.is_fatal());
    }

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

#[allow(dead_code)]
pub type DownloadResult<T> = Result<T, DownloadError>; 