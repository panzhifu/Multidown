// use std::time::{Duration, Instant};
// use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
// use crate::core::task::DownloadTask;
// use std::sync::Arc;
// use tokio::sync::Mutex;
// use indicatif::ProgressBar;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

// 结构体：ProgressManager
// 用于管理下载进度条
pub struct ProgressManager {
    multi: Arc<MultiProgress>,
    bars: Arc<Mutex<HashMap<String, ProgressBar>>>,
}

impl ProgressManager {
    // 构造函数：创建 ProgressManager 实例
    pub fn new() -> Self {
        ProgressManager {
            multi: Arc::new(MultiProgress::new()),
            bars: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 添加一个新进度条，task_id为唯一标识，file_name为显示名
    pub fn add_progress_bar(&self, task_id: &str, total: u64, file_name: &str) {
        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(ProgressStyle::with_template(
            "{msg:20.20} |{bar:40.cyan/blue}| {percent:>3}% {bytes:>8}/{total_bytes:<8} {binary_bytes_per_sec:>10} ETA:{eta:>8}"
        ).unwrap()
        .progress_chars("=>-"));
        pb.set_message(file_name.to_string());
        self.bars.lock().unwrap().insert(task_id.to_string(), pb);
    }

    /// 更新进度，downloaded为已下载字节数，total为总字节数，speed为B/s
    pub fn update_progress(&self, task_id: &str, downloaded: u64, total: u64, speed: u64, file_name: &str) {
        if let Some(pb) = self.bars.lock().unwrap().get(task_id) {
            pb.set_length(total.max(downloaded)); // 防止total为0
            pb.set_position(downloaded);
            pb.set_message(file_name.to_string());
            // 自定义速度和ETA显示
            let speed_str = if speed > 1024 * 1024 {
                format!("{:.2} MB/s", speed as f64 / (1024.0 * 1024.0))
            } else if speed > 1024 {
                format!("{:.2} KB/s", speed as f64 / 1024.0)
            } else {
                format!("{} B/s", speed)
            };
            let eta_str = if speed > 0 && total > downloaded {
                let secs = (total - downloaded) / speed;
                let h = secs / 3600;
                let m = (secs % 3600) / 60;
                let s = secs % 60;
                format!("{:02}:{:02}:{:02}", h, m, s)
            } else {
                "--:--:--".to_string()
            };
            pb.set_message(format!("{} [{}]", file_name, eta_str));
            pb.set_prefix(format!("{}", speed_str));
        }
    }
} 