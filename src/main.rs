use clap::Parser;
use anyhow::Result;
use crate::DownloadManager::{DownloadProtocol, DownloadTask, TaskStatus}; 

mod cli;
mod downloader;
mod DownloadManager;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Args::parse();
    
    println!("URLs: {:?}", args.urls);
    println!("Threads: {}", args.threads);
    println!("Output: {}", args.output);
    println!("Speed limit: {}MB/s", args.limit);
    println!("Config: {}", args.config);

    if let Some(url_str) = args.urls.get(0) {
        let task = DownloadTask { // 直接使用 DownloadTask
            url: url_str.clone(),
            protocol: DownloadProtocol::HTTP, // 直接使用 DownloadProtocol
            status: TaskStatus::Pending, // 直接使用 TaskStatus
            progress: 0.0,
            speed: 0,
            size: 0,
            start_time: 0,
        };

        let downloader = downloader::Downloader::new();
        match downloader.download(task, &args.output, args.threads as u32).await {
            Ok(_) => println!("文件下载完成！"),
            Err(e) => eprintln!("下载失败: {}", e),
        }
    } else {
        println!("请提供至少一个URL进行下载。例如: cargo run -- -u \"http://example.com/file.zip\" -o \"file.zip\" -n 4");
    }

    Ok(())
}
