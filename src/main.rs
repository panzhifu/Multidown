use anyhow::Result; // 用于处理错误
use crossterm::{ 
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode}, // 用于控制键盘输入
};
use std::time::Duration; // 用于设置定时器
use std::process; // 用于退出程序
use actix::prelude::*;

mod cli;
mod core;
mod ui;
mod utils;
mod config;

#[actix::main]
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

    // 创建全局任务管理器 Actor
    let manager = core::actor_manager::DownloadManagerActor::new().start();
    let mut task_ids = Vec::new();
    // 批量添加任务
    for url in urls {
        let id = manager.send(core::actor_manager::AddTask { urls: vec![url] }).await.unwrap();
        task_ids.push(id);
    }
    // 启动所有任务
    for id in &task_ids {
        manager.do_send(core::actor_manager::StartTaskById { task_id: *id });
    }

    // 启用原始模式以捕获键盘事件
    enable_raw_mode()?;
    let manager_clone = manager.clone();
    let keyboard_handler = actix::spawn(async move {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(50)) {
                if let Ok(Event::Key(key)) = event::read() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            log::info!("用户按下 q 键，退出");
                            println!("\n正在退出...");
                            // 可扩展：发送取消所有任务消息
                            process::exit(0);
                        },
                        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                            log::info!("用户按下 Ctrl+C，退出");
                            println!("\n正在退出...");
                            process::exit(0);
                        },
                        _ => {}
                    }
                }
            }
        }
    });

    // 简单轮询进度（可扩展为事件驱动）
    loop {
        let mut all_done = true;
        for id in &task_ids {
            let status = manager.send(core::actor_manager::QueryTaskStatusById { task_id: *id }).await.unwrap();
            match status {
                Some(core::actor_task::TaskStatus::Completed) => {},
                Some(core::actor_task::TaskStatus::Failed(e)) => {
                    log::error!("任务{id}失败: {e}");
                    all_done = false;
                },
                Some(_) => {
                    all_done = false;
                },
                None => all_done = false,
            }
            let progress = manager.send(core::actor_manager::QueryTaskProgressById { task_id: *id }).await.unwrap();
            if let Some(p) = progress {
                println!("任务{id}进度: {:.2}%", p);
            }
        }
        if all_done {
            break;
        }
        actix::clock::sleep(Duration::from_secs(1)).await;
    }

    // 禁用原始模式
    disable_raw_mode()?;
    keyboard_handler.abort();
    log::info!("所有下载任务完成");
    ui::print_success("所有文件下载完成！");
    process::exit(0);
}
