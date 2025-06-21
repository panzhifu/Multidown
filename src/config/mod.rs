use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::{Result};
use std::collections::HashMap;
use crate::core::error::DownloadError;

/// 全局配置结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// 最大并发下载数
    pub max_concurrent_downloads: usize,
    /// 每个任务默认线程数
    pub default_threads: usize,
    /// 默认速度限制（MB/s）
    pub default_speed_limit: f32,
    /// 下载速度限制（KB/s），0 表示不限速
    pub speed_limit_kb: u64,
    /// 默认输出目录
    pub default_output_dir: String,
    /// 下载重试次数
    pub retry_count: usize,
    /// 下载重试间隔（秒）
    pub retry_delay: u64,
    /// 重试最大延迟（秒）
    pub retry_max_delay: u64,
    /// 重试指数退避倍数
    pub retry_backoff_multiplier: f64,
    /// 重试抖动因子
    pub retry_jitter_factor: f64,
    /// 可重试错误关键字
    pub retryable_errors: Vec<String>,
    
    /// 网络设置
    pub timeout: u64,
    /// User-Agent
    pub user_agent: String,
    /// 代理设置
    pub proxy: Option<String>,
    /// 是否校验SSL
    pub verify_ssl: bool,
    
    /// 文件设置
    pub auto_rename: bool,
    /// 是否覆盖已存在文件
    pub overwrite_existing: bool,
    /// 自动创建目录
    pub create_directories: bool,
    
    /// 通知设置
    pub enable_notifications: bool,
    /// 通知声音
    pub notification_sound: bool,
    
    /// 界面设置
    pub show_progress_bar: bool,
    pub show_speed: bool,
    pub show_eta: bool,
    pub show_size: bool,
    
    /// 高级设置
    pub chunk_size: usize,
    pub buffer_size: usize,
    pub max_redirects: usize,
    pub custom_headers: HashMap<String, String>,
    
    /// 分块下载设置
    pub enable_chunked_download: bool,
    pub max_chunks_per_file: usize,
    pub min_chunk_size: usize,
    pub chunk_timeout: u64,
    
    /// 断点续传设置
    pub enable_resume: bool,
    pub resume_check_interval: u64,
    pub auto_resume_on_startup: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // 下载设置
            max_concurrent_downloads: 3,
            default_threads: 4,
            default_speed_limit: 10.0,
            speed_limit_kb: 0,
            default_output_dir: "./downloads".to_string(),
            retry_count: 3,
            retry_delay: 5,
            retry_max_delay: 60,
            retry_backoff_multiplier: 2.0,
            retry_jitter_factor: 0.1,
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
            
            // 网络设置
            timeout: 30,
            user_agent: "MultiDown/1.0".to_string(),
            proxy: None,
            verify_ssl: true,
            
            // 文件设置
            auto_rename: true,
            overwrite_existing: false,
            create_directories: true,
            
            // 通知设置
            enable_notifications: true,
            notification_sound: true,
            
            // 界面设置
            show_progress_bar: true,
            show_speed: true,
            show_eta: true,
            show_size: true,
            
            // 高级设置
            chunk_size: 8192,
            buffer_size: 16384,
            max_redirects: 5,
            custom_headers: HashMap::new(),
            
            // 分块下载设置
            enable_chunked_download: true,
            max_chunks_per_file: 10,
            min_chunk_size: 1024,
            chunk_timeout: 10,
            
            // 断点续传设置
            enable_resume: true,
            resume_check_interval: 60,
            auto_resume_on_startup: true,
        }
    }
}

