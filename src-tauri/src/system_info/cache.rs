// src/system_info/cache.rs

use super::{ProcessInfo, MemoryInfo, CpuInfo, DiskInfo};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};
use sysinfo::{System, ProcessRefreshKind};

pub struct ProcessCache {
    processes: Arc<RwLock<Vec<ProcessInfo>>>,
    last_update: Arc<RwLock<Instant>>,
    update_interval: Duration,
}

impl ProcessCache {
    pub fn new(update_interval_secs: u64) -> Self {
        ProcessCache {
            processes: Arc::new(RwLock::new(Vec::new())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            update_interval: Duration::from_secs(update_interval_secs),
        }
    }
    
    pub async fn get_top_memory_processes(&self, limit: usize) -> Vec<ProcessInfo> {
        let last_update = *self.last_update.read().await;
        
        // Return cached if recent
        if last_update.elapsed() < self.update_interval {
            let processes = self.processes.read().await;
            return processes.iter()
                .take(limit)
                .cloned()
                .collect();
        }
        
        // Update cache
        self.update_cache().await;
        
        let processes = self.processes.read().await;
        processes.iter()
            .take(limit)
            .cloned()
            .collect()
    }
    
    async fn update_cache(&self) {
        let mut system = System::new();
        system.refresh_processes_specifics(ProcessRefreshKind::everything());
        
        let mut process_list: Vec<ProcessInfo> = system.processes()
            .iter()
            .map(|(pid, process)| ProcessInfo {
                pid: pid.as_u32(),
                name: process.name().to_string(),
                memory_usage: process.memory(),
                cpu_usage: process.cpu_usage(),
                virtual_memory: process.virtual_memory(),
                status: format!("{:?}", process.status()),
                parent_pid: process.parent().map(|p| p.as_u32()),
            })
            .collect();
        
        // Sort by memory usage
        process_list.sort_by(|a, b| b.memory_usage.cmp(&a.memory_usage));
        
        // Update cache
        let mut processes = self.processes.write().await;
        *processes = process_list;
        
        let mut last_update = self.last_update.write().await;
        *last_update = Instant::now();
    }
}

// Global process cache
lazy_static::lazy_static! {
    pub static ref PROCESS_CACHE: ProcessCache = ProcessCache::new(2);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardData {
    pub memory: MemoryInfo,
    pub cpu: CpuInfo,
    pub disks: Vec<DiskInfo>,
    pub top_processes: Vec<ProcessInfo>,
}

use serde::{Serialize, Deserialize};