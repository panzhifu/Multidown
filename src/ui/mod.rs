mod progress;

use std::fmt;
pub use progress::ProgressManager;

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
    pub elapsed_time: std::time::Duration,
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