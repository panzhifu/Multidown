use clap::Parser;
use anyhow::{Result, Context};
use std::path::PathBuf;
use std::fs;
use crate::config::Config;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
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
    pub fn parse_args() -> Result<(Self, Config)> {
        // 解析命令行参数
        let args = Args::parse();
        
        // 加载或创建配置文件
        let config = if PathBuf::from(&args.config).exists() {
            Config::load(&args.config)?
        } else {
            let config = Config::default();
            config.save(&args.config)?;
            config
        };

        // 验证配置
        config.validate()?;

        Ok((args, config))
    }

    pub fn get_urls(&self) -> Result<Vec<String>> {
        let mut urls = Vec::new();

        // 如果提供了URL列表，添加到结果中
        urls.extend(self.urls.clone());

        // 如果提供了文件，从文件中读取URL
        if let Some(file_path) = &self.file {
            let content = fs::read_to_string(file_path)
                .with_context(|| format!("无法读取URL文件: {}", file_path))?;
            
            // 按行读取URL，忽略空行和注释
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    urls.push(line.to_string());
                }
            }
        }

        // 验证URL列表不为空
        if urls.is_empty() {
            anyhow::bail!("未提供任何URL。请通过命令行参数或文件提供至少一个URL。");
        }

        Ok(urls)
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