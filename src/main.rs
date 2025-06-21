use actix::Actor;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::time::Duration;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode};

use multidown::cli;
use multidown::config;
use multidown::core;

#[actix::main]
async fn main() {
    let args = cli::Args::parse();
    
    // Logger actor needs to be created with a file handle
    // let log_file = std::fs::File::create("logs/multidown.log").unwrap();
    // let logger = utils::logger::LoggerActor { file: log_file, level: LevelFilter::Info }.start();
    
    let mut config = config::Config::load(&args.config).unwrap_or_default();
    args.merge_into_config(&mut config);
    
    let manager = core::actor_manager::DownloadManagerActor::new(config.clone()).start();
    // let ui_addr = ui::UiActor::new_with_manager(progress_manager.clone()).start();
    
    if let Ok(urls) = args.get_urls() {
        let mut task_ids = Vec::new();
        let mut _total_size = 0u64;
        for url in &urls {
            let file = url.split('/').last().unwrap_or("downloaded_file").to_string();
            let id: uuid::Uuid = match manager.send(core::actor_manager::CreateTask { url: url.clone(), file }).await {
                Ok(Ok(val)) => val,
                _ => continue,
            };
            // 查询每个任务的总大小
            if let Ok(Some(detail)) = manager.send(core::actor_manager::QueryTaskDetail(id)).await {
                _total_size += detail.total;
            }
            task_ids.push(id);
        }
        let progress_manager = multidown::ui::ProgressManager::new(_total_size);
        for id in &task_ids {
            let _ = manager.send(core::actor_manager::StartTaskFromMeta { task_id: *id }).await;
        }
        enable_raw_mode().unwrap();
        let mut should_exit = false;
        let mut last_all_paused = false;
        while !should_exit {
            let _stats = match manager.send(core::actor_manager::GetStats).await {
                Ok(stats) => stats,
                _ => break,
            };
            let mut _has_running = false;
            let mut _has_paused = false;
            let mut _downloaded = 0u64;
            for id in &task_ids {
                if let Ok(Some(detail)) = manager.send(core::actor_manager::QueryTaskDetail(*id)).await {
                    _downloaded += detail.downloaded;
                    if detail.progress < 100.0 {
                        _has_running = true;
                    }
                }
                if let Ok(Ok(status)) = manager.send(core::actor_manager::QueryTaskStatus(*id)).await {
                    if status == core::task::state::TaskStatus::Paused {
                        _has_paused = true;
                    }
                }
            }
            
            progress_manager.update_progress(_downloaded, 0);

            // 键盘事件监听
            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Event::Key(key_event) = event::read().unwrap() {
                    match key_event.code {
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            for id in &task_ids {
                                let _ = manager.send(core::actor_manager::PauseTask(*id)).await;
                            }
                            println!("所有任务已暂停");
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            for id in &task_ids {
                                let _ = manager.send(core::actor_manager::CancelTask(*id)).await;
                            }
                            println!("所有任务已取消");
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            for id in &task_ids {
                                if let Ok(Ok(status)) = manager.send(core::actor_manager::QueryTaskStatus(*id)).await {
                                    if status == core::task::state::TaskStatus::Paused {
                                        let _ = manager.send(core::actor_manager::StartTaskFromMeta { task_id: *id }).await;
                                    }
                                }
                            }
                            println!("所有暂停任务已恢复");
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            println!("用户退出");
                            should_exit = true;
                        }
                        _ => {}
                    }
                }
            }

            // 只在状态变化时打印"所有任务已暂停"提示
            let all_paused = _has_paused && !_has_running;
            if all_paused && !last_all_paused {
                println!("所有任务已暂停，按 q 退出或 r 恢复");
            }
            last_all_paused = all_paused;

            let mut all_completed_or_terminal = true;
            for id in &task_ids {
                if let Ok(Ok(status)) = manager.send(core::actor_manager::QueryTaskStatus(*id)).await {
                    match status {
                        core::task::state::TaskStatus::Completed |
                        core::task::state::TaskStatus::Failed(_) |
                        core::task::state::TaskStatus::Cancelled => {},
                        _ => {
                            all_completed_or_terminal = false;
                        }
                    }
                } else {
                    all_completed_or_terminal = false;
                }
            }

            if all_completed_or_terminal {
                break;
            }

            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        disable_raw_mode().unwrap();
    } else {
        // Interactive mode (TUI)
        enable_raw_mode().unwrap();
        // ... TUI loop ...
        disable_raw_mode().unwrap();
    }
}
