//! CLI: 命令行接口和参数解析模块
//! 
//! ## 主要功能
//! 
//! - 命令行参数解析和验证
//! - 配置文件路径管理
//! - URL 列表处理（命令行参数和文件）
//! - 平台特定的路径处理
//! - 配置文件编辑器集成
//! 
//! ## 支持的命令
//! 
//! - 基本下载：`multidown <url>`
//! - 批量下载：`multidown -f urls.txt`
//! - 编辑配置：`multidown -e`
//! - 指定配置：`multidown -c config.conf <url>`
//! - 速度限制：`multidown -l 1024 <url>`
//! 
//! ## 平台支持
//! 
//! - Windows: `%APPDATA%/multidown/multidown.conf`
//! - macOS: `~/Library/Application Support/multidown/multidown.conf`
//! - Linux: `~/.config/multidown/multidown.conf`

use clap::Parser;
use std::fs;
use crate::config::Config;
use actix::prelude::*;
use crate::core::error::DownloadError;
use std::path::Path;
use std::env;
use std::borrow::Cow;

/// 获取平台默认配置文件路径
pub fn default_config_path() -> String {
    #[cfg(target_os = "windows")]
    {
        let appdata = env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        format!("{}/multidown/multidown.conf", appdata)
    }
    #[cfg(target_os = "macos")]
    {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/Library/Application Support/multidown/multidown.conf", home)
    }
    #[cfg(target_os = "linux")]
    {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.config/multidown/multidown.conf", home)
    }
}

/// 打开配置文件编辑器
pub fn open_config_in_editor(config_path: &str) {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("notepad").arg(config_path).status().ok();
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-e").arg(config_path).status().ok();
    }
    #[cfg(target_os = "linux")]
    {
        // 优先 xdg-open，否则 nano
        if std::process::Command::new("xdg-open").arg(config_path).status().is_err() {
            let _ = std::process::Command::new("nano").arg(config_path).status();
        }
    }
}

/// 获取平台默认下载目录（当前工作目录）
fn get_default_download_dir() -> String {
    std::env::current_dir() // 获取当前工作目录
        .map(|p| p.display().to_string()) // .map是Option的迭代器方法，将Option转换为String
        .unwrap_or_else(|_| ".".to_string()) // 如果获取失败，返回"."，即当前目录，保证返回值不为空
}

/// MultiDown 命令行参数
/// 
/// 示例用法：
///   multidown https://example.com/file.zip
///   multidown -e  # 编辑配置文件
///   multidown -c /path/to/config.conf https://example.com/file.zip
///   multidown -l 1000 https://example.com/file.zip
///
/// 更多用法请加 --help 查看
#[derive(Parser, Debug, Clone)]
#[command(
    name = "multidown",
    author = "panzhifu",
    version = env!("CARGO_PKG_VERSION"),
    about = "一个用 Rust 编写的多线程下载管理器",
    long_about = "支持并发下载、断点续传、动态分片调整和实时进度显示的多线程下载管理器。\n\n示例：\n  multidown https://example.com/file.zip\n  multidown -e\n  multidown -c /path/to/config.conf https://example.com/file.zip\n  multidown --speed-limit-kb 1000 https://example.com/file.zip\n"
)]
pub struct Args {
    /// 要下载的URL列表（可同时指定多个）
    #[arg(required = false, help = "要下载的URL列表，可以同时指定多个URL。")]
    pub urls: Vec<String>,

    /// 包含URL列表的文件路径
    #[arg(short, long, help = "包含URL列表的文件路径，每行一个URL。")]
    pub file: Option<String>,

    /// 配置文件路径，默认为平台推荐路径
    #[arg(short = 'c', long, default_value_t = default_config_path(), help = "配置文件路径，默认为平台推荐路径。")]
    pub config: String,

    /// 编辑配置文件（-e 或 --edit）
    #[arg(short = 'e', long = "edit", help = "用系统默认编辑器打开配置文件并退出。")]
    pub edit_config: bool,

    /// 下载速度限制（KB/s），0 表示不限速
    #[arg(long, short = 'l', help = "下载速度限制（KB/s），0 表示不限速。")]
    pub speed_limit_kb: Option<u64>,

    /// 指定下载目录（默认：当前工作目录）
    #[arg(long, short = 'd', default_value_t = get_default_download_dir(), help = "指定下载目录，覆盖配置文件中的设置，默认当前工作目录。")]
    pub download_dir: String,

    /// 指定下载文件名
    #[arg(long, short = 'n', help = "指定下载文件名，覆盖URL自动推断。")]
    pub file_name: Option<String>,

    /// 指定下载线程数
    #[arg(long, short = 't', help = "指定下载线程数，覆盖配置文件中的设置。")]
    pub thread_count: Option<usize>,

}

