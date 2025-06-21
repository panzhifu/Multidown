use actix::prelude::*;
use crossterm::{event::{self, Event, KeyCode, KeyModifiers}, terminal::{disable_raw_mode, enable_raw_mode}};
use std::time::Duration;
use std::process;

mod cli;
mod core;
mod ui;
mod utils;
mod config;

#[actix::main]
async fn main() -> Result<(), core::error::DownloadError> {
    // 检查--version参数
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        cli::Args::show_version();
        return Ok(());
    }
    // 启动 LoggerActor
    let logger = utils::logger::LoggerActor {
        file: std::fs::File::create("logs/actix_log.log").map_err(core::error::DownloadError::IoError)?,
        level: log::LevelFilter::Info,
    }.start();
    // 启动CLI Actor
    let cli_addr = cli::CliActor.start();
    // 通过消息解析命令行参数和配置
    let (args, config) = cli_addr.send(cli::ParseArgs).await
        .map_err(|e| core::error::DownloadError::Unknown(format!("CLI actor 消息失败: {}", e)))?
        .map_err(|e| core::error::DownloadError::Unknown(format!("参数解析失败: {}", e)))?;
    // 通过消息获取URL列表
    let urls = cli_addr.send(cli::GetUrls(args.clone())).await
        .map_err(|e| core::error::DownloadError::Unknown(format!("CLI actor 消息失败: {}", e)))?
        .map_err(|e| core::error::DownloadError::Unknown(format!("URL获取失败: {}", e)))?;
    // 记录命令行参数和配置
    logger.do_send(utils::logger::LogMsg {
        level: log::LevelFilter::Info,
        message: format!("开始下载 {} 个文件", urls.len()),
    });
    for (i, url) in urls.iter().enumerate() {
        logger.do_send(utils::logger::LogMsg {
            level: log::LevelFilter::Info,
            message: format!("文件 {}: {}", i + 1, url),
        });
    }
    logger.do_send(utils::logger::LogMsg {
        level: log::LevelFilter::Info,
        message: format!("配置: 并发数={}, 线程数={}, 速度限制={}MB/s, 输出目录={}",
            config.max_concurrent_downloads,
            config.default_threads,
            config.default_speed_limit,
            config.default_output_dir),
    });
    // 创建全局任务管理器 Actor
    let manager = core::actor_manager::DownloadManagerActor::new(config.clone()).start();
    let ui_addr = ui::UiActor::new().start();
    let mut task_ids = Vec::new();
    // 批量添加任务
    for url in urls {
        let file = url.split('/').last().unwrap_or("downloaded_file").to_string();
        let id = match manager.send(core::actor_manager::AddTask { url: url.clone(), file, tags: Vec::new() }).await {
            Ok(Ok(val)) => val,
            Ok(Err(e)) => {
                logger.do_send(utils::logger::LogMsg {
                    level: log::LevelFilter::Error,
                    message: format!("添加任务失败: {}", e),
                });
                ui::print_error(&format!("添加任务失败: {}", e));
                continue;
            },
            Err(e) => {
                logger.do_send(utils::logger::LogMsg {
                    level: log::LevelFilter::Error,
                    message: format!("任务管理器消息失败: {}", e),
                });
                ui::print_error(&format!("任务管理器消息失败: {}", e));
                continue;
            }
        };
        task_ids.push(id);
    }
    // 启动所有任务
    for id in &task_ids {
        let _ = manager.send(core::actor_manager::StartTaskById { task_id: *id }).await;
    }
    // 启用原始模式以捕获键盘事件
    enable_raw_mode().map_err(core::error::DownloadError::IoError)?;
    let keyboard_handler = actix::spawn(async move {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(50)) {
                if let Ok(Event::Key(key)) = event::read() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            println!("\n正在退出...");
                            process::exit(0);
                        },
                        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                            println!("\n正在退出...");
                            process::exit(0);
                        },
                        _ => {}
                    }
                }
            }
        }
    });
    // 简单轮询进度
    loop {
        let mut all_done = true;
        for id in &task_ids {
            let status = match manager.send(core::actor_manager::QueryTaskStatusById { task_id: *id }).await {
                Ok(Ok(val)) => val,
                Ok(Err(e)) => {
                    logger.do_send(utils::logger::LogMsg {
                        level: log::LevelFilter::Error,
                        message: format!("查询任务状态失败: {}", e),
                    });
                    all_done = false;
                    continue;
                },
                Err(e) => {
                    logger.do_send(utils::logger::LogMsg {
                        level: log::LevelFilter::Error,
                        message: format!("任务管理器消息失败: {}", e),
                    });
                    all_done = false;
                    continue;
                }
            };
            match status {
                Some(core::actor_task::TaskStatus::Completed) => {},
                Some(core::actor_task::TaskStatus::Failed(ref e)) => {
                    logger.do_send(utils::logger::LogMsg {
                        level: log::LevelFilter::Error,
                        message: format!("任务 {:?} 失败: {}", id, e),
                    });
                    all_done = false;
                },
                Some(_) => {
                    all_done = false;
                },
                None => all_done = false,
            }
            // 获取详细进度
            let detail = match manager.send(core::actor_manager::QueryTaskDetailById { task_id: *id }).await {
                Ok(Ok(val)) => val,
                _ => None,
            };
            if let Some(detail) = detail {
                ui_addr.do_send(ui::UpdateProgressMsg {
                    task_id: id.to_string(),
                    progress: detail.progress,
                    speed: detail.size, // TODO: 速度统计
                    size: detail.size,
                });
            }
        }
        if all_done {
            break;
        }
        actix::clock::sleep(Duration::from_secs(1)).await;
    }
    // 禁用原始模式
    disable_raw_mode().map_err(core::error::DownloadError::IoError)?;
    keyboard_handler.abort();
    logger.do_send(utils::logger::LogMsg {
        level: log::LevelFilter::Info,
        message: "所有下载任务完成".to_string(),
    });
    ui::print_success("所有文件下载完成！");
    Ok(())
}
