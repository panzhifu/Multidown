use reqwest::Client;
use std::fs::File;
use std::io::{Write, Seek, SeekFrom};
use std::sync::Arc; // 只需要 Arc
use tokio::sync::Mutex; // 确保这里是 tokio::sync::Mutex
use url::Url;
use anyhow::{Context, Result};

// 重新添加 DownloadProtocol 到导入列表
use crate::DownloadManager::{DownloadTask, TaskStatus, DownloadProtocol}; 

pub struct Downloader {
    client: Client,
}

impl Downloader {
    pub fn new() -> Self {
        Downloader {
            client: Client::new(),
        }
    }

    // 辅助函数，从必应图片搜索URL中提取实际图片URL
    fn extract_image_url(&self, url: &str) -> Option<String> {
        if let Ok(parsed_url) = Url::parse(url) {
            if parsed_url.host_str() == Some("cn.bing.com") {
                // 从查询参数中提取mediaurl
                if let Some(query_pairs) = parsed_url.query_pairs().find(|(key, _)| key == "mediaurl") {
                    return Some(query_pairs.1.to_string());
                }
            }
        }
        None
    }

    pub async fn download(&self, mut task: DownloadTask, output_path: &str, num_blocks: u32) -> Result<()> {
        println!("准备下载: {}", task.url);

        // 根据协议类型进行初步判断
        match task.protocol {
            DownloadProtocol::HTTP | DownloadProtocol::HTTPS => {
                // 继续HTTP/HTTPS下载逻辑
            },
            _ => return Err(anyhow::anyhow!("目前只支持 HTTP 和 HTTPS 协议的下载")),
        }

        // 验证URL并处理必应图片URL
        let parsed_url = Url::parse(&task.url)?;
        if !["http", "https"].contains(&parsed_url.scheme()) {
            return Err(anyhow::anyhow!("只支持 HTTP 和 HTTPS 协议"));
        }

        let download_url = if parsed_url.host_str() == Some("cn.bing.com") {
            if let Some(image_url) = self.extract_image_url(&task.url) {
                println!("从必应图片搜索页面提取到图片URL: {}", image_url);
                image_url
            } else {
                return Err(anyhow::anyhow!("无法从URL中提取图片地址"));
            }
        } else {
            task.url.clone()
        };

        // 获取文件大小
        let response = self.client.get(&download_url).send().await?
            .error_for_status() // 检查HTTP状态码
            .context("获取文件信息失败，非2xx状态码")?;
        
        let total_size = response.content_length().unwrap_or(0);
        
        if total_size == 0 {
            return Err(anyhow::anyhow!("无法获取文件大小，请确保URL指向一个可下载的文件"));
        }

        task.size = total_size;
        println!("开始下载文件，总大小: {} 字节", total_size);
        println!("下载URL: {}", download_url);

        // 如果文件太小，使用单线程下载
        if total_size < 1024 * 1024 * 5 && num_blocks == 1 { // 小于5MB且只分1块时，进行单线程下载
            println!("文件较小或指定单线程，使用单线程下载");
            let bytes = response.bytes().await?;
            let mut file = File::create(output_path)?;
            file.write_all(&bytes)?;
            task.status = TaskStatus::Completed;
            task.progress = 100.0;
            println!("单线程下载完成！");
            return Ok(());
        }

        // 计算每个块的大小
        let block_size = total_size / num_blocks as u64;
        let mut handles = vec![];

        // 创建输出文件
        let file = File::create(output_path)?;
        let file = Arc::new(Mutex::new(file));

        // 启动多个下载任务
        for i in 0..num_blocks {
            let start = i as u64 * block_size;
            let end = if i == num_blocks - 1 {
                total_size - 1
            } else {
                (i + 1) as u64 * block_size - 1
            };

            let client = self.client.clone();
            let url = download_url.clone();
            let file = Arc::clone(&file);

            println!("启动下载块 {}: 字节范围 {}-{}", i + 1, start, end);

            let handle = tokio::spawn(async move {
                let response = client
                    .get(&url)
                    .header("Range", format!("bytes={}-{}", start, end))
                    .send()
                    .await?
                    .error_for_status()
                    .context(format!("下载块 {}: 获取数据失败，非2xx状态码", i + 1))?;

                let bytes = response.bytes().await?;
                let mut file_guard = file.lock().await;
                file_guard.seek(SeekFrom::Start(start))?;
                file_guard.write_all(&bytes)?;
                Ok::<_, anyhow::Error>(())
            });

            handles.push(handle);
        }

        // 等待所有下载任务完成
        for (i, handle) in handles.into_iter().enumerate() {
            handle.await? // 捕获spawn内部的panic
                .context(format!("下载块 {} 任务失败", i + 1))?; // 捕获内部的Result错误
            println!("下载块 {} 完成", i + 1);
        }

        task.status = TaskStatus::Completed;
        task.progress = 100.0;
        println!("所有下载任务完成！");
        Ok(())
    }
} 