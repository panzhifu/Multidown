// 下载管理器
// 定义下载协议
pub enum DownloadProtocol {
    HTTP,
    HTTPS,
    FTP,
    SFTP,
    Bittorrent,
    Metalink,
    // 添加更多协议
}
// 任务状态
pub enum TaskStatus {
    Pending, // 等待
    Downloading, 
    Paused,
    Completed,
    Failed,
}

// 下载任务
pub struct DownloadTask {
    pub url: String,
    pub protocol: DownloadProtocol,
    pub status: TaskStatus,
    pub progress: f32,
    pub speed: u64,
    pub size: u64,
    pub start_time: u64,
}