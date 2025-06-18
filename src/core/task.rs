// pub是rust中的一种访问控制关键字，它表示公共的，可以被外部访问。
// 在rust中，变量、函数、结构体等默认都是私有的，只有在其定义的模块中才能访问。

use std::str::FromStr;  // 添加 FromStr trait 的导入
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc::UnboundedSender};
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use anyhow::Result;
use serde::{Serialize, Deserialize};
use serde_json;

// 分片进度信息 - 从downloader移植
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChunkProgress {
    pub index: usize,  // 分片索引
    pub start: u64,  // 分片起始位置
    pub end: u64,  // 分片结束位置
    pub downloaded: bool,  // 分片是否已下载
    pub retry_count: u32,  // 重试次数
    pub last_speed: Option<u64>, // 上次下载速度（字节/秒）
}

impl ChunkProgress {
    // 计算分片大小
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }
}

// 文件进度信息 - 从downloader移植
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileProgress {
    pub url: String,  // 下载文件的 URL
    pub file: String,  // 下载文件的本地路径
    pub total_size: u64,  // 文件总大小
    pub chunks: Vec<ChunkProgress>,  // 分片信息列表
}

impl FileProgress {
    // 构造函数：根据 URL、文件路径、总大小和分片大小创建 FileProgress 实例
    pub fn new(url: &str, file: &str, total_size: u64, chunk_size: u64) -> Self {
        let mut chunks = vec![];
        let mut start = 0;
        let mut idx = 0;
        while start < total_size {
            let end = std::cmp::min(start + chunk_size - 1, total_size - 1);
            chunks.push(ChunkProgress {
                index: idx,
                start,
                end,
                downloaded: false,
                retry_count: 0,
                last_speed: None,
            });
            start = end + 1;
            idx += 1;
        }
        FileProgress {
            url: url.to_string(),
            file: file.to_string(),
            total_size,
            chunks,
        }
    }

