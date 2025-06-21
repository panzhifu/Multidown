use std::sync::{Arc, Mutex};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::core::error::DownloadError;
use crate::core::actor_manager::ResumeInfo;
use super::retry::{RetryContext, RetryStrategy, RetryStats};
use super::util::FileInfo;

use actix::{Context, AsyncContext};
use super::actor::DownloadTaskActor;
use super::messages::DownloadChunkMsg;

/// 下载块结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadChunk {
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub completed: bool,
}

/// 分块下载统计信息
#[derive(Debug, Clone)]
pub struct ChunkDownloadStats {
    pub total_chunks: usize,
    pub completed_chunks: usize,
    pub active_chunks: usize,
    pub failed_chunks: usize,
    pub pending_chunks: usize,
    pub progress: f32,
}

/// 分块下载管理器
#[derive(Debug)]
pub struct ChunkedDownloadManager {
    pub chunks: Vec<DownloadChunk>,
    pub total_size: u64,
    pub temp_dir: String,
    pub file_name: String,
    pub active_chunks: Arc<Mutex<Vec<usize>>>,
    pub completed_chunks: Arc<Mutex<Vec<usize>>>,
    pub failed_chunks: Arc<Mutex<Vec<usize>>>,
    pub max_concurrent_chunks: usize,
    pub retry_context: RetryContext,
}

impl ChunkedDownloadManager {
    pub fn new(total_size: u64, chunk_size: u64, file_name: String) -> Self {
        let num_chunks = ((total_size + chunk_size - 1) / chunk_size) as usize;
        let mut chunks = Vec::new();
        
        for i in 0..num_chunks {
            let start = i as u64 * chunk_size;
            let end = if i == num_chunks - 1 {
                total_size - 1
            } else {
                (i + 1) as u64 * chunk_size - 1
            };
            
            chunks.push(DownloadChunk {
                start,
                end,
                downloaded: 0,
                completed: false,
            });
        }
        
        // 使用文件名作为临时目录名，避免路径过长
        let temp_dir = format!("downloads/temp/{}", file_name.replace("/", "_").replace("\\", "_"));
        std::fs::create_dir_all(&temp_dir).ok();
        
        Self {
            chunks,
            total_size,
            temp_dir,
            file_name,
            active_chunks: Arc::new(Mutex::new(Vec::new())),
            completed_chunks: Arc::new(Mutex::new(Vec::new())),
            failed_chunks: Arc::new(Mutex::new(Vec::new())),
            max_concurrent_chunks: 3, // 默认最大并发块数
            retry_context: RetryContext::new(RetryStrategy::default()),
        }
    }
    
    /// 设置最大并发块数
    pub fn set_max_concurrent_chunks(&mut self, max: usize) {
        self.max_concurrent_chunks = max;
    }
    
    /// 获取下一个可下载的块
    pub fn get_next_available_chunk(&mut self) -> Option<(usize, &mut DownloadChunk)> {
        let active_count = self.active_chunks.lock().unwrap().len();
        
        // 检查是否达到最大并发数
        if active_count >= self.max_concurrent_chunks {
            return None;
        }
        
        // 先收集可用的块索引
        let mut available_indices = Vec::new();
        for (i, chunk) in self.chunks.iter().enumerate() {
            if !chunk.completed && !self.is_chunk_active(i) && !self.is_chunk_failed(i) {
                available_indices.push(i);
            }
        }
        
        // 如果有可用块，返回第一个
        if let Some(&chunk_index) = available_indices.first() {
            // 标记为活跃
            self.active_chunks.lock().unwrap().push(chunk_index);
            // 返回可变引用
            if let Some(chunk) = self.chunks.get_mut(chunk_index) {
                return Some((chunk_index, chunk));
            }
        }
        
        None
    }
    
    /// 检查块是否正在下载
    pub fn is_chunk_active(&self, chunk_index: usize) -> bool {
        self.active_chunks.lock().unwrap().contains(&chunk_index)
    }
    
    /// 检查块是否下载失败
    pub fn is_chunk_failed(&self, chunk_index: usize) -> bool {
        self.failed_chunks.lock().unwrap().contains(&chunk_index)
    }
    
