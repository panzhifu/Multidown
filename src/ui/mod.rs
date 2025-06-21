use actix::prelude::*;
use std::fmt;
use std::time::Duration;
use std::collections::HashSet;

mod progress;
use progress::ProgressManager;

// UI展示Actor
pub struct UiActor {
    progress: ProgressManager,
    known_tasks: HashSet<String>,
}

impl UiActor {
    pub fn new() -> Self {
        UiActor {
            progress: ProgressManager::new(),
            known_tasks: HashSet::new(),
        }
    }
}

impl Actor for UiActor {
    type Context = Context<Self>;
}

// 进度更新消息
pub struct UpdateProgressMsg {
    pub task_id: String,
    pub progress: f32,
    pub speed: u64,
    pub size: u64,
}
impl Message for UpdateProgressMsg {
    type Result = ();
}

// 下载汇总消息
pub struct ShowSummaryMsg {
    pub summary: DownloadSummary,
}
impl Message for ShowSummaryMsg {
    type Result = ();
}

// 进度更新处理
impl Handler<UpdateProgressMsg> for UiActor {
    type Result = ();
    fn handle(&mut self, msg: UpdateProgressMsg, _ctx: &mut Self::Context) {
        if !self.known_tasks.contains(&msg.task_id) {
            self.progress.add_progress_bar(&msg.task_id, msg.size, &format!("任务 {}", msg.task_id));
            self.known_tasks.insert(msg.task_id.clone());
        }
        let downloaded = (msg.progress * msg.size as f32 / 100.0) as u64;
        let file_name = format!("任务 {}", msg.task_id);
        self.progress.update_progress(&msg.task_id, downloaded, msg.size, msg.speed, &file_name);
    }
}

// 汇总展示处理
impl Handler<ShowSummaryMsg> for UiActor {
    type Result = ();
    fn handle(&mut self, msg: ShowSummaryMsg, _ctx: &mut Self::Context) {
        println!("{}", msg.summary);
    }
}

pub fn print_success(message: &str) {
    println!("✓ {}", message);
}

pub fn print_error(message: &str) {
    println!("✗ {}", message);
}

// pub fn print_info(message: &str) {
//     println!("{} {}", "ℹ".blue(), message.blue());
// }

// pub fn print_warning(message: &str) {
//     println!("{} {}", "⚠".yellow(), message.yellow());
// }

pub struct DownloadSummary {
    pub total_files: usize,
    pub total_size: u64,
    pub elapsed_time: Duration,
    pub success_count: usize,
    pub failed_count: usize,
}

impl fmt::Display for DownloadSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n下载摘要:")?;
        writeln!(f, "总文件数: {}", self.total_files)?;
        writeln!(f, "总大小: {}", format_size(self.total_size))?;
        writeln!(f, "耗时: {:.2}秒", self.elapsed_time.as_secs_f64())?;
        writeln!(f, "成功: {}", self.success_count)?;
        writeln!(f, "失败: {}", self.failed_count)?;
        Ok(())
    }
}

fn format_size(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
} 