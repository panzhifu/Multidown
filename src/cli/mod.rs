use clap::Parser;
use anyhow::Result;
use std::path::PathBuf;
use crate::config::Config;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// 要下载的URL列表
    #[arg(required = true)]
    pub urls: Vec<String>,

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
}

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
}