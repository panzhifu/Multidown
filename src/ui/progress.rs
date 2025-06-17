use std::time::{Duration, Instant};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::core::task::DownloadTask;

pub struct ProgressManager {
    multi_progress: Arc<Mutex<MultiProgress>>,
    progress_bars: Arc<Mutex<Vec<ProgressBar>>>,
    start_time: Instant,
}

impl ProgressManager {
    pub fn new() -> Self {
        ProgressManager {
            multi_progress: Arc::new(Mutex::new(MultiProgress::new())),
            progress_bars: Arc::new(Mutex::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    pub async fn add_task(&self, task: &DownloadTask) {
        let pb = ProgressBar::new(100);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) {msg}")
                .unwrap()
                .progress_chars("█▓▒░")
        );

        // 设置文件名显示
        let filename = task.url.split('/').last().unwrap_or("未知文件");
        pb.set_message(format!("下载: {}", filename));

        let mut bars = self.progress_bars.lock().await;
        let multi = self.multi_progress.lock().await;
        multi.add(pb.clone());
        bars.push(pb);
    }

    pub async fn update_progress(&self, task_index: usize, downloaded: u64, total: u64, speed: u64) {
        if let Some(pb) = self.progress_bars.lock().await.get(task_index) {
            // 设置进度条长度
            if total > 0 {
                pb.set_length(total);
            }
            pb.set_position(downloaded);
            
            // 计算进度百分比
            let percentage = if total > 0 {
                (downloaded as f64 / total as f64 * 100.0) as u32
            } else {
                0
            };

            // 更新速度显示
            let speed_str = if speed > 1024 * 1024 {
                format!("{:.2} MB/s", speed as f64 / (1024.0 * 1024.0))
            } else if speed > 1024 {
                format!("{:.2} KB/s", speed as f64 / 1024.0)
            } else {
                format!("{} B/s", speed)
            };

            // 计算剩余时间
            let eta = if speed > 0 && total > downloaded {
                let remaining = total - downloaded;
                let seconds = remaining / speed;
                if seconds > 3600 {
                    format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
                } else if seconds > 60 {
                    format!("{}m{}s", seconds / 60, seconds % 60)
                } else {
                    format!("{}s", seconds)
                }
            } else {
                "未知".to_string()
            };

            // 更新状态信息
            let status = format!(
                "{}% | {} | ETA:{}",
                percentage,
                speed_str,
                eta
            );
            pb.set_message(status);
        }
    }

    pub async fn finish_task(&self, task_index: usize, success: bool) {
        if let Some(pb) = self.progress_bars.lock().await.get(task_index) {
            if success {
                pb.finish_with_message("✓ 下载完成");
            } else {
                pb.finish_with_message("✗ 下载失败");
            }
        }
    }

    #[allow(dead_code)]
    pub async fn finish_all(&self) {
        let bars = self.progress_bars.lock().await;
        for pb in bars.iter() {
            pb.finish_and_clear();
        }
    }

    pub fn elapsed_time(&self) -> Duration {
        self.start_time.elapsed()
    }

    #[allow(dead_code)]
    pub fn format_size(size: u64) -> String {
        const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[allow(dead_code)]
impl ProgressManager {
    // ... existing code ...
} 