// use std::time::{Duration, Instant};
// use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
// use crate::core::task::DownloadTask;
// use std::sync::Arc;
// use tokio::sync::Mutex;
// use indicatif::ProgressBar;

use std::time::Instant;

pub struct ProgressManager {
    pub total_size: u64,
    pub start_time: Instant,
}

impl ProgressManager {
    pub fn new(total_size: u64) -> Self {
        Self {
            total_size,
            start_time: Instant::now(),
        }
    }

    /// 更新总进度，aria2c 风格输出
    pub fn update_progress(&self, downloaded: u64, _speed: u64) {
        let percent = if self.total_size > 0 {
            (downloaded as f64 / self.total_size as f64) * 100.0
        } else {
            0.0
        };
        let eta = if downloaded > 0 {
            let elapsed = self.start_time.elapsed().as_secs();
            let speed = downloaded as f64 / (elapsed.max(1) as f64);
            let remain = self.total_size.saturating_sub(downloaded);
            let eta_secs = if speed > 0.0 {
                (remain as f64 / speed) as u64
            } else {
                0
            };
            format_time(eta_secs)
        } else {
            "--:--:--".to_string()
        };
        let speed = if self.start_time.elapsed().as_secs() > 0 {
            downloaded as f64 / self.start_time.elapsed().as_secs_f64()
        } else {
            0.0
        };
        let speed_str = if speed > 1024.0 * 1024.0 {
            format!("{:.2} MiB/s", speed / 1024.0 / 1024.0)
        } else if speed > 1024.0 {
            format!("{:.2} KiB/s", speed / 1024.0)
        } else {
            format!("{:.0} B/s", speed)
        };
        let gid = "multidown";
        let total_str = human_size(self.total_size);
        let down_str = human_size(downloaded);
        print!("\r\x1b[2K[#{} {}/{} DL:{}][{:>5.1}%] ETA:{}   ", gid, down_str, total_str, speed_str, percent, eta);
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    pub fn finish(&self) {
        println!("\n下载完成");
    }
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MiB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.2} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_time(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
} 