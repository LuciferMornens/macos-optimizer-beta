// src/config.rs

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    // Parallelism
    pub max_parallel_scans: usize,   // Default: num_cpus::get()
    pub max_parallel_deletes: usize, // Default: 4

    // Caching
    pub dir_cache_size: usize,      // Default: 1000 entries
    pub dir_cache_ttl: u64,         // Default: 300 seconds
    pub metadata_cache_size: usize, // Default: 5000 entries

    // Memory optimization
    pub adaptive_memory: bool,   // Default: true
    pub max_memory_chunk: usize, // Default: 50MB
    pub memory_pool_size: usize, // Default: 10 chunks

    // Background tasks
    pub enable_background_refresh: bool, // Default: true
    pub refresh_interval: u64,           // Default: 60 seconds
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        PerformanceConfig {
            max_parallel_scans: num_cpus::get(),
            max_parallel_deletes: 4,
            dir_cache_size: 1000,
            dir_cache_ttl: 300,
            metadata_cache_size: 5000,
            adaptive_memory: true,
            max_memory_chunk: 50 * 1024 * 1024,
            memory_pool_size: 10,
            enable_background_refresh: true,
            refresh_interval: 60,
        }
    }
}

// Global configuration
lazy_static! {
    pub static ref PERFORMANCE_CONFIG: PerformanceConfig = PerformanceConfig::default();
}

// Operation metrics tracking
// Removed lightweight timing helpers to keep the crate warning-free.
