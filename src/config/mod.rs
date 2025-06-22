use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::{Result};
use crate::core::error::DownloadError;
use std::borrow::Cow;

/// 配置结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// 下载速度限制（KB/s），0 表示不限速
    pub speed_limit_kb: u64,
    /// 默认下载目录
    pub download_dir: String,
    /// 默认线程数
    pub thread_count: usize,
    /// 最大并发下载数
    pub max_concurrent_downloads: usize,
    /// 网络超时时间（秒）
    pub timeout: u64,
    /// User-Agent
    pub user_agent: String,
    /// 是否启用断点续传
    pub enable_resume: bool,
    /// 是否启用分块下载
    pub enable_chunked_download: bool,
    /// 分块大小（字节）
    pub chunk_size: usize,
    /// 最小分块大小（字节）
    pub min_chunk_size: usize,
    /// 重试次数
    pub retry_count: usize,
    /// 重试延迟（秒）
    pub retry_delay: u64,
    /// 最大重试延迟（秒）
    pub retry_max_delay: u64,
    /// 启动时自动恢复
    pub auto_resume_on_startup: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed_limit_kb: 0, // 默认不限速
            download_dir: "./downloads".to_string(),
            thread_count: 4,
            max_concurrent_downloads: 3,
            timeout: 30,
            user_agent: "MultiDown/1.0".to_string(),
            enable_resume: true,
            enable_chunked_download: true,
            chunk_size: 8192,
            min_chunk_size: 1024,
            retry_count: 3,
            retry_delay: 5,
            retry_max_delay: 60,
            auto_resume_on_startup: true,
        }
    }
}

impl Config {
    /// 加载配置文件
    pub fn load(path: &str) -> Result<Self, DownloadError> {
        if Path::new(path).exists() {
            let content = fs::read_to_string(path)
                .map_err(|e| DownloadError::IoError(e.to_string().into()))?;
            // 尝试解析TOML
            match toml::from_str(&content) {
                Ok(config) => Ok(config),
                Err(e) => {
                    eprintln!("配置文件格式错误: {}，将使用默认配置", e);
                    let config = Config::default();
                    Config::save_with_tutorial(&config, path)?;
                    Ok(config)
                }
            }
        } else {
            let config = Config::default();
            Config::save_with_tutorial(&config, path)?;
            Ok(config)
        }
    }

