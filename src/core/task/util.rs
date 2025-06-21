use std::time::{Duration, Instant};
use crate::core::error::DownloadError;
use std::io::Write;
use serde::{Deserialize, Serialize};

/// 文件信息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FileInfo {
    pub size: u64,
    pub supports_range: bool,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

/// 缓冲区管理器
#[allow(dead_code)]
pub struct BufferManager {
    buffer: Vec<u8>,
    buffer_size: usize,
    current_pos: usize,
    file_handle: std::fs::File,
    total_written: u64,
    flush_count: u64,
}

#[allow(dead_code)]
impl BufferManager {
    /// 创建新的 BufferManager
    pub fn new(file_path: &str, buffer_size: usize) -> Result<Self, DownloadError> {
        let file_handle = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(file_path)
            .map_err(|e| DownloadError::IoError(e.to_string()))?;

        Ok(Self {
            buffer: vec![0; buffer_size],
            buffer_size,
            current_pos: 0,
            file_handle,
            total_written: 0,
            flush_count: 0,
        })
    }

    /// 向缓冲区写入数据
    pub fn write(&mut self, data: &[u8]) -> Result<(), DownloadError> {
        let mut bytes_written = 0;
        while bytes_written < data.len() {
            let space_left = self.buffer_size - self.current_pos;
            let to_copy = std::cmp::min(space_left, data.len() - bytes_written);

            if to_copy > 0 {
                self.buffer[self.current_pos..self.current_pos + to_copy]
                    .copy_from_slice(&data[bytes_written..bytes_written + to_copy]);
                self.current_pos += to_copy;
                bytes_written += to_copy;
            }

            if self.current_pos == self.buffer_size {
                self.flush()?;
            }
        }
        Ok(())
    }

    /// 将缓冲区内容刷入文件
    pub fn flush(&mut self) -> Result<(), DownloadError> {
        if self.current_pos > 0 {
            self.file_handle
                .write_all(&self.buffer[..self.current_pos])
                .map_err(|e| DownloadError::IoError(e.to_string()))?;
            self.total_written += self.current_pos as u64;
            self.current_pos = 0;
            self.flush_count += 1;
        }
        Ok(())
    }
    
    /// 获取缓冲区使用情况
    pub fn get_buffer_usage(&self) -> (usize, usize) {
        (self.current_pos, self.buffer_size)
    }

    /// 获取总写入字节数
    pub fn get_total_written(&self) -> u64 {
        self.total_written
    }

    /// 获取刷新次数
    pub fn get_flush_count(&self) -> u64 {
        self.flush_count
    }
    
    /// 缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.current_pos == 0
    }

    /// 缓冲区是否已满
    pub fn is_full(&self) -> bool {
        self.current_pos == self.buffer_size
    }

    /// 缓冲区可用空间
    pub fn available_space(&self) -> usize {
        self.buffer_size - self.current_pos
    }
}

/// 速度限制器
#[allow(dead_code)]
pub struct SpeedLimiter {
    pub max_speed: u64, // B/s
    pub window_size: Duration,
    pub tokens: u64,
    pub last_refill: Instant,
}

#[allow(dead_code)]
impl SpeedLimiter {
    pub fn new(max_speed: u64) -> Self {
        Self {
            max_speed,
            window_size: Duration::from_secs(1),
            tokens: max_speed,
            last_refill: Instant::now(),
        }
    }
    
    pub fn consume(&mut self, bytes: u64) -> bool {
        self.refill_tokens();
        if self.tokens >= bytes {
            self.tokens -= bytes;
            true
        } else {
            false
        }
    }

    fn refill_tokens(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        if elapsed >= self.window_size {
            self.tokens = self.max_speed;
            self.last_refill = now;
        }
    }

    pub fn wait_if_needed(&mut self, bytes: u64) -> Duration {
        while !self.consume(bytes) {
            let now = Instant::now();
            let elapsed = now.duration_since(self.last_refill);
            let time_to_wait = self.window_size.saturating_sub(elapsed);
            if !time_to_wait.is_zero() {
                return time_to_wait;
            }
            self.refill_tokens();
        }
        Duration::from_secs(0)
    }
} 