impl Config {
    /// 加载配置文件
    pub fn load(path: &str) -> Result<Self, DownloadError> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)
                .map_err(|e| DownloadError::IoError(e.to_string()))?;
            
            // 尝试解析TOML
            match toml::from_str(&content) {
                Ok(config) => Ok(config),
                Err(e) => {
                    // 如果解析失败，创建新的默认配置
                    log::warn!("配置文件格式错误: {}，将使用默认配置", e);
                    let config = Config::default();
                    config.save(path)?;
                    Ok(config)
                }
            }
        } else {
            // 如果文件不存在，创建默认配置
            let config = Config::default();
            config.save(path)?;
            Ok(config)
        }
    }

    /// 保存配置文件
    pub fn save(&self, path: &str) -> Result<(), DownloadError> {
        // 确保目录存在
        if let Some(parent) = Path::new(path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| DownloadError::IoError(e.to_string()))?;
            }
        }

        // 序列化为TOML
        let content = toml::to_string_pretty(self)
            .map_err(|e| DownloadError::Unknown(format!("无法序列化配置: {}", e)))?;
        
        // 写入文件
        fs::write(path, content)
            .map_err(|e| DownloadError::IoError(e.to_string()))?;
        
        Ok(())
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), DownloadError> {
        // 验证并发下载数
        if self.max_concurrent_downloads == 0 {
            return Err(DownloadError::Unknown("并发下载数必须大于0".to_string()));
        }

        // 验证线程数
        if self.default_threads == 0 {
            return Err(DownloadError::Unknown("默认线程数必须大于0".to_string()));
        }

        // 验证速度限制
        if self.default_speed_limit < 0.0 {
            return Err(DownloadError::Unknown("速度限制不能为负数".to_string()));
        }

        // 验证重试次数
        if self.retry_count == 0 {
            return Err(DownloadError::Unknown("重试次数必须大于0".to_string()));
        }

        // 验证超时时间
        if self.timeout == 0 {
            return Err(DownloadError::Unknown("超时时间必须大于0".to_string()));
        }

        // 验证块大小
        if self.chunk_size == 0 {
            return Err(DownloadError::Unknown("块大小必须大于0".to_string()));
        }

        // 验证缓冲区大小
        if self.buffer_size == 0 {
            return Err(DownloadError::Unknown("缓冲区大小必须大于0".to_string()));
        }

        // 验证最大重定向次数
        if self.max_redirects == 0 {
            return Err(DownloadError::Unknown("最大重定向次数必须大于0".to_string()));
        }

        // 验证最大块数
        if self.max_chunks_per_file == 0 {
            return Err(DownloadError::Unknown("最大块数必须大于0".to_string()));
        }

        // 验证最小块大小
        if self.min_chunk_size == 0 {
            return Err(DownloadError::Unknown("最小块大小必须大于0".to_string()));
        }

        // 验证块超时时间
        if self.chunk_timeout == 0 {
            return Err(DownloadError::Unknown("块超时时间必须大于0".to_string()));
        }

        // 验证恢复检查间隔
        if self.resume_check_interval == 0 {
            return Err(DownloadError::Unknown("恢复检查间隔必须大于0".to_string()));
        }

        // 路径校验
        if self.default_output_dir.trim().is_empty() {
            return Err(DownloadError::InvalidUrl("输出目录不能为空".to_string()));
        }

        // proxy 校验
        if let Some(proxy) = &self.proxy {
            if !proxy.starts_with("http://") && !proxy.starts_with("https://") && !proxy.starts_with("socks5://") {
                return Err(DownloadError::UnsupportedProtocol(proxy.clone()));
            }
        }

        // custom_headers 校验
        for (k, v) in &self.custom_headers {
            if k.trim().is_empty() || v.trim().is_empty() {
                return Err(DownloadError::Unknown("自定义请求头键值不能为空".to_string()));
            }
        }

        Ok(())
    }

    /// 合并配置
    #[allow(dead_code)]
    pub fn merge(&mut self, other: &Config) {
        // 合并下载设置
        self.max_concurrent_downloads = other.max_concurrent_downloads;
        self.default_threads = other.default_threads;
        self.default_speed_limit = other.default_speed_limit;
        self.speed_limit_kb = other.speed_limit_kb;
        self.default_output_dir = other.default_output_dir.clone();
        self.retry_count = other.retry_count;
        self.retry_delay = other.retry_delay;
        self.retry_max_delay = other.retry_max_delay;
        self.retry_backoff_multiplier = other.retry_backoff_multiplier;
        self.retry_jitter_factor = other.retry_jitter_factor;
        self.retryable_errors = other.retryable_errors.clone();

        // 合并网络设置
        self.timeout = other.timeout;
        self.user_agent = other.user_agent.clone();
        self.proxy = other.proxy.clone();
        self.verify_ssl = other.verify_ssl;

        // 合并文件设置
        self.auto_rename = other.auto_rename;
        self.overwrite_existing = other.overwrite_existing;
        self.create_directories = other.create_directories;

        // 合并通知设置
        self.enable_notifications = other.enable_notifications;
        self.notification_sound = other.notification_sound;

        // 合并界面设置
        self.show_progress_bar = other.show_progress_bar;
        self.show_speed = other.show_speed;
        self.show_eta = other.show_eta;
        self.show_size = other.show_size;

        // 合并高级设置
        self.chunk_size = other.chunk_size;
        self.buffer_size = other.buffer_size;
        self.max_redirects = other.max_redirects;
        self.custom_headers = other.custom_headers.clone();

        // 合并分块下载设置
        self.enable_chunked_download = other.enable_chunked_download;
        self.max_chunks_per_file = other.max_chunks_per_file;
        self.min_chunk_size = other.min_chunk_size;
        self.chunk_timeout = other.chunk_timeout;

        // 合并断点续传设置
        self.enable_resume = other.enable_resume;
        self.resume_check_interval = other.resume_check_interval;
        self.auto_resume_on_startup = other.auto_resume_on_startup;
    }

    /// 生成标准化的 RetryStrategy
    pub fn retry_strategy(&self) -> crate::core::task::retry::RetryStrategy {
        crate::core::task::retry::RetryStrategy {
            max_retries: self.retry_count,
            base_delay: std::time::Duration::from_secs(self.retry_delay),
            max_delay: std::time::Duration::from_secs(self.retry_max_delay),
            backoff_multiplier: self.retry_backoff_multiplier,
            jitter_factor: self.retry_jitter_factor,
            retryable_errors: self.retryable_errors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.max_concurrent_downloads, 3);
        assert_eq!(config.default_threads, 4);
        assert_eq!(config.default_speed_limit, 10.0);
        assert_eq!(config.retry_count, 3);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        config.max_concurrent_downloads = 0;
        assert!(config.validate().is_err());

        config = Config::default();
        config.default_threads = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_save_load() {
        let c = Config::default();
        let path = "./downloads/test_config.toml";
        std::fs::create_dir_all("./downloads").unwrap(); // 保证目录存在
        c.save(path).expect("保存配置失败");
        let c2 = Config::load(path).expect("加载配置失败");
        assert_eq!(c2.max_concurrent_downloads, c.max_concurrent_downloads);
        let _ = std::fs::remove_file(path); // 测试后清理
    }
} 