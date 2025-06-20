use anyhow::Result; // 用于处理错误
use std::sync::Arc; // Arc用于跨线程共享数据
use tokio::sync::Mutex; // Mutex用于线程安全的锁
use crossterm::{ 
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode}, // 用于控制键盘输入
};
use std::time::Duration; // 用于设置定时器
use std::process; // 用于退出程序

mod cli;
mod core;
mod ui;
mod utils;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    utils::logger::init_logger()?;
    
    // 解析命令行参数并加载配置
    let (args, config) = cli::Args::parse_args()?;
    
    // 获取URL列表
    let urls = args.get_urls()?;
    
    // 记录命令行参数和配置
    log::info!("开始下载 {} 个文件", urls.len());
    for (i, url) in urls.iter().enumerate() {
        log::info!("文件 {}: {}", i + 1, url);
    }
    log::info!("配置: 并发数={}, 线程数={}, 速度限制={}MB/s, 输出目录={}",
        config.max_concurrent_downloads,
        config.default_threads,
        config.default_speed_limit,
        config.default_output_dir
    );

    // 创建下载任务列表
    let tasks: Vec<core::DownloadTask> = urls.iter()
        .map(|url| core::DownloadTask::new(url.clone()))
        .collect();

    // 创建下载器实例，使用配置的并发数
    let downloader = Arc::new(Mutex::new(core::Downloader::new(
        config.chunk_size as u64,
        config.max_concurrent_downloads
    )));
    
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
                            log::info!("用户按下 q 键，保存进度并退出");
                            println!("\n正在保存进度并退出...");
                            let downloader = downloader_clone.lock().await;
                            downloader.stop().await;
                            process::exit(0);
                        },
                        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                            log::info!("用户按下 Ctrl+C，保存进度并退出");
                            println!("\n正在保存进度并退出...");
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
        &config.default_output_dir
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
