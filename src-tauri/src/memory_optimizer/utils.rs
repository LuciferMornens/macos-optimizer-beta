// src/memory_optimizer/utils.rs

use lazy_static::lazy_static;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MemoryPool {
    pools: Arc<Mutex<Vec<Vec<u8>>>>,
    chunk_size: usize,
}

impl MemoryPool {
    pub fn new(chunk_size: usize) -> Self {
        MemoryPool {
            pools: Arc::new(Mutex::new(Vec::new())),
            chunk_size,
        }
    }

    pub async fn acquire(&self) -> Vec<u8> {
        let mut pools = self.pools.lock().await;
        pools.pop().unwrap_or_else(|| vec![0; self.chunk_size])
    }

    pub async fn release(&self, mut chunk: Vec<u8>) {
        chunk.clear();
        chunk.resize(self.chunk_size, 0);

        let mut pools = self.pools.lock().await;
        if pools.len() < 10 {
            // Keep max 10 chunks
            pools.push(chunk);
        }
    }
}

// Global memory pool
lazy_static! {
    pub static ref MEMORY_POOL: MemoryPool = MemoryPool::new(20 * 1024 * 1024);
}

// Adaptive memory pressure calculation
pub fn calculate_adaptive_chunk_size(memory_pressure: f32) -> usize {
    let base_chunk_size = 50 * 1024 * 1024; // 50MB base

    match memory_pressure {
        p if p > 90.0 => base_chunk_size / 4, // 12.5MB when pressure high
        p if p > 75.0 => base_chunk_size / 2, // 25MB when moderate
        _ => base_chunk_size,                 // 50MB when low
    }
}

// Helper functions for memory stats parsing
pub fn get_page_size() -> u64 {
    // Get macOS page size (usually 4096)
    let output = Command::new("pagesize").output().unwrap_or_else(|_| {
        Command::new("getconf")
            .arg("PAGESIZE")
            .output()
            .expect("Failed to get page size")
    });

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(4096)
}

pub fn extract_number(line: &str) -> Option<u64> {
    // Extract number from vm_stat output line
    let parts: Vec<&str> = line.split_whitespace().collect();
    for part in parts.iter().rev() {
        if let Ok(num) = part.trim_end_matches('.').parse::<u64>() {
            return Some(num);
        }
    }
    None
}

pub fn extract_sysctl_value(key: &str) -> Option<u64> {
    let output = Command::new("sysctl").arg("-n").arg(key).output().ok()?;

    if output.status.success() {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .ok()
    } else {
        None
    }
}

pub fn extract_swap_used() -> u64 {
    let output = Command::new("sysctl").arg("vm.swapusage").output().ok();

    if let Some(output) = output {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Parse: vm.swapusage: total = X M  used = Y M  free = Z M
            if let Some(used_pos) = output_str.find("used = ") {
                let used_str = &output_str[used_pos + 7..];
                if let Some(m_pos) = used_str.find('M') {
                    if let Ok(used_mb) = used_str[..m_pos].trim().parse::<f64>() {
                        return (used_mb * 1024.0 * 1024.0) as u64;
                    }
                }
            }
        }
    }
    0
}
