// src/memory_optimizer.rs

use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;

// Internal modules backing this facade.
// Keep this file as the stable entry point that others import.
mod stats;
mod non_admin;
mod admin;
mod utils;

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

    // Public API unchanged: orchestrates non-admin steps via non_admin module.
    pub fn optimize_memory(&self) -> Result<MemoryOptimizationResult, String> {
        let memory_before = Self::get_memory_stats()?;

        let mut success = true;
        let mut message = String::new();
        let mut optimizations_performed = Vec::new();

        // 1. Clear inactive memory pages (non-sudo)
        if let Ok(freed) = non_admin::clear_inactive_memory_safe() {
            if freed > 0 {
                optimizations_performed.push(format!("Cleared {} MB of inactive memory", freed / (1024 * 1024)));
                message.push_str(&format!("Freed {} MB from inactive memory\n", freed / (1024 * 1024)));
            }
        }

        // 2. Optimize file system caches
        if let Ok(_) = non_admin::optimize_file_caches() {
            optimizations_performed.push("Optimized file system caches".to_string());
            message.push_str("Optimized file system caches\n");
        }

        // 3. Clear application caches (non-sudo)
        if let Ok(cleared) = non_admin::clear_app_caches() {
            optimizations_performed.push(format!("Cleared {} application caches", cleared));
            message.push_str(&format!("Cleared {} application caches\n", cleared));
        }

        // 4. Optimize memory compression
        if let Ok(_) = non_admin::optimize_memory_compression() {
            optimizations_performed.push("Optimized memory compression".to_string());
            message.push_str("Optimized memory compression\n");
        }

        // 5. Clear DNS and network caches (non-sudo versions)
        if let Ok(_) = non_admin::clear_network_caches_safe() {
            optimizations_performed.push("Cleared network caches".to_string());
            message.push_str("Cleared network caches\n");
        }

        // 6. Trigger garbage collection in running apps
        if let Ok(apps) = non_admin::trigger_app_gc() {
            optimizations_performed.push(format!("Triggered GC in {} apps", apps));
            message.push_str(&format!("Triggered garbage collection in {} apps\n", apps));
        }

        // 7. Clear temporary memory allocations
        if let Ok(_) = non_admin::clear_temp_allocations() {
            optimizations_performed.push("Cleared temporary allocations".to_string());
            message.push_str("Cleared temporary memory allocations\n");
        }

        // Wait for optimizations to take effect
        thread::sleep(Duration::from_secs(2));

        let memory_after = Self::get_memory_stats()?;
        let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);

        if optimizations_performed.is_empty() {
            success = false;
            message = "No optimizations could be performed without admin access".to_string();
        }

        Ok(MemoryOptimizationResult {
            memory_before,
            memory_after,
            freed_memory: freed_memory.abs(),
            optimization_type: "Comprehensive Safe Mode".to_string(),
            success,
            message: message.trim().to_string(),
            optimizations_performed,
        })
    }

    // Public API unchanged: delegates privileged work to admin module and keeps the same flow.
    pub fn optimize_memory_with_admin(&self, _use_gui_auth: bool) -> Result<MemoryOptimizationResult, String> {
        let memory_before = Self::get_memory_stats()?;

        let mut success = true;
        let mut message = String::new();
        let mut optimizations_performed = Vec::new();

        // Run the deep-clean privileged script in one go.
        let outcome = admin::run_deep_clean();

        if outcome.success {
            // Map markers to human-friendly labels
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
            for (marker, label) in mapping {
                if stdout.contains(marker) {
                    optimizations_performed.push(label.to_string());
                }
            }
            message.push_str("Deep clean script executed\n");
        } else if outcome.cancelled {
            message.push_str("User cancelled admin authentication\n");
            success = false;

            // Fallback to non-admin optimization
            if let Ok(regular_result) = self.optimize_memory() {
                optimizations_performed.extend(regular_result.optimizations_performed);
                message.push_str(&format!("\nPerformed standard optimizations instead:\n{}\n", regular_result.message));
            }

            thread::sleep(Duration::from_secs(3));
            let memory_after = Self::get_memory_stats()?;
            let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);

            return Ok(MemoryOptimizationResult {
                memory_before,
                memory_after,
                freed_memory: freed_memory.abs(),
                optimization_type: "Standard Optimization (Admin Cancelled)".to_string(),
                success,
                message: message.trim().to_string(),
                optimizations_performed,
            });
        } else {
            // Script run failed
            message.push_str(&format!("Deep clean script failed: {} {}\n", outcome.stderr, outcome.stdout));
            success = false;
        }

        // Also perform non-admin optimizations
        if let Ok(regular_result) = self.optimize_memory() {
            optimizations_performed.extend(regular_result.optimizations_performed);
            message.push_str(&format!("\nAlso performed standard optimizations:\n{}\n", regular_result.message));
        }

        // Wait for changes to settle
        thread::sleep(Duration::from_secs(5));

        let memory_after = Self::get_memory_stats()?;
        let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);

        if optimizations_performed.is_empty() {
            success = false;
            message = "No optimizations could be performed. Admin access may have been denied.".to_string();
        }

        Ok(MemoryOptimizationResult {
            memory_before,
            memory_after,
            freed_memory: freed_memory.abs(),
            optimization_type: "Deep Clean with Admin Privileges".to_string(),
            success,
            message: message.trim().to_string(),
            optimizations_performed,
        })
    }

    // Keep thin wrappers to preserve the public API exactly.
    pub fn clear_inactive_memory(&self) -> Result<u64, String> {
        non_admin::clear_inactive_memory_safe()
    }

    pub fn get_memory_pressure(&self) -> Result<f32, String> {
        let stats = Self::get_memory_stats()?;
        if stats.total > 0 {
            Ok((stats.used as f32 / stats.total as f32) * 100.0)
        } else {
            Err("Unable to calculate memory pressure".to_string())
        }
    }

    pub fn optimize_swap(&self) -> Result<String, String> {
        non_admin::optimize_swap()
    }

    pub fn kill_memory_intensive_processes(&self, threshold_mb: u64) -> Result<Vec<String>, String> {
        non_admin::kill_memory_intensive_processes(threshold_mb)
    }
}