    /// 保存带教程的配置文件（唯一写入方法）
    pub fn save_with_tutorial(&self, path: &str) -> Result<(), DownloadError> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DownloadError::IoError(e.to_string().into()))?;
        }
        let tutorial_content = Config::generate_tutorial_content();
        let config_content = toml::to_string_pretty(self)
            .map_err(|e| DownloadError::Unknown(format!("无法序列化配置: {}", e).into()))?;
        let full_content = format!("{}\n\n{}", tutorial_content, config_content);
        std::fs::write(path, full_content)
            .map_err(|e| DownloadError::IoError(e.to_string().into()))?;
        Ok(())
    }

    /// 生成配置文件教程内容（静态方法）
    fn generate_tutorial_content() -> String {
        r#"# MultiDown 配置文件
# ====================
# 
# 这是一个 TOML 格式的配置文件，用于配置 MultiDown 下载管理器的行为。
# 你可以根据需要修改这些设置，然后保存文件。
#
# 配置文件位置：
# - Windows: %APPDATA%/multidown/multidown.conf
# - macOS: ~/Library/Application Support/multidown/multidown.conf  
# - Linux: ~/.config/multidown/multidown.conf
#
# 命令行参数会覆盖配置文件中的设置，优先级：命令行 > 配置文件 > 默认值
#
# 使用示例：
#   multidown https://example.com/file.zip                    # 使用默认配置
#   multidown -l 1000 https://example.com/file.zip           # 限制速度1MB/s
#   multidown -t 8 https://example.com/file.zip              # 使用8个线程
#   multidown -d /path/to/downloads https://example.com/file.zip  # 指定下载目录

# ==================== 下载设置 ====================

# 下载速度限制（KB/s），0 表示不限速
# 示例：1024 = 1MB/s, 5120 = 5MB/s
speed_limit_kb = 0

# 默认下载目录
# 支持相对路径和绝对路径
download_dir = "./downloads"

# 默认线程数（每个下载任务使用的线程数）
# 建议值：2-16，根据网络环境调整
thread_count = 4

# 最大并发下载数（同时进行的下载任务数）
# 建议值：1-5，避免过多任务影响性能
max_concurrent_downloads = 3

# ==================== 网络设置 ====================

# 网络超时时间（秒）
# 如果下载在指定时间内没有响应，会重试
timeout = 30

# User-Agent 字符串
# 某些服务器可能需要特定的 User-Agent
user_agent = "MultiDown/1.0"

# ==================== 高级功能 ====================

# 是否启用断点续传
# 启用后，下载中断可以从断点继续
enable_resume = true

# 是否启用分块下载
# 启用后，大文件会被分成多个块并行下载
enable_chunked_download = true

# 分块大小（字节）
# 建议值：4096-32768，太小影响性能，太大会占用更多内存
chunk_size = 8192

# 最小分块大小（字节）
# 只有文件大小超过此值才会使用分块下载
min_chunk_size = 1024

# ==================== 重试设置 ====================

# 重试次数
# 网络错误时的重试次数
retry_count = 3

# 重试延迟（秒）
# 第一次重试前的等待时间
retry_delay = 5

# 最大重试延迟（秒）
# 重试延迟的最大值（使用指数退避）
retry_max_delay = 60

# ==================== 启动设置 ====================

# 启动时自动恢复未完成的下载
# 启用后，程序启动时会自动恢复上次未完成的下载
auto_resume_on_startup = true

# ==================== 使用说明 ====================
#
# 1. 基本使用：
#    multidown https://example.com/file.zip
#
# 2. 批量下载：
#    multidown -f urls.txt
#    # urls.txt 文件内容（每行一个URL）：
#    # https://example.com/file1.zip
#    # https://example.com/file2.zip
#
# 3. 速度限制：
#    multidown -l 1000 https://example.com/file.zip
#
# 4. 指定线程数：
#    multidown -t 8 https://example.com/file.zip
#
# 5. 指定下载目录：
#    multidown -d /path/to/downloads https://example.com/file.zip
#
# 6. 编辑配置文件：
#    multidown -e
#
# 7. 查看帮助：
#    multidown --help
#
# ==================== 故障排除 ====================
#
# 问题：下载速度很慢
# 解决：增加 thread_count 或检查 speed_limit_kb 设置
#
# 问题：经常下载失败
# 解决：增加 retry_count 或 timeout 值
#
# 问题：大文件下载中断
# 解决：确保 enable_resume = true
#
# 问题：内存占用过高
# 解决：减少 chunk_size 或 max_concurrent_downloads
#
# ==================== 性能调优建议 ====================
#
# 高速网络（100Mbps+）：
#   thread_count = 8-16
#   chunk_size = 16384
#   max_concurrent_downloads = 3-5
#
# 中速网络（10-100Mbps）：
#   thread_count = 4-8
#   chunk_size = 8192
#   max_concurrent_downloads = 2-3
#
# 低速网络（<10Mbps）：
#   thread_count = 2-4
#   chunk_size = 4096
#   max_concurrent_downloads = 1-2

# ==================== 配置项说明 ====================
"#.to_string()
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), DownloadError> {
        // 验证线程数
        if self.thread_count == 0 {
            return Err(DownloadError::Unknown(Cow::Borrowed("线程数必须大于0")));
        }

        // 验证并发下载数
        if self.max_concurrent_downloads == 0 {
            return Err(DownloadError::Unknown(Cow::Borrowed("并发下载数必须大于0")));
        }

        // 验证超时时间
        if self.timeout == 0 {
            return Err(DownloadError::Unknown(Cow::Borrowed("超时时间必须大于0")));
        }

        // 验证下载目录
        if self.download_dir.is_empty() {
            return Err(DownloadError::Unknown(Cow::Borrowed("下载目录不能为空")));
        }

        // 验证分块大小
        if self.chunk_size == 0 {
            return Err(DownloadError::Unknown(Cow::Borrowed("分块大小必须大于0")));
        }

        // 验证最小分块大小
        if self.min_chunk_size == 0 {
            return Err(DownloadError::Unknown(Cow::Borrowed("最小分块大小必须大于0")));
        }

        // 验证重试次数
        if self.retry_count == 0 {
            return Err(DownloadError::Unknown(Cow::Borrowed("重试次数必须大于0")));
        }

        Ok(())
    }

    /// 合并命令行参数到配置
    pub fn merge_from_args(&mut self, args: &crate::cli::Args) {
        // 命令行参数覆盖配置文件
        if let Some(speed_limit) = args.speed_limit_kb {
            self.speed_limit_kb = speed_limit;
        }
        
        if !args.download_dir.is_empty() {
            self.download_dir = args.download_dir.clone();
        }
        
        if let Some(thread_count) = args.thread_count {
            self.thread_count = thread_count;
        }
    }

    /// 获取配置摘要信息
    pub fn get_summary(&self) -> String {
        format!(
            "配置摘要:\n\
            - 下载目录: {}\n\
            - 线程数: {}\n\
            - 并发数: {}\n\
            - 速度限制: {} KB/s\n\
            - 超时时间: {} 秒\n\
            - 重试次数: {}\n\
            - 断点续传: {}\n\
            - 分块下载: {}",
            self.download_dir,
            self.thread_count,
            self.max_concurrent_downloads,
            if self.speed_limit_kb == 0 { "不限速".to_string() } else { self.speed_limit_kb.to_string() },
            self.timeout,
            self.retry_count,
            if self.enable_resume { "启用" } else { "禁用" },
            if self.enable_chunked_download { "启用" } else { "禁用" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.speed_limit_kb, 0);
        assert_eq!(config.thread_count, 4);
        assert_eq!(config.max_concurrent_downloads, 3);
        assert_eq!(config.timeout, 30);
        assert_eq!(config.retry_count, 3);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        config.thread_count = 0;
        assert!(config.validate().is_err());

        config = Config::default();
        config.max_concurrent_downloads = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_save_load() {
        let config = Config::default();
        let path = "./test_config.toml";
        
        config.save_with_tutorial(path).expect("保存带教程的配置失败");
        let loaded_config = Config::load(path).expect("加载配置失败");
        
        assert_eq!(loaded_config.speed_limit_kb, config.speed_limit_kb);
        assert_eq!(loaded_config.thread_count, config.thread_count);
        
        // 清理测试文件
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_config_save_with_tutorial() {
        let config = Config::default();
        let path = "./test_config_with_tutorial.toml";
        config.save_with_tutorial(path).expect("保存带教程的配置失败");
        let content = std::fs::read_to_string(path).expect("读取配置文件失败");
        assert!(content.contains("MultiDown 配置文件"));
        assert!(content.contains("使用示例"));
        assert!(content.contains("故障排除"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_config_summary() {
        let config = Config::default();
        let summary = config.get_summary();
        
        assert!(summary.contains("配置摘要"));
        assert!(summary.contains("下载目录"));
        assert!(summary.contains("线程数"));
        assert!(summary.contains("不限速"));
    }
} 