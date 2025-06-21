use clap::Parser;
use std::fs;
use crate::config::Config;
use actix::prelude::*;
use crate::core::error::DownloadError;
use std::path::Path;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "multidown",
    author = "MultiDown Team",
    version = env!("CARGO_PKG_VERSION"),
    about = "一个用 Rust 编写的多线程下载管理器",
    long_about = "支持并发下载、断点续传、动态分片调整和实时进度显示的多线程下载管理器"
)]
pub struct Args {
    /// 要下载的URL列表
    #[arg(required = false)]
    pub urls: Vec<String>,

    /// 包含URL列表的文件路径
    #[arg(short, long)]
    pub file: Option<String>,

    /// 配置文件路径
    #[arg(short, long, default_value = "./multidown.conf")]
    pub config: String,
}

impl Args {
    /// 显示版本信息
    pub fn show_version() {
        println!("MultiDown v{}", env!("CARGO_PKG_VERSION"));
        
        // 尝试显示构建时间（如果可用）
        if let Ok(timestamp) = std::env::var("VERGEN_BUILD_TIMESTAMP") {
            println!("构建时间: {}", timestamp);
        }
        
        // 尝试显示Git提交（如果可用）
        if let Ok(git_sha) = std::env::var("VERGEN_GIT_SHA_SHORT") {
            println!("Git提交: {}", git_sha);
        }
        
        // 显示目标平台
        if let Ok(target) = std::env::var("TARGET") {
            println!("目标平台: {}", target);
        }
        if let Ok(rust_version) = std::env::var("RUST_VERSION") {
            println!("Rust版本: {}", rust_version);
        }
    }

    pub fn parse_args() -> Result<(Self, Config), DownloadError> {
        // 解析命令行参数
        let args = Args::parse();
        
        // 加载或创建配置文件
        let config = if Path::new(&args.config).exists() {
            Config::load(&args.config).map_err(|e| DownloadError::PermissionError(format!("无法读取配置文件: {}", e)))?
        } else {
            let config = Config::default();
            config.save(&args.config).map_err(|e| DownloadError::PermissionError(format!("无法保存配置文件: {}", e)))?;
            config
        };

        // 验证配置
        config.validate().map_err(|e| DownloadError::Unknown(format!("配置无效: {}", e)))?;

        Ok((args, config))
    }

    pub fn get_urls(&self) -> Result<Vec<String>, DownloadError> {
        let mut urls = Vec::new();

        // 如果提供了URL列表，添加到结果中
        urls.extend(self.urls.clone());

        // 如果提供了文件，从文件中读取URL
        if let Some(file_path) = &self.file {
            if !Path::new(file_path).exists() {
                return Err(DownloadError::FileExists(file_path.clone()));
            }
            let content = fs::read_to_string(file_path)
                .map_err(|e| DownloadError::PermissionError(format!("无法读取URL文件: {}", e)))?;
            
            // 按行读取URL，忽略空行和注释
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    if !crate::utils::validator::is_valid_url(line) {
                        return Err(DownloadError::InvalidUrl(line.to_string()));
                    }
                    urls.push(line.to_string());
                }
            }
        }

        // 验证URL列表不为空
        if urls.is_empty() {
            return Err(DownloadError::InvalidUrl("未提供任何URL。请通过命令行参数或文件提供至少一个URL。".to_string()));
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
pub struct CliActor;
impl Actor for CliActor { type Context = actix::Context<Self>; }

impl Handler<ParseArgs> for CliActor {
    type Result = MessageResult<ParseArgs>;
    fn handle(&mut self, _msg: ParseArgs, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(Args::parse_args().map_err(DownloadError::from))
    }
}

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
        config.save(temp_config).unwrap();

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