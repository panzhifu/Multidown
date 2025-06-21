use std::time::Duration;
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
#[derive(Debug)]
#[allow(dead_code)]
pub struct RetryContext {
    pub strategy: RetryStrategy,
    pub retry_count: usize,
    pub last_error: Option<DownloadError>,
    pub retry_history: Vec<(DownloadError, Duration)>, // 记录重试历史
    pub total_retry_time: Duration,
}

#[allow(dead_code)]
impl RetryContext {
    pub fn new(strategy: RetryStrategy) -> Self {
        Self {
            strategy,
            retry_count: 0,
            last_error: None,
            retry_history: Vec::new(),
            total_retry_time: Duration::from_secs(0),
        }
    }
    
    pub fn should_retry(&self, error: &DownloadError) -> bool {
        self.strategy.should_retry(error, self.retry_count)
    }
    
    pub fn increment_retry(&mut self, error: DownloadError) {
        self.retry_count += 1;
        self.last_error = Some(error.clone());
        
        let delay = self.strategy.get_delay(self.retry_count);
        self.retry_history.push((error, delay));
        self.total_retry_time += delay;
    }
    
    pub fn get_delay(&self) -> Duration {
        self.strategy.get_delay(self.retry_count)
    }
    
    pub fn get_retry_stats(&self) -> RetryStats {
        RetryStats {
            total_retries: self.retry_count,
            total_retry_time: self.total_retry_time,
            retry_history: self.retry_history.clone(),
        }
    }
    
    pub fn reset(&mut self) {
        self.retry_count = 0;
        self.last_error = None;
        self.retry_history.clear();
        self.total_retry_time = Duration::from_secs(0);
    }
}

/// 重试统计信息
#[derive(Debug, Clone)]
pub struct RetryStats {
    pub total_retries: usize,
    pub total_retry_time: Duration,
    pub retry_history: Vec<(DownloadError, Duration)>,
} 