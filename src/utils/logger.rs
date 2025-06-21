use chrono::Local; // 用于获取本地时间
use log::LevelFilter; // 用于设置日志级别
use std::io::Write; // 用于写入日志
use std::fs::File;
use actix::prelude::*;

// ========== actix 日志Actor ==========

/// 日志消息
pub struct LogMsg {
    pub level: LevelFilter,
    pub message: String,
}
impl Message for LogMsg { type Result = (); }

/// 动态设置日志级别
pub struct SetLogLevelMsg(pub LevelFilter);
impl Message for SetLogLevelMsg { type Result = (); }

/// 日志Actor
pub struct LoggerActor {
    pub file: File,
    pub level: LevelFilter,
}

impl Actor for LoggerActor {
    type Context = Context<Self>;
}

impl Handler<LogMsg> for LoggerActor {
    type Result = ();
    fn handle(&mut self, msg: LogMsg, _ctx: &mut Self::Context) {
        if msg.level <= self.level {
            let _ = writeln!(self.file,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                msg.level,
                msg.message
            );
        }
    }
}

impl Handler<SetLogLevelMsg> for LoggerActor {
    type Result = ();
    fn handle(&mut self, msg: SetLogLevelMsg, _ctx: &mut Self::Context) {
        self.level = msg.0;
    }
}

// ========== 兼容原有同步日志初始化 ==========

/// 设置日志级别
#[allow(dead_code)]
pub fn set_log_level(level: LevelFilter) {
    log::set_max_level(level); // 设置日志级别
} 