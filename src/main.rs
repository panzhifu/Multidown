use multidown::cli;
use multidown::core::actor_manager::*;
use actix::prelude::*;
use multidown::utils::logger::{LoggerActor, LoggerExt};
use log::LevelFilter;
use std::path::Path;
use uuid::Uuid;
use crossterm::{
    cursor, execute, terminal,
    event::{self, Event, KeyCode},
};
use multidown::ui::ProgressManager;

const PROGRESS_UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const KEYBOARD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

#[actix::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logger = LoggerActor::new("logs/app.log", LevelFilter::Info, 10 * 1024 * 1024)?.start();
    logger.info("程序启动");

    // 解析参数和配置
    let (args, config) = match cli::Args::parse_args() {
        Ok((args, config)) => (args, config),
        Err(e) => {
            logger.error(&format!("参数解析失败: {}", e));
            eprintln!("参数解析失败: {}", e);
            std::process::exit(1);
        }
    };

    // 获取下载URL列表
    let urls = match args.get_urls() {
        Ok(urls) => urls,
        Err(e) => {
            logger.error(&format!("获取URL列表失败: {}", e));
            eprintln!("获取URL列表失败: {}", e);
            std::process::exit(1);
        }
    };

    logger.info(&format!("解析到的URLs: {:?}", urls));
    logger.info(&format!("配置文件路径: {}", args.config));
    logger.info(&format!("下载目录: {}", args.download_dir));
    logger.info(&format!("配置摘要:\n{}", config.get_summary()));

    println!("配置加载成功");
    println!("{}", config.get_summary());

    // 创建下载管理器
    let download_manager = DownloadManagerActor::new(config).start();
    logger.info("下载管理器已启动");

    // 创建并启动所有下载任务
    let task_ids = create_and_start_tasks(&download_manager, &args, &urls, &logger).await?;

    if task_ids.is_empty() {
        eprintln!("没有可下载的任务");
        return Ok(());
    }

    println!("\n开始下载... (按 'p' 暂停, 'c' 取消, 'q' 退出)");
    logger.info(&format!("开始下载 {} 个任务", task_ids.len()));

    // 主循环：处理键盘输入和更新进度
    run_download_loop(&download_manager, &task_ids, &logger).await?;

    Ok(())
}

/// 创建并启动所有下载任务
async fn create_and_start_tasks(
    download_manager: &Addr<DownloadManagerActor>,
    args: &cli::Args,
    urls: &[String],
    logger: &Addr<LoggerActor>,
) -> Result<Vec<Uuid>, Box<dyn std::error::Error>> {
    let mut task_ids = Vec::new();
    
    for url in urls {
        let file_name = extract_filename_from_url(url, &args.file_name);
        let file_path = Path::new(&args.download_dir).join(&file_name);
        
        match download_manager.send(CreateTask {
            url: url.clone(),
            file: file_path.to_string_lossy().to_string(),
        }).await {
            Ok(Ok(task_id)) => {
                task_ids.push(task_id);
                logger.info(&format!("创建下载任务: {} -> {}", url, file_name));
                println!("✓ 创建下载任务: {}", file_name);
            }
            Ok(Err(e)) => {
                logger.error(&format!("创建下载任务失败: {} - {}", url, e));
                eprintln!("✗ 创建下载任务失败: {} - {}", url, e);
            }
            Err(e) => {
                logger.error(&format!("发送创建任务消息失败: {} - {}", url, e));
                eprintln!("✗ 发送创建任务消息失败: {} - {}", url, e);
            }
        }
    }

    // 启动所有任务
    for task_id in &task_ids {
        download_manager.do_send(StartTaskFromMeta { task_id: *task_id });
    }

    Ok(task_ids)
}

/// 从URL中提取文件名
fn extract_filename_from_url(url: &str, custom_name: &Option<String>) -> String {
    if let Some(name) = custom_name {
        return name.clone();
    }
    
    // 从URL路径中提取文件名
    if let Some(last_slash) = url.rfind('/') {
        let filename = &url[last_slash + 1..];
        if !filename.is_empty() && !filename.contains('?') {
            return filename.to_string();
        }
    }
    
    // 如果无法从URL提取，使用默认名称
    format!("download_{}", chrono::Utc::now().timestamp())
}

/// 运行下载主循环
async fn run_download_loop(
    download_manager: &Addr<DownloadManagerActor>,
    task_ids: &[Uuid],
    logger: &Addr<LoggerActor>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_update = std::time::Instant::now();

    // 设置终端
    terminal::enable_raw_mode()?;
    execute!(std::io::stdout(), cursor::Hide)?;

    // 创建UI进度管理器
    let stats = download_manager.send(GetStats).await?;
    let progress = ProgressManager::new(stats.total_bytes);

    loop {
        // 处理键盘输入
        if let Ok(true) = event::poll(KEYBOARD_POLL_INTERVAL) {
            if let Ok(Event::Key(key_event)) = event::read() {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        println!("\n用户退出");
                        logger.info("用户主动退出下载");
                        break;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        // 暂停所有任务
                        for task_id in task_ids {
                            download_manager.do_send(PauseTask(*task_id));
                        }
                        println!("\n已暂停所有下载任务");
                        logger.info("用户暂停所有下载任务");
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        // 取消所有任务
                        for task_id in task_ids {
                            download_manager.do_send(CancelTask(*task_id));
                        }
                        println!("\n已取消所有下载任务");
                        logger.info("用户取消所有下载任务");
                        break;
                    }
                    _ => {}
                }
            }
        }

        // 更新进度
        if last_update.elapsed() >= PROGRESS_UPDATE_INTERVAL {
            let stats = download_manager.send(GetStats).await?;
            progress.update_progress(stats.downloaded_bytes, stats.speed);

            // 检查是否所有任务都完成
            if stats.completed + stats.failed == stats.total && stats.total > 0 {
                break;
            }

            last_update = std::time::Instant::now();
        }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // 恢复终端
    execute!(std::io::stdout(), cursor::Show)?;
    terminal::disable_raw_mode()?;
    progress.finish();

    // 显示最终统计
    let final_stats = download_manager.send(GetStats).await?;
    println!("\n下载统计:");
    println!("  总任务数: {}", final_stats.total);
    println!("  成功完成: {}", final_stats.completed);
    println!("  失败: {}", final_stats.failed);
    println!("  暂停: {}", final_stats.paused);

    logger.info(&format!("下载完成 - 成功: {}, 失败: {}", final_stats.completed, final_stats.failed));

    Ok(())
}


