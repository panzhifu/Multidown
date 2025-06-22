use std::time::{Duration, Instant};
use crate::core::error::DownloadError;

/// 重试策略
#[derive(Debug, Clone)]
pub struct RetryStrategy {
    pub max_retries: usize,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub jitter_factor: f64, // 添加抖动因子避免重试风暴
    pub retryable_errors: Vec<String>,
}

#[allow(dead_code)]
impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1, // 10% 的抖动
            retryable_errors: vec![
                "network error".to_string(),
                "timeout".to_string(),
                "connection reset".to_string(),
                "temporary failure".to_string(),
                "connection refused".to_string(),
                "connection timeout".to_string(),
                "dns resolution failed".to_string(),
                "ssl error".to_string(),
                "certificate error".to_string(),
                "server error".to_string(),
                "gateway timeout".to_string(),
                "service unavailable".to_string(),
            ],
        }
    }
}

impl RetryStrategy {
    pub fn should_retry(&self, error: &DownloadError, retry_count: usize) -> bool {
        if retry_count >= self.max_retries {
            return false;
        }
        
        // 根据错误类型判断是否可重试
        match error {
            DownloadError::NetworkError(_) => true,
            DownloadError::ServerError(msg) => {
                // 服务器错误中，5xx 错误通常可以重试
                msg.contains("500") || msg.contains("502") || msg.contains("503") || 
                msg.contains("504") || msg.contains("507") || msg.contains("508")
            },
            DownloadError::Timeout => true,
            DownloadError::IoError(_) => {
                // IO错误中，网络相关的可以重试
                let error_str = error.to_string().to_lowercase();
                self.retryable_errors.iter().any(|retryable| {
                    error_str.contains(retryable)
                })
            },
            DownloadError::SizeMismatch { .. } => false, // 大小不匹配不重试
            DownloadError::InvalidUrl(_) => false, // URL无效不重试
            DownloadError::FileExists(_) => false, // 文件已存在不重试
            DownloadError::ResumeFailed(_) => false, // 断点续传失败不重试
            DownloadError::Unknown(msg) => {
                // 未知错误中，网络相关的可以重试
                let error_str = msg.to_lowercase();
                self.retryable_errors.iter().any(|retryable| {
                    error_str.contains(retryable)
                })
            },
            _ => false, // 其他错误不重试
        }
    }
    
    pub fn get_delay(&self, retry_count: usize) -> Duration {
        let delay_secs = self.base_delay.as_secs_f64() * 
            self.backoff_multiplier.powi(retry_count as i32);
        
        // 添加抖动避免重试风暴
        let jitter = delay_secs * self.jitter_factor * (rand::random::<f64>() - 0.5);
        let final_delay = delay_secs + jitter;
        
        let delay = Duration::from_secs_f64(final_delay.max(0.1)); // 最小延迟100ms
        delay.min(self.max_delay)
    }
}

/// 重试上下文
#[derive(Debug, Clone)]
pub struct RetryContext {
    pub max_retries: u32,
    pub current_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub last_retry_time: Option<Instant>,
}

impl RetryContext {
    pub fn new(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries,
            current_retries: 0,
            base_delay,
            max_delay,
            last_retry_time: None,
        }
    }

    /// 判断是否应该重试
    pub fn should_retry(&self, error: &DownloadError) -> bool {
        if self.current_retries >= self.max_retries {
            return false;
        }

        // 检查错误类型是否可重试
        self.is_retryable_error(error)
    }

    /// 判断错误是否可重试
    fn is_retryable_error(&self, error: &DownloadError) -> bool {
        // 定义可重试的错误类型
        static RETRYABLE_ERRORS: &[&str] = &[
            "network error",
            "timeout",
            "connection reset",
            "temporary failure",
            "connection refused",
            "connection timeout",
            "dns resolution failed",
            "ssl error",
            "certificate error",
            "server error",
            "gateway timeout",
            "service unavailable",
        ];

        let error_str = error.to_string().to_lowercase();
        
        // 检查错误消息是否包含可重试的错误类型
        RETRYABLE_ERRORS.iter().any(|&retryable| {
            error_str.contains(retryable)
        })
    }

    /// 获取下次重试延迟
    pub fn get_next_delay(&self) -> Duration {
        let delay = self.base_delay * 2_u32.pow(self.current_retries);
        delay.min(self.max_delay)
    }

    /// 记录重试
    pub fn record_retry(&mut self) {
        self.current_retries += 1;
        self.last_retry_time = Some(Instant::now());
    }

    /// 重置重试计数
    pub fn reset(&mut self) {
        self.current_retries = 0;
        self.last_retry_time = None;
    }

    /// 获取当前重试次数
    pub fn current_retries(&self) -> u32 {
        self.current_retries
    }

    /// 检查是否达到最大重试次数
    pub fn is_max_retries_reached(&self) -> bool {
        self.current_retries >= self.max_retries
    }
}

/// 重试统计信息
#[derive(Debug, Clone)]
pub struct RetryStats {
    pub total_retries: usize,
    pub total_retry_time: Duration,
    pub retry_history: Vec<(DownloadError, Duration)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_context() {
        let mut context = RetryContext::new(
            3,
            Duration::from_secs(1),
            Duration::from_secs(10)
        );

        // 测试初始状态
        assert_eq!(context.current_retries(), 0);
        assert!(!context.is_max_retries_reached());

        // 测试重试记录
        context.record_retry();
        assert_eq!(context.current_retries(), 1);

        // 测试重置
        context.reset();
        assert_eq!(context.current_retries(), 0);
    }

    #[test]
    fn test_retryable_errors() {
        let context = RetryContext::new(3, Duration::from_secs(1), Duration::from_secs(10));

        // 测试可重试的错误
        let retryable_error = DownloadError::network_error("network error occurred");
        assert!(context.should_retry(&retryable_error));

        // 测试不可重试的错误
        let non_retryable_error = DownloadError::invalid_url("invalid url");
        assert!(!context.should_retry(&non_retryable_error));
    }
} 