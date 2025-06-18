// use std::time::{Duration, Instant};
// use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
// use crate::core::task::DownloadTask;
use std::sync::Arc;
use tokio::sync::Mutex;
use indicatif::ProgressBar;

// 结构体：ProgressManager
// 用于管理下载进度条
pub struct ProgressManager {
    progress_bars: Arc<Mutex<Vec<ProgressBar>>>,  // 使用 Arc<Mutex<Vec<ProgressBar>>> 存储进度条列表
}

impl ProgressManager {
    // 构造函数：创建 ProgressManager 实例
    pub fn new() -> Self {
        ProgressManager {
            progress_bars: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // 新增：添加进度条，返回索引
    pub async fn add_progress_bar(&self, total: u64, msg: &str) -> usize {
        let pb = ProgressBar::new(total);
        pb.set_message(msg.to_string());
        let mut bars = self.progress_bars.lock().await;
        bars.push(pb);
        bars.len() - 1
    }

    // 方法：更新下载进度
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
} 