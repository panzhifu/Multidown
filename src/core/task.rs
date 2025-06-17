// pub是rust中的一种访问控制关键字，它表示公共的，可以被外部访问。
// 在rust中，变量、函数、结构体等默认都是私有的，只有在其定义的模块中才能访问。

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DownloadProtocol {
    HTTP,
    HTTPS,
    FTP,
    // 磁力链
    Magnet,
    // BT种子
    BT,

}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub url: String,
    pub protocol: DownloadProtocol,
    pub status: TaskStatus,
    pub progress: f32, // 下载进度
    pub speed: u64, // 下载速度
    pub size: u64, // 文件大小
    pub start_time: u64, // 开始时间
}

#[allow(dead_code)]
impl DownloadTask {
    pub fn new(url: String) -> Self {
        //match进行模式匹配，根据url的协议类型，返回不同的DownloadProtocol
        let protocol = match url.split("://").next().unwrap() {
            //split("://")将url按"//"分割，next()返回第一个元素，unwrap()将Option转换为String
            "http" => DownloadProtocol::HTTP,
            "https" => DownloadProtocol::HTTPS,
            "ftp" => DownloadProtocol::FTP,
            "magnet" => DownloadProtocol::Magnet,
            "bt" => DownloadProtocol::BT,
            _ => DownloadProtocol::HTTP,
        };

        DownloadTask {
            url,
            protocol,
            status: TaskStatus::Pending,
            progress: 0.0,
            speed: 0,
            size: 0,
            start_time: 0,
        }
    }

    pub fn update_status(&mut self, status: TaskStatus) {
        self.status = status;
    }

    pub fn update_progress(&mut self, progress: f32, speed: u64, size: u64) {
        self.progress = progress;
        self.speed = speed;
        self.size = size;
    }
}

// 测试代码
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_task_creation() {
        let task = DownloadTask::new("http://example.com/file.zip".to_string());
        assert_eq!(task.url, "http://example.com/file.zip");
        assert_eq!(task.progress, 0.0);
        assert_eq!(task.speed, 0);
        assert!(matches!(task.protocol, DownloadProtocol::HTTP));
        assert!(matches!(task.status, TaskStatus::Pending));
    }

    #[test]
    fn test_protocol_detection() {
        let https_task = DownloadTask::new("https://example.com/file.zip".to_string());
        let http_task = DownloadTask::new("http://example.com/file.zip".to_string());
        let ftp_task = DownloadTask::new("ftp://example.com/file.zip".to_string());
        let unknown_task = DownloadTask::new("example.com/file.zip".to_string());

        assert!(matches!(https_task.protocol, DownloadProtocol::HTTPS));
        assert!(matches!(http_task.protocol, DownloadProtocol::HTTP));
        assert!(matches!(ftp_task.protocol, DownloadProtocol::FTP));
        assert!(matches!(unknown_task.protocol, DownloadProtocol::HTTP));
    }

    #[test]
    fn test_task_status_update() {
        let mut task = DownloadTask::new("http://example.com/file.zip".to_string());
        task.update_status(TaskStatus::Downloading);
        assert!(matches!(task.status, TaskStatus::Downloading));
    }

    #[test]
    fn test_task_progress_update() {
        let mut task = DownloadTask::new("http://example.com/file.zip".to_string());
        task.update_progress(50.0, 1024, 2048);
        assert_eq!(task.progress, 50.0);
        assert_eq!(task.speed, 1024);
        assert_eq!(task.size, 2048);
    }
} 