    /// 标记块为完成
    pub fn mark_chunk_completed(&mut self, chunk_index: usize) {
        if let Some(chunk) = self.chunks.get_mut(chunk_index) {
            chunk.completed = true;
            chunk.downloaded = chunk.end - chunk.start + 1;
        }
        
        // 从活跃列表中移除
        if let Ok(mut active) = self.active_chunks.lock() {
            active.retain(|&x| x != chunk_index);
        }
        
        // 添加到完成列表
        if let Ok(mut completed) = self.completed_chunks.lock() {
            completed.push(chunk_index);
        }
        
        // 从失败列表中移除（如果存在）
        if let Ok(mut failed) = self.failed_chunks.lock() {
            failed.retain(|&x| x != chunk_index);
        }
    }
    
    /// 标记块为失败
    pub fn mark_chunk_failed(&mut self, chunk_index: usize) {
        // 从活跃列表中移除
        if let Ok(mut active) = self.active_chunks.lock() {
            active.retain(|&x| x != chunk_index);
        }
        
        // 添加到失败列表
        if let Ok(mut failed) = self.failed_chunks.lock() {
            if !failed.contains(&chunk_index) {
                failed.push(chunk_index);
            }
        }
    }
    
    /// 获取失败的重试块
    pub fn get_failed_chunks_for_retry(&mut self) -> Vec<usize> {
        let failed_chunks = self.failed_chunks.lock().unwrap().clone();
        let mut retry_chunks = Vec::new();
        
        for chunk_index in failed_chunks {
            if self.retry_context.should_retry(&DownloadError::Unknown("chunk_download_failed".to_string())) {
                retry_chunks.push(chunk_index);
            }
        }
        
        retry_chunks
    }
    
    /// 更新块下载进度
    pub fn update_chunk_progress(&mut self, chunk_index: usize, downloaded: u64) {
        if let Some(chunk) = self.chunks.get_mut(chunk_index) {
            chunk.downloaded = downloaded;
        }
    }
    
    /// 获取总体下载进度
    pub fn get_total_progress(&self) -> f32 {
        let total_downloaded: u64 = self.chunks.iter().map(|c| c.downloaded).sum();
        if self.total_size > 0 {
            (total_downloaded as f32 / self.total_size as f32) * 100.0
        } else {
            0.0
        }
    }
    
    /// 检查是否所有块都完成
    pub fn is_completed(&self) -> bool {
        self.chunks.iter().all(|chunk| chunk.completed)
    }
    
    /// 获取下载统计信息
    pub fn get_stats(&self) -> ChunkDownloadStats {
        let total_chunks = self.chunks.len();
        let completed_count = self.completed_chunks.lock().unwrap().len();
        let active_count = self.active_chunks.lock().unwrap().len();
        let failed_count = self.failed_chunks.lock().unwrap().len();
        let pending_count = total_chunks - completed_count - active_count - failed_count;
        
        ChunkDownloadStats {
            total_chunks,
            completed_chunks: completed_count,
            active_chunks: active_count,
            failed_chunks: failed_count,
            pending_chunks: pending_count,
            progress: self.get_total_progress(),
        }
    }
    
    pub fn get_chunk_file_path(&self, chunk_index: usize) -> String {
        format!("{}/chunk_{:04}", self.temp_dir, chunk_index)
    }
    
    pub fn merge_chunks(&self, output_path: &str) -> Result<(), DownloadError> {
        let mut output_file = std::fs::File::create(output_path)
            .map_err(|e| DownloadError::IoError(e.to_string()))?;
        
        for (i, chunk) in self.chunks.iter().enumerate() {
            let chunk_path = self.get_chunk_file_path(i);
            if let Ok(mut chunk_file) = std::fs::File::open(&chunk_path) {
                std::io::copy(&mut chunk_file, &mut output_file)
                    .map_err(|e| DownloadError::IoError(e.to_string()))?;
            } else {
                return Err(DownloadError::Unknown(format!("无法打开块文件: {}", chunk_path)));
            }
        }
        
        // 清理临时文件
        self.cleanup_temp_files();
        Ok(())
    }
    
    pub fn cleanup_temp_files(&self) {
        if let Err(_e) = std::fs::remove_dir_all(&self.temp_dir) {
            // println!("[chunked_download] 清理临时文件失败: {}", e);
        }
    }
    
