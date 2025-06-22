use chrono::Local; // 用于获取本地时间
use log::LevelFilter; // 用于设置日志级别
use std::io::{Write, BufWriter};
use std::fs::{File, OpenOptions};
use std::path::Path;
use actix::prelude::*;

/// 日志消息
pub struct LogMsg {
    pub level: LevelFilter,
    pub message: String,
}
impl Message for LogMsg { type Result = (); }

/// 日志Actor
pub struct LoggerActor {
    pub writer: BufWriter<File>,
    pub level: LevelFilter,
    pub file_path: String,
    pub max_size: u64, // 最大文件大小 (bytes)
    pub current_size: u64,
}

impl LoggerActor {
    /// 创建新的日志Actor
    pub fn new(file_path: &str, level: LevelFilter, max_size: u64) -> Result<Self, std::io::Error> {
        // 确保日志目录存在
        if let Some(parent) = Path::new(file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;
        
        let writer = BufWriter::new(file);
        
        Ok(Self {
            writer,
            level,
            file_path: file_path.to_string(),
            max_size,
            current_size: 0,
        })
    }
    
    /// 检查并执行日志轮转
    fn check_rotation(&mut self) -> Result<(), std::io::Error> {
        if self.current_size > self.max_size {
            // 关闭当前文件
            self.writer.flush()?;
            
            // 重命名当前日志文件
            let backup_path = format!("{}.backup", self.file_path);
            if Path::new(&backup_path).exists() {
                std::fs::remove_file(&backup_path)?;
            }
            std::fs::rename(&self.file_path, &backup_path)?;
            
            // 创建新文件
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.file_path)?;
            
            self.writer = BufWriter::new(file);
            self.current_size = 0;
        }
        Ok(())
    }
    
    /// 写入日志并处理错误
    fn write_log(&mut self, level: LevelFilter, message: &str) -> Result<(), std::io::Error> {
        if level <= self.level {
            let log_entry = format!(
                "{} [{}] - {}\n",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                level,
                message
            );
            
            // 检查轮转
            self.check_rotation()?;
            
            // 写入日志
            self.writer.write_all(log_entry.as_bytes())?;
            self.current_size += log_entry.len() as u64;
            
            // 定期刷新缓冲区
            if self.current_size % 1024 < log_entry.len() as u64 {
                self.writer.flush()?;
            }
        }
        Ok(())
    }
}

impl Actor for LoggerActor {
    type Context = Context<Self>;
}

impl Handler<LogMsg> for LoggerActor {
    type Result = ();
    fn handle(&mut self, msg: LogMsg, _ctx: &mut Self::Context) {
        if let Err(e) = self.write_log(msg.level, &msg.message) {
            eprintln!("日志写入失败: {}", e);
        }
    }
}

// 便捷的日志方法 - 为Addr<LoggerActor>提供扩展方法
pub trait LoggerExt {
    fn info(&self, message: &str);
    fn error(&self, message: &str);
    fn warn(&self, message: &str);
    fn debug(&self, message: &str);
}

impl LoggerExt for Addr<LoggerActor> {
    fn info(&self, message: &str) {
        self.do_send(LogMsg {
            level: LevelFilter::Info,
            message: message.to_string(),
        });
    }
    
    fn error(&self, message: &str) {
        self.do_send(LogMsg {
            level: LevelFilter::Error,
            message: message.to_string(),
        });
    }
    
    fn warn(&self, message: &str) {
        self.do_send(LogMsg {
            level: LevelFilter::Warn,
            message: message.to_string(),
        });
    }
    
    fn debug(&self, message: &str) {
        self.do_send(LogMsg {
            level: LevelFilter::Debug,
            message: message.to_string(),
        });
    }
} 