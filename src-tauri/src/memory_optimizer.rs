// src/memory_optimizer.rs

use serde::{Deserialize, Serialize};
// time helpers imported ad-hoc where needed

#[derive(Debug, Serialize, Clone)]
pub struct ProgressUpdate {
    pub operation: String,
    pub stage: String,
    pub progress: f32,
    pub message: String,
}

// Internal modules backing this facade.
// Keep this file as the stable entry point that others import.
mod stats;
mod non_admin;
mod admin;
mod utils;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryOptimizationResult {
    pub memory_before: MemoryStats,
    pub memory_after: MemoryStats,
    pub freed_memory: i64,
    pub optimization_type: String,
    pub success: bool,
    pub message: String,
    pub optimizations_performed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryStats {
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub wired: u64,
    pub compressed: u64,
    pub swap_used: u64,
    pub app_memory: u64,
    pub cache_files: u64,
}

pub struct MemoryOptimizer;

impl MemoryOptimizer {
    pub fn new() -> Self {
        MemoryOptimizer
    }

    // Public API unchanged: delegate to stats module.
    pub fn get_memory_stats() -> Result<MemoryStats, String> {
        stats::get_memory_stats()
    }


    pub async fn optimize_memory_with_cancel(&self, cancel: &CancellationToken) -> Result<MemoryOptimizationResult, String> {
        let memory_before = Self::get_memory_stats()?;
        let (inactive_result, cache_result, app_cache_result, compression_result, network_result, gc_result, temp_result) = tokio::join!(
            non_admin::clear_inactive_memory_adaptive_with_cancel(Some(cancel)),
            non_admin::optimize_file_caches(),
            non_admin::clear_app_caches(),
            non_admin::optimize_memory_compression(),
            non_admin::clear_network_caches_safe(),
            non_admin::trigger_app_gc(),
            non_admin::clear_temp_allocations()
        );
        if cancel.is_cancelled() { return Err("cancelled".into()); }
        let mut optimizations_performed = Vec::new();
        let mut message = String::new();
        let mut success = true;
        if let Ok(freed) = inactive_result { if freed > 0 { optimizations_performed.push(format!("Cleared {} MB of inactive memory", freed / (1024 * 1024))); message.push_str(&format!("Freed {} MB from inactive memory\n", freed / (1024 * 1024))); } }
        if cache_result.is_ok() { optimizations_performed.push("Optimized file system caches".to_string()); message.push_str("Optimized file system caches\n"); }
        if let Ok(cleared) = app_cache_result { optimizations_performed.push(format!("Cleared {} application caches", cleared)); message.push_str(&format!("Cleared {} application caches\n", cleared)); }
        if compression_result.is_ok() { optimizations_performed.push("Optimized memory compression".to_string()); message.push_str("Optimized memory compression\n"); }
        if network_result.is_ok() { optimizations_performed.push("Cleared network caches".to_string()); message.push_str("Cleared network caches\n"); }
        if let Ok(apps) = gc_result { optimizations_performed.push(format!("Triggered GC in {} apps", apps)); message.push_str(&format!("Triggered garbage collection in {} apps\n", apps)); }
        if temp_result.is_ok() { optimizations_performed.push("Cleared temporary allocations".to_string()); message.push_str("Cleared temporary memory allocations\n"); }
        if cancel.is_cancelled() { return Err("cancelled".into()); }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let memory_after = Self::get_memory_stats()?;
        let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);
        if optimizations_performed.is_empty() { success = false; message = "No optimizations could be performed without admin access".to_string(); }
        Ok(MemoryOptimizationResult { memory_before, memory_after, freed_memory: freed_memory.abs(), optimization_type: "Parallel Optimization Mode".to_string(), success, message: message.trim().to_string(), optimizations_performed })
    }