    pub fn save_resume_info(&self, task_id: Uuid, url: &str, file_info: &FileInfo) -> Result<(), DownloadError> {
        let resume_info = ResumeInfo {
            task_id,
            url: url.to_string(),
            file: self.file_name.clone(),
            downloaded_chunks: self.chunks.iter()
                .enumerate()
                .filter(|(_, chunk)| chunk.completed)
                .map(|(_, chunk)| (chunk.start, chunk.end))
                .collect(),
            total_size: self.total_size,
            last_modified: file_info.last_modified.clone(),
            etag: file_info.etag.clone(),
        };
        
        let path = format!("downloads/resume_{}.json", task_id);
        let json = serde_json::to_string_pretty(&resume_info)
            .map_err(|e| DownloadError::Unknown(format!("序列化失败: {}", e)))?;
        
        std::fs::write(path, json)
            .map_err(|e| DownloadError::IoError(e.to_string()))?;
        Ok(())
    }
    
    pub fn load_and_validate_resume_info(&mut self, task_id: Uuid, current_file_info: &FileInfo) -> Result<(), DownloadError> {
        let path = format!("downloads/resume_{}.json", task_id);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Ok(()), // No resume file, not an error, just continue fresh.
        };
        
        let resume_info: ResumeInfo = serde_json::from_str(&content)
            .map_err(|e| DownloadError::Unknown(format!("反序列化失败: {}", e)))?;

        // --- VALIDATION LOGIC ---
        // 1. ETag check (primary)
        if let (Some(old_etag), Some(new_etag)) = (&resume_info.etag, &current_file_info.etag) {
            if old_etag != new_etag {
                return Err(DownloadError::ResumeFailed("ETag mismatch, file has changed.".to_string()));
            }
        }
        // 2. Last-Modified check (fallback)
        else if let (Some(old_lm), Some(new_lm)) = (&resume_info.last_modified, &current_file_info.last_modified) {
             if old_lm != new_lm {
                return Err(DownloadError::ResumeFailed("Last-Modified mismatch, file has changed.".to_string()));
            }
        }
        // If one has info and the other doesn't, it's ambiguous. Let's be strict.
        else if resume_info.etag.is_some() != current_file_info.etag.is_some() || resume_info.last_modified.is_some() != current_file_info.last_modified.is_some() {
             return Err(DownloadError::ResumeFailed("Inconsistent resume headers (ETag/Last-Modified).".to_string()));
        }

        // --- RESTORE STATE ---
        for (start, end) in &resume_info.downloaded_chunks {
            if let Some(chunk) = self.chunks.iter_mut()
                .find(|c| c.start == *start && c.end == *end) {
                chunk.completed = true;
                chunk.downloaded = end - start + 1;
                
                // 添加到完成列表
                if let Ok(mut completed) = self.completed_chunks.lock() {
                    if let Some(index) = self.chunks.iter().position(|c| c.start == *start && c.end == *end) {
                        completed.push(index);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 重试失败的块
    pub fn retry_failed_chunks(&mut self, ctx: &mut Context<DownloadTaskActor>, url: &str, file: &str, task_id: Uuid) {
        let failed_chunks = self.get_failed_chunks_for_retry();
        
        for chunk_index in failed_chunks {
            if let Some(chunk) = self.chunks.get(chunk_index) {
                // 从失败列表中移除
                if let Ok(mut failed) = self.failed_chunks.lock() {
                    failed.retain(|&x| x != chunk_index);
                }
                
                // 重新发送下载消息
                ctx.address().do_send(DownloadChunkMsg {
                    chunk_index,
                    url: url.to_string(),
                    file: file.to_string(),
                    start: chunk.start,
                    end: chunk.end,
                    task_id,
                });
            }
        }
    }
    
    /// 检查是否需要重试
    pub fn should_retry_failed_chunks(&self) -> bool {
        let failed_count = self.failed_chunks.lock().unwrap().len();
        failed_count > 0 && self.retry_context.retry_count < self.retry_context.strategy.max_retries
    }
    
    /// 获取重试统计信息
    pub fn get_retry_stats(&self) -> RetryStats {
        self.retry_context.get_retry_stats()
    }
    
    /// 重置重试状态
    pub fn reset_retry_state(&mut self) {
        self.retry_context.reset();
        if let Ok(mut failed) = self.failed_chunks.lock() {
            failed.clear();
        }
    }
} 