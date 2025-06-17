use chrono::Local; // 用于获取本地时间
use env_logger::Builder; // 用于初始化日志
use log::LevelFilter; // 用于设置日志级别
use std::io::Write; // 用于写入日志
use std::fs::{self, File};
use std::path::Path;
use anyhow::Result;

// 初始化日志函数
pub fn init_logger() -> Result<()> {
    // 创建logs目录
    let log_dir = "logs";
    if !Path::new(log_dir).exists() {
        fs::create_dir_all(log_dir)?;
    }

    // 创建日志文件
    let log_file = format!("{}/multidown_{}.log", 
        log_dir,
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let file = File::create(&log_file)?;

    // 创建日志记录器
    Builder::new()
        .format(|buf, record| {
            // 格式化日志记录
            writeln!(buf, // 写入日志
                "{} [{}] - {}", // 时间、级别、消息
                Local::now().format("%Y-%m-%d %H:%M:%S"), // 时间
                record.level(), // 级别
                record.args() // 消息
            )
        })
        .filter(None, LevelFilter::Info) // 设置日志级别
        .target(env_logger::Target::Pipe(Box::new(file)))
        .init();

    log::info!("日志文件已创建: {}", log_file);
    Ok(())
}

// 设置日志级别
#[allow(dead_code)]
pub fn set_log_level(level: LevelFilter) {
    log::set_max_level(level); // 设置日志级别
} 