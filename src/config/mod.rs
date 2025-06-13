use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    // 下载设置
    pub max_concurrent_downloads: usize,
    pub default_threads: usize,
    pub default_speed_limit: f32,
    pub default_output_dir: String,
    pub retry_count: usize,
    pub retry_delay: u64,
    
    // 网络设置
    pub timeout: u64,
    pub user_agent: String,
    pub proxy: Option<String>,
    pub verify_ssl: bool,
    
    // 文件设置
    pub auto_rename: bool,
    pub overwrite_existing: bool,
    pub create_directories: bool,
    
    // 通知设置
    pub enable_notifications: bool,
    pub notification_sound: bool,
    
    // 界面设置
    pub show_progress_bar: bool,
    pub show_speed: bool,
    pub show_eta: bool,
    pub show_size: bool,
    
    // 高级设置
    pub chunk_size: usize,
    pub buffer_size: usize,
    pub max_redirects: usize,
    pub custom_headers: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // 下载设置
            max_concurrent_downloads: 3,
            default_threads: 4,
            default_speed_limit: 10.0,
            default_output_dir: "./downloads".to_string(),
            retry_count: 3,
            retry_delay: 5,
            
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
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)
                .with_context(|| format!("无法读取配置文件: {}", path))?;
            
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

    pub fn save(&self, path: &str) -> Result<()> {
        // 确保目录存在
        if let Some(parent) = Path::new(path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("无法创建配置目录: {}", parent.display()))?;
            }
        }

        // 序列化为TOML
        let content = toml::to_string_pretty(self)
            .with_context(|| "无法序列化配置")?;
        
        // 写入文件
        fs::write(path, content)
            .with_context(|| format!("无法保存配置文件: {}", path))?;
        
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // 验证并发下载数
        if self.max_concurrent_downloads == 0 {
            anyhow::bail!("并发下载数必须大于0");
        }

        // 验证线程数
        if self.default_threads == 0 {
            anyhow::bail!("默认线程数必须大于0");
        }

        // 验证速度限制
        if self.default_speed_limit < 0.0 {
            anyhow::bail!("速度限制不能为负数");
        }

        // 验证重试次数
        if self.retry_count == 0 {
            anyhow::bail!("重试次数必须大于0");
        }

        // 验证超时时间
        if self.timeout == 0 {
            anyhow::bail!("超时时间必须大于0");
        }

        // 验证块大小
        if self.chunk_size == 0 {
            anyhow::bail!("块大小必须大于0");
        }

        // 验证缓冲区大小
        if self.buffer_size == 0 {
            anyhow::bail!("缓冲区大小必须大于0");
        }

        // 验证最大重定向次数
        if self.max_redirects == 0 {
            anyhow::bail!("最大重定向次数必须大于0");
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn merge(&mut self, other: &Config) {
        // 合并下载设置
        self.max_concurrent_downloads = other.max_concurrent_downloads;
        self.default_threads = other.default_threads;
        self.default_speed_limit = other.default_speed_limit;
        self.default_output_dir = other.default_output_dir.clone();
        self.retry_count = other.retry_count;
        self.retry_delay = other.retry_delay;

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        let temp_config = "temp_config.conf";
        let config = Config::default();
        
        // 测试保存
        config.save(temp_config).unwrap();
        assert!(Path::new(temp_config).exists());
        
        // 测试加载
        let loaded_config = Config::load(temp_config).unwrap();
        assert_eq!(loaded_config.max_concurrent_downloads, config.max_concurrent_downloads);
        assert_eq!(loaded_config.default_threads, config.default_threads);
        
        // 清理
        fs::remove_file(temp_config).unwrap();
    }
} 