    pub async fn optimize_memory_with_admin_cancel(&self, cancel: &CancellationToken) -> Result<MemoryOptimizationResult, String> {
        let memory_before = Self::get_memory_stats()?;
        let mut success = true;
        let mut message = String::new();
        let mut optimizations_performed = Vec::new();

        let outcome = admin::run_deep_clean_with_cancel(cancel).await;
        if outcome.success {
            let stdout = outcome.stdout;
            let mapping = vec![
                ("OK:PURGE", "Purged memory and disk cache (admin)"),
                ("OK:DNS", "Flushed DNS cache (admin)"),
                ("OK:MDNS", "Signaled mDNSResponder (admin)"),
                ("OK:CLEAR_SYS_CACHE", "Cleared system caches (admin)"),
                ("OK:CLEAR_SWAP", "Cleared swap files (admin)"),
                ("OK:LSREGISTER", "Reset Launch Services database (admin)"),
                ("OK:ATSUTIL", "Cleared font caches (admin)"),
                ("OK:KEXT_TOUCH", "Touched extensions directory (admin)"),
                ("OK:KEXTCACHE", "Rebuilt kernel extension cache (admin)"),
                ("OK:PERIODIC", "Ran maintenance scripts (admin)"),
                ("OK:RESTART_Dock", "Restarted Dock"),
                ("OK:RESTART_Finder", "Restarted Finder"),
                ("OK:RESTART_SysUIS", "Restarted SystemUIServer"),
                ("OK:RESTART_cfprefsd", "Restarted cfprefsd"),
            ];
            for (marker, label) in mapping { if stdout.contains(marker) { optimizations_performed.push(label.to_string()); } }
            message.push_str("Deep clean script executed\n");
        } else if outcome.cancelled {
            message.push_str("User canceled admin authentication or operation\n");
            success = false;
            if !cancel.is_cancelled() {
                if let Ok(regular_result) = self.optimize_memory_with_cancel(cancel).await {
                    optimizations_performed.extend(regular_result.optimizations_performed);
                    message.push_str(&format!("\nPerformed standard optimizations instead:\n{}\n", regular_result.message));
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let memory_after = Self::get_memory_stats()?;
            let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);
            return Ok(MemoryOptimizationResult { memory_before, memory_after, freed_memory: freed_memory.abs(), optimization_type: "Standard Optimization (Admin Canceled)".to_string(), success, message: message.trim().to_string(), optimizations_performed });
        } else {
            message.push_str(&format!("Deep clean script failed: {} {}\n", outcome.stderr, outcome.stdout));
            success = false;
        }

        if cancel.is_cancelled() { return Err("cancelled".into()); }
        if let Ok(regular_result) = self.optimize_memory_with_cancel(cancel).await {
            optimizations_performed.extend(regular_result.optimizations_performed);
            message.push_str(&format!("\nAlso performed standard optimizations:\n{}\n", regular_result.message));
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let memory_after = Self::get_memory_stats()?;
        let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);
        if optimizations_performed.is_empty() { success = false; message = "No optimizations could be performed. Admin access may have been denied.".to_string(); }
        Ok(MemoryOptimizationResult { memory_before, memory_after, freed_memory: freed_memory.abs(), optimization_type: "Deep Clean with Admin Privileges".to_string(), success, message: message.trim().to_string(), optimizations_performed })
    }

    // Keep thin wrappers to preserve the public API exactly.
    pub async fn clear_inactive_memory(&self) -> Result<u64, String> {
        non_admin::clear_inactive_memory_safe().await
    }

    pub fn get_memory_pressure(&self) -> Result<f32, String> {
        let stats = Self::get_memory_stats()?;
        if stats.total > 0 {
            Ok((stats.used as f32 / stats.total as f32) * 100.0)
        } else {
            Err("Unable to calculate memory pressure".to_string())
        }
    }

    pub async fn optimize_swap(&self) -> Result<String, String> {
        non_admin::optimize_swap().await
    }

    pub async fn kill_memory_intensive_processes(&self, threshold_mb: u64) -> Result<Vec<String>, String> {
        non_admin::kill_memory_intensive_processes(threshold_mb).await
    }
}
