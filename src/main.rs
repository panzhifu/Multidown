use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::time::Duration;
use std::process;

mod cli;
mod core;
mod ui;
mod utils;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    utils::logger::init_logger();
    
    // 解析命令行参数并加载配置
    let (args, config) = cli::Args::parse_args()?;
    
    // url为空报错处理
    if args.urls.is_empty() {
        log::error!("未提供下载URL");
        println!("请提供至少一个URL进行下载。");
        println!("示例: cargo run -- https://example.com/file1.zip https://example.com/file2.zip");
        process::exit(1);
    }

    // 记录命令行参数和配置
    log::info!("开始下载 {} 个文件", args.urls.len());
    for (i, url) in args.urls.iter().enumerate() {
        log::info!("文件 {}: {}", i + 1, url);
    }
    log::info!("配置: 并发数={}, 线程数={}, 速度限制={}MB/s, 输出目录={}",
        config.max_concurrent_downloads,
        config.default_threads,
        config.default_speed_limit,
        config.default_output_dir
    );

    // 创建下载任务列表
    let tasks: Vec<core::DownloadTask> = args.urls.iter()
        .map(|url| core::DownloadTask::new(url.clone()))
        .collect();

    // 创建下载器实例，使用配置的并发数
    let downloader = Arc::new(Mutex::new(core::Downloader::new(config.max_concurrent_downloads))); 
    
    // 启用原始模式以捕获键盘事件
    enable_raw_mode()?;
    
    // 创建键盘事件处理器
    let downloader_clone = Arc::clone(&downloader);
    let keyboard_handler = tokio::spawn(async move {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(50)) {
                if let Ok(Event::Key(key)) = event::read() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            log::info!("用户按下 q 键，暂停下载");
                            println!("\n正在暂停下载...");
                            let downloader = downloader_clone.lock().await;
                            downloader.stop().await;
                            process::exit(0);
                        },
                        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                            log::info!("用户按下 Ctrl+C，暂停下载");
                            println!("\n正在暂停下载...");
                            let downloader = downloader_clone.lock().await;
                            downloader.stop().await;
                            process::exit(0);
                        },
                        _ => {}
                    }
                }
            }
        }
    });

    let downloader = downloader.lock().await;
    let download_result = downloader.download_multiple(
        tasks,
        &config.default_output_dir,
        config.default_threads as u32
    ).await;
    
    // 禁用原始模式
    disable_raw_mode()?;
    
    // 等待键盘事件处理器完成
    keyboard_handler.abort();

    match download_result {
        Ok(_) => {
            log::info!("所有下载任务完成");
            ui::print_success("所有文件下载完成！");
            process::exit(0);
        },
        Err(e) => {
            log::error!("下载任务失败: {}", e);
            ui::print_error(&format!("下载失败: {}", e));
            process::exit(1);
        }
    }
}