    // 保存进度到文件
    pub async fn save_to_file(&self, path: &str) -> anyhow::Result<()> {
        let json = serde_json::to_string(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    // 从文件加载进度
    pub async fn load_from_file(path: &str) -> anyhow::Result<Self> {
        let data = tokio::fs::read_to_string(path).await?;
        let progress = serde_json::from_str(&data)?;
        Ok(progress)
    }

    #[allow(dead_code)]
    pub fn get_progress_path(&self) -> String {
        format!("{}.progress", self.file)
    }

    // 检查是否有未完成的分片
    pub fn has_incomplete_chunks(&self) -> bool {
        self.chunks.iter().any(|c| !c.downloaded)
    }

    // 获取已下载的字节数
    pub fn get_downloaded_bytes(&self) -> u64 {
        self.chunks.iter()
            .filter(|c| c.downloaded)
            .map(|c| c.size())
            .sum()
    }

    #[allow(dead_code)]
    pub fn get_progress_percentage(&self) -> f32 {
        if self.total_size == 0 {
            return 0.0;
        }
        (self.get_downloaded_bytes() as f32 / self.total_size as f32) * 100.0
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DownloadProtocol {
    HTTP,
    HTTPS,
    FTP,
    SFTP,  // 新增 SFTP 协议
    FTPS,  // 新增 FTPS 协议
    // 磁力链
    Magnet,
    // BT种子
    BT,

}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum TaskEvent {
    Started,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub usize);

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub urls: Vec<String>,
    pub status: TaskStatus,
    pub progress: f32, // 下载进度
    pub speed: u64, // 下载速度
    pub size: u64, // 文件大小
    pub is_paused: Arc<AtomicBool>,
    pub is_cancelled: Arc<AtomicBool>,
    pub event_sender: Option<UnboundedSender<TaskEvent>>,
    #[allow(dead_code)]
    pub file_progress: Option<FileProgress>, // 新增：文件进度信息
}

pub struct TaskManager {
    pub tasks: Arc<Mutex<HashMap<TaskId, DownloadTask>>>,
    pub task_counter: AtomicUsize,
    // 动态分片配置 - 从downloader移植
    pub current_chunks: Arc<AtomicUsize>,
    pub max_chunks: usize,
    pub min_chunks: usize,
}

impl TaskManager {
    pub fn new() -> Self {
        TaskManager {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            task_counter: AtomicUsize::new(0),
            current_chunks: Arc::new(AtomicUsize::new(4)), // 初始分片数
            max_chunks: 16,
            min_chunks: 1,
        }
    }

    // 设置动态分片配置
    #[allow(dead_code)]
    pub fn set_chunk_config(&mut self, max_chunks: usize, min_chunks: usize) {
        self.max_chunks = max_chunks;
        self.min_chunks = min_chunks;
    }

    // 获取当前分片数
    pub fn get_current_chunks(&self) -> usize {
        self.current_chunks.load(Ordering::SeqCst)
    }

    // 增加分片数
    pub fn increase_chunks(&self) {
        let current = self.current_chunks.load(Ordering::SeqCst);
        if current < self.max_chunks {
            self.current_chunks.fetch_add(1, Ordering::SeqCst);
            println!("[动态分片] 提高并发分片数: {} -> {}", current, current + 1);
        }
    }

    // 减少分片数
    pub fn decrease_chunks(&self) {
        let current = self.current_chunks.load(Ordering::SeqCst);
        if current > self.min_chunks {
            self.current_chunks.fetch_sub(1, Ordering::SeqCst);
            println!("[动态分片] 降低并发分片数: {} -> {}", current, current - 1);
        }
    }

    // 文件创建方法 - 从downloader移植
    pub async fn create_file(path: &str) -> Result<tokio::fs::File> {
        tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create file: {}", e))
    }

    pub async fn add_task(&self, urls: Vec<String>, event_sender: Option<UnboundedSender<TaskEvent>>) -> TaskId {
        let id = TaskId(self.task_counter.fetch_add(1, Ordering::SeqCst));
        
        let task = DownloadTask {
            urls,
            status: TaskStatus::Pending,
            progress: 0.0,
            speed: 0,
            size: 0,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            event_sender,
            file_progress: None,
        };
        
        self.tasks.lock().await.insert(id.clone(), task);
        id
    }

    pub async fn update_task_status(&self, id: &TaskId, status: TaskStatus) {
        if let Some(task) = self.tasks.lock().await.get_mut(id) {
            task.status = status.clone();
            
            // 发送状态变更事件
            if let Some(sender) = &task.event_sender {
                let event = match status {
                    TaskStatus::Running => TaskEvent::Started,
                    TaskStatus::Completed => TaskEvent::Completed,
                    TaskStatus::Failed(_) => TaskEvent::Failed,
                    _ => return,
                };
                let _ = sender.send(event);
            }
        }
    }

    pub async fn update_task_progress(&self, id: &TaskId, progress: f32, speed: u64, size: u64) {
        if let Some(task) = self.tasks.lock().await.get_mut(id) {
            task.progress = progress;
            task.speed = speed;
            task.size = size;
            
            // 发送进度事件
            if let Some(sender) = &task.event_sender {
                let _ = sender.send(TaskEvent::Progress);
            }
        }
    }

    #[allow(dead_code)]
    pub async fn update_file_progress(&self, id: &TaskId, file_progress: FileProgress) {
        if let Some(task) = self.tasks.lock().await.get_mut(id) {
            task.file_progress = Some(file_progress);
        }
    }

    pub async fn is_task_paused(&self, id: &TaskId) -> Option<bool> {
        self.tasks.lock().await.get(id).map(|task| task.is_paused.load(Ordering::SeqCst))
    }

    pub async fn is_task_cancelled(&self, id: &TaskId) -> Option<bool> {
        self.tasks.lock().await.get(id).map(|task| task.is_cancelled.load(Ordering::SeqCst))
    }

    // 从URL提取文件名 - 从downloader移植
    pub fn get_filename_from_url(url: &str) -> String {
        url.split('/')
            .last()
            .unwrap_or("downloaded_file")
            .to_string()
    }
}

impl Clone for TaskManager {
    fn clone(&self) -> Self {
        TaskManager {
            tasks: Arc::clone(&self.tasks),
            task_counter: AtomicUsize::new(self.task_counter.load(Ordering::SeqCst)),
            current_chunks: Arc::new(AtomicUsize::new(self.current_chunks.load(Ordering::SeqCst))),
            max_chunks: self.max_chunks,
            min_chunks: self.min_chunks,
        }
    }
}

// DownloadTask 只保留new方法
impl DownloadTask {
    pub fn new(url: String) -> Self {
        DownloadTask {
            urls: vec![url],
            status: TaskStatus::Pending,
            progress: 0.0,
            speed: 0,
            size: 0,
            is_paused: Arc::new(AtomicBool::new(false)),
            is_cancelled: Arc::new(AtomicBool::new(false)),
            event_sender: None,
            file_progress: None,
        }
    }
}

impl FromStr for DownloadProtocol {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "http" => Ok(DownloadProtocol::HTTP),
            "https" => Ok(DownloadProtocol::HTTPS),
            "ftp" => Ok(DownloadProtocol::FTP),
            "sftp" => Ok(DownloadProtocol::SFTP),
            "ftps" => Ok(DownloadProtocol::FTPS),
            "magnet" => Ok(DownloadProtocol::Magnet),
            "bt" => Ok(DownloadProtocol::BT),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_task_creation() {
        let task = DownloadTask::new("https://example.com/file.txt".to_string());
        assert_eq!(task.urls.len(), 1);
        assert_eq!(task.urls[0], "https://example.com/file.txt");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.progress, 0.0);
    }

    #[test]
    fn test_protocol_detection() {
        assert_eq!(DownloadProtocol::from_str("http").unwrap(), DownloadProtocol::HTTP);
        assert_eq!(DownloadProtocol::from_str("https").unwrap(), DownloadProtocol::HTTPS);
        assert_eq!(DownloadProtocol::from_str("ftp").unwrap(), DownloadProtocol::FTP);
        assert_eq!(DownloadProtocol::from_str("sftp").unwrap(), DownloadProtocol::SFTP);
        assert_eq!(DownloadProtocol::from_str("ftps").unwrap(), DownloadProtocol::FTPS);
        assert_eq!(DownloadProtocol::from_str("magnet").unwrap(), DownloadProtocol::Magnet);
        assert_eq!(DownloadProtocol::from_str("bt").unwrap(), DownloadProtocol::BT);
        assert!(DownloadProtocol::from_str("invalid").is_err());
    }

    #[test]
    fn test_filename_extraction() {
        assert_eq!(TaskManager::get_filename_from_url("https://example.com/file.txt"), "file.txt");
        assert_eq!(TaskManager::get_filename_from_url("https://example.com/path/to/file.zip"), "file.zip");
        assert_eq!(TaskManager::get_filename_from_url("https://example.com/"), "downloaded_file");
    }

    #[test]
    fn test_file_progress_creation() {
        let progress = FileProgress::new("https://example.com/file.txt", "/tmp/file.txt", 1000, 100);
        assert_eq!(progress.url, "https://example.com/file.txt");
        assert_eq!(progress.file, "/tmp/file.txt");
        assert_eq!(progress.total_size, 1000);
        assert_eq!(progress.chunks.len(), 10); // 1000/100 = 10 chunks
        assert_eq!(progress.get_progress_percentage(), 0.0);
    }
} 