impl Args {
    pub fn parse_args() -> Result<(Self, Config), DownloadError> { 
        // 解析命令行参数，并返回配置文件路径和配置，DownloadError是自定义错误类型
        let args = Args::parse();
        
        // --edit-config 逻辑
        if args.edit_config {
            open_config_in_editor(&args.config);
            std::process::exit(0); // 退出程序
        }

        // 加载或创建配置文件
        let mut config = if Path::new(&args.config).exists() {
            Config::load(&args.config).map_err(|e| DownloadError::permission_error(format!("无法读取配置文件: {}", e)))?
        } else {
            // 确保配置文件所在目录存在
            if let Some(parent) = Path::new(&args.config).parent() { 
                // .parent() 返回父目录路径，如果存在
                std::fs::create_dir_all(parent).map_err(|e| DownloadError::permission_error(format!("无法创建配置目录: {}", e)))?;
            }
            
            let config = Config::default();
            config.save_with_tutorial(&args.config).map_err(|e| DownloadError::permission_error(format!("无法保存配置文件: {}", e)))?;
            config // 返回默认配置
        };

        // 合并命令行参数到配置
        config.merge_from_args(&args);

        // 验证配置
        config.validate().map_err(|e| DownloadError::unknown(format!("配置无效: {}", e)))?;

        Ok((args, config))
    }

    // 定义从文件中读取URL的方法
    pub fn get_urls(&self) -> Result<Vec<String>, DownloadError> {
        let mut urls = Vec::new(); // vec是一个动态数组，可以存储任意类型的元素

        // 如果提供了URL列表，添加到结果中
        urls.extend_from_slice(&self.urls); // extend_from_slice方法将切片中的元素添加到vec中，不使用clone提升性能

        // 如果提供了文件，从文件中读取URL
        if let Some(file_path) = &self.file {
            if !Path::new(file_path).exists() {
                return Err(DownloadError::file_exists(file_path.to_string()));
            }
            let content = fs::read_to_string(file_path)
                .map_err(|e| DownloadError::permission_error(std::borrow::Cow::Owned(format!("无法读取URL文件: {}", e))))?;
            
            // 按行读取URL，忽略空行和注释
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    if !crate::utils::validator::is_valid_url(line) {
                        return Err(DownloadError::invalid_url(line.to_string()));
                    }
                    urls.push(line.to_string());
                }
            }
        }

        // 验证URL列表不为空
        if urls.is_empty() {
            return Err(DownloadError::invalid_url(Cow::Borrowed("未提供任何URL。请通过命令行参数或文件提供至少一个URL。")));
        }

        Ok(urls)
    }
}

// ========== actix集成 ==========

/// 消息：解析命令行参数和配置
pub struct ParseArgs;
impl Message for ParseArgs { type Result = Result<(Args, Config), DownloadError>; }

/// 消息：获取URL列表
pub struct GetUrls(pub Args);
impl Message for GetUrls { type Result = Result<Vec<String>, DownloadError>; }

/// CLI参数解析Actor
/// CLI 是Command Line Interface 的缩写，表示命令行界面。
pub struct CliActor;
impl Actor for CliActor { type Context = actix::Context<Self>; }

// 实现Handler trait，用于处理消息，用于ParseArgs消息
impl Handler<ParseArgs> for CliActor {
    type Result = MessageResult<ParseArgs>;
    fn handle(&mut self, _msg: ParseArgs, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(Args::parse_args().map_err(DownloadError::from))
    }
}

// 实现Handler trait，用于处理消息，用于GetUrls消息
impl Handler<GetUrls> for CliActor {
    type Result = MessageResult<GetUrls>;
    fn handle(&mut self, msg: GetUrls, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(msg.0.get_urls().map_err(DownloadError::from))
    }
}

// 测试模块
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_args_parsing() {
        let args = vec!["multidown", "https://example.com/file.zip"];
        let result = Args::try_parse_from(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_loading() {
        // 创建临时配置文件
        let temp_config = "temp_config.conf";
        let config = Config::default();
        config.save_with_tutorial(temp_config).unwrap();

        // 测试加载配置
        let args = vec!["multidown", "-c", temp_config, "https://example.com/file.zip"];
        let result = Args::try_parse_from(args);
        assert!(result.is_ok());

        // 清理临时文件
        fs::remove_file(temp_config).unwrap();
    }

    #[test]
    fn test_url_file_parsing() {
        // 创建临时URL文件
        let temp_url_file = "temp_urls.txt";
        let content = "# 这是一个注释\nhttps://example.com/file1.zip\nhttps://example.com/file2.zip\n";
        fs::write(temp_url_file, content).unwrap();

        // 测试从文件读取URL
        let args = vec!["multidown", "-f", temp_url_file];
        let result = Args::try_parse_from(args);
        assert!(result.is_ok());

        let args = result.unwrap();
        let urls = args.get_urls().unwrap();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com/file1.zip");
        assert_eq!(urls[1], "https://example.com/file2.zip");

        // 清理临时文件
        fs::remove_file(temp_url_file).unwrap();
    }
}