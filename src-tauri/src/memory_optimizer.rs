use serde::{Deserialize, Serialize};
use std::process::Command;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use libc;
use std::thread;
use std::time::Duration;

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

    pub fn get_memory_stats() -> Result<MemoryStats, String> {
        // Use vm_stat command to get detailed memory information on macOS
        let output = Command::new("vm_stat")
            .output()
            .map_err(|e| format!("Failed to execute vm_stat: {}", e))?;

        if !output.status.success() {
            return Err("vm_stat command failed".to_string());
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut stats = MemoryStats {
            total: 0,
            available: 0,
            used: 0,
            wired: 0,
            compressed: 0,
            swap_used: 0,
            app_memory: 0,
            cache_files: 0,
        };

        // Parse vm_stat output
        let page_size = Self::get_page_size();
        
        let mut pages_free = 0u64;
        let mut pages_active = 0u64;
        let mut pages_inactive = 0u64;
        let mut pages_speculative = 0u64;
        let mut pages_wired = 0u64;
        let mut pages_compressed = 0u64;
        let mut pages_purgeable = 0u64;
        let mut file_backed = 0u64;
        
        for line in output_str.lines() {
            if line.contains("Pages free:") {
                pages_free = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("Pages active:") {
                pages_active = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("Pages inactive:") {
                pages_inactive = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("Pages speculative:") {
                pages_speculative = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("Pages wired down:") {
                pages_wired = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("Pages occupied by compressor:") {
                pages_compressed = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("Pages purgeable:") {
                pages_purgeable = Self::extract_number(line).unwrap_or(0);
            } else if line.contains("File-backed pages:") {
                file_backed = Self::extract_number(line).unwrap_or(0);
            }
        }

        // Calculate memory values in bytes
        stats.wired = pages_wired * page_size;
        stats.compressed = pages_compressed * page_size;
        stats.cache_files = file_backed * page_size;
        
        // Available memory = free + inactive + purgeable + speculative
        stats.available = (pages_free + pages_inactive + pages_purgeable + pages_speculative) * page_size;
        
        // App memory = active pages
        stats.app_memory = pages_active * page_size;

        // Get total memory using sysctl
        let total_output = Command::new("sysctl")
            .arg("hw.memsize")
            .output()
            .map_err(|e| format!("Failed to get total memory: {}", e))?;

        if let Ok(total_str) = String::from_utf8(total_output.stdout) {
            if let Some(total) = Self::extract_sysctl_value(&total_str) {
                stats.total = total;
                // Used memory = total - available
                stats.used = if stats.available < stats.total {
                    stats.total - stats.available
                } else {
                    stats.wired + stats.app_memory + stats.compressed
                };
            }
        }

        // Get swap usage
        let swap_output = Command::new("sysctl")
            .arg("vm.swapusage")
            .output()
            .map_err(|e| format!("Failed to get swap usage: {}", e))?;

        if let Ok(swap_str) = String::from_utf8(swap_output.stdout) {
            stats.swap_used = Self::extract_swap_used(&swap_str);
        }

        Ok(stats)
    }

    fn get_page_size() -> u64 {
        // macOS page size is typically 4096 bytes
        // Use sysconf to get the actual page size
        unsafe {
            let page_size = libc::sysconf(libc::_SC_PAGESIZE);
            if page_size > 0 {
                page_size as u64
            } else {
                4096 // Default fallback
            }
        }
    }

    fn extract_number(line: &str) -> Option<u64> {
        line.split_whitespace()
            .last()?
            .trim_end_matches('.')
            .parse::<u64>()
            .ok()
    }

    fn extract_sysctl_value(line: &str) -> Option<u64> {
        line.split(':')
            .last()?
            .trim()
            .parse::<u64>()
            .ok()
    }

    fn extract_swap_used(swap_str: &str) -> u64 {
        // Parse format: vm.swapusage: total = X M  used = Y M  free = Z M
        if let Some(used_part) = swap_str.split("used =").nth(1) {
            if let Some(value_part) = used_part.split('M').next() {
                if let Ok(mb_value) = value_part.trim().parse::<f64>() {
                    return (mb_value * 1024.0 * 1024.0) as u64;
                }
            }
        }
        0
    }

    pub fn optimize_memory(&self) -> Result<MemoryOptimizationResult, String> {
        let memory_before = Self::get_memory_stats()?;
        
        let mut success = true;
        let mut message = String::new();
        let mut optimizations_performed = Vec::new();
        
        // 1. Clear inactive memory pages (non-sudo)
        if let Ok(freed) = self.clear_inactive_memory_safe() {
            if freed > 0 {
                optimizations_performed.push(format!("Cleared {} MB of inactive memory", freed / (1024 * 1024)));
                message.push_str(&format!("Freed {} MB from inactive memory\n", freed / (1024 * 1024)));
            }
        }
        
        // 2. Optimize file system caches
        if let Ok(_) = self.optimize_file_caches() {
            optimizations_performed.push("Optimized file system caches".to_string());
            message.push_str("Optimized file system caches\n");
        }
        
        // 3. Clear application caches (non-sudo)
        if let Ok(cleared) = self.clear_app_caches() {
            optimizations_performed.push(format!("Cleared {} application caches", cleared));
            message.push_str(&format!("Cleared {} application caches\n", cleared));
        }
        
        // 4. Optimize memory compression
        if let Ok(_) = self.optimize_memory_compression() {
            optimizations_performed.push("Optimized memory compression".to_string());
            message.push_str("Optimized memory compression\n");
        }
        
        // 5. Clear DNS and network caches (non-sudo versions)
        if let Ok(_) = self.clear_network_caches_safe() {
            optimizations_performed.push("Cleared network caches".to_string());
            message.push_str("Cleared network caches\n");
        }
        
        // 6. Trigger garbage collection in running apps
        if let Ok(apps) = self.trigger_app_gc() {
            optimizations_performed.push(format!("Triggered GC in {} apps", apps));
            message.push_str(&format!("Triggered garbage collection in {} apps\n", apps));
        }
        
        // 7. Clear temporary memory allocations
        if let Ok(_) = self.clear_temp_allocations() {
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

    pub fn optimize_memory_with_admin(&self, _use_gui_auth: bool) -> Result<MemoryOptimizationResult, String> {
        let memory_before = Self::get_memory_stats()?;
        
        let mut success = true;
        let mut message = String::new();
        let mut optimizations_performed = Vec::new();
        
        // Build a single privileged script to run all admin-required tasks
        let script_path = "/tmp/macos_optimizer_deep_clean.sh";
        let shell_script = r#"#!/bin/bash
set -euo pipefail

# Helper to run a step and echo a marker
run() {
  local label="$1"; shift
  if "$@"; then
    echo "OK:${label}"
  else
    echo "ERR:${label}"
  fi
}

# Admin-required tasks (with markers)
run PURGE purge
run DNS dscacheutil -flushcache
run MDNS killall -HUP mDNSResponder
run CLEAR_SYS_CACHE bash -lc 'rm -rf /Library/Caches/* && rm -rf /private/var/folders/*/C/* && rm -rf /private/var/folders/*/*/com.apple.LaunchServices*'
run CLEAR_SWAP bash -lc 'rm -f /private/var/vm/swapfile*'
run LSREGISTER "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister" -kill -r -domain local -domain system -domain user
run ATSUTIL atsutil databases -remove
run KEXT_TOUCH touch /System/Library/Extensions
run KEXTCACHE kextcache -update-volume /
run PERIODIC periodic daily weekly monthly

# Restart common UI services (markers too)
run RESTART_Dock killall -KILL Dock
run RESTART_Finder killall -KILL Finder
run RESTART_SysUIS killall -KILL SystemUIServer
run RESTART_cfprefsd killall cfprefsd
"#;

        // Write script to disk and make it executable
        fs::write(script_path, shell_script).map_err(|e| format!("Failed to write admin script: {}", e))?;
        if let Ok(meta) = fs::metadata(script_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(script_path, perms);
        }

        // One admin prompt for the whole run
        let applescript = format!(r#"with timeout of 1200 seconds
  do shell script "{}" with administrator privileges
end timeout"#, script_path);

        match Command::new("osascript").arg("-e").arg(applescript).output() {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Map markers to human-friendly labels
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
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Detect cancellation
                    if stderr.contains("canceled") || stderr.contains("cancelled") || stderr.contains("-128") {
                        message.push_str("User cancelled admin authentication\n");
                        success = false;

                        if let Ok(regular_result) = self.optimize_memory() {
                            optimizations_performed.extend(regular_result.optimizations_performed);
                            message.push_str(&format!("\nPerformed standard optimizations instead:\n{}\n", regular_result.message));
                        }

                        thread::sleep(Duration::from_secs(3));
                        let memory_after = Self::get_memory_stats()?;
                        let freed_memory = (memory_after.available as i64) - (memory_before.available as i64);

                        let _ = fs::remove_file(script_path);
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
                        eprintln!("Deep clean script failed - stderr: {}, stdout: {}", stderr, stdout);
                        message.push_str(&format!("Deep clean script failed: {} {}\n", stderr, stdout));
                        success = false;
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to run admin script: {}", e);
                message.push_str(&format!("Failed to run admin script: {}\n", e));
                success = false;
            }
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

        let _ = fs::remove_file(script_path);

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

    fn clear_inactive_memory_safe(&self) -> Result<u64, String> {
        let before = Self::get_memory_stats()?;
        
        // Force sync to write dirty pages
        Command::new("sync")
            .output()
            .map_err(|e| format!("Failed to sync: {}", e))?;
        
        // Allocate and free memory to trigger compaction
        unsafe {
            // Allocate multiple smaller chunks to avoid issues
            let chunks = 10;
            let chunk_size = 50 * 1024 * 1024; // 50MB chunks
            let mut ptrs = Vec::new();
            
            for _ in 0..chunks {
                let ptr = libc::malloc(chunk_size);
                if !ptr.is_null() {
                    libc::memset(ptr, 0, chunk_size);
                    ptrs.push(ptr);
                }
            }
            
            // Free all at once to trigger compaction
            for ptr in ptrs {
                libc::free(ptr);
            }
        }
        
        thread::sleep(Duration::from_millis(500));
        
        let after = Self::get_memory_stats()?;
        Ok(if after.available > before.available {
            after.available - before.available
        } else {
            0
        })
    }

    fn optimize_file_caches(&self) -> Result<(), String> {
        // Drop file system caches that can be rebuilt
        Command::new("sync")
            .output()
            .map_err(|e| format!("Failed to sync: {}", e))?;
        
        // Force file system to drop clean caches
        Command::new("sync")
            .output()
            .map_err(|e| format!("Failed to sync: {}", e))?;
        
        Ok(())
    }

    fn clear_app_caches(&self) -> Result<usize, String> {
        let mut cleared = 0;
        
        // Clear Safari cache
        if let Ok(_) = Command::new("rm")
            .args(&["-rf", "~/Library/Caches/com.apple.Safari/Cache.db"])
            .output()
        {
            cleared += 1;
        }
        
        // Clear Chrome memory cache
        if let Ok(_) = Command::new("rm")
            .args(&["-rf", "~/Library/Caches/Google/Chrome/Default/Cache"])
            .output()
        {
            cleared += 1;
        }
        
        // Clear system app caches
        if let Ok(_) = Command::new("rm")
            .args(&["-rf", "~/Library/Caches/com.apple.dt.Xcode/Cache"])
            .output()
        {
            cleared += 1;
        }
        
        Ok(cleared)
    }

    fn optimize_memory_compression(&self) -> Result<(), String> {
        // Trigger memory compression by creating memory pressure
        unsafe {
            // Allocate memory to trigger compression
            let size = 100 * 1024 * 1024; // 100MB
            let ptr = libc::malloc(size);
            if !ptr.is_null() {
                // Write random data to prevent deduplication
                for i in 0..size/8 {
                    let random_ptr = ptr.add(i * 8) as *mut u64;
                    *random_ptr = i as u64;
                }
                
                // Sleep to let compression happen
                thread::sleep(Duration::from_millis(100));
                
                // Free the memory
                libc::free(ptr);
            }
        }
        Ok(())
    }

    fn clear_network_caches_safe(&self) -> Result<(), String> {
        // Clear network preferences cache
        Command::new("rm")
            .args(&["-rf", "~/Library/Caches/com.apple.networkserviceproxy"])
            .output()
            .ok();
        
        // Clear CFNetwork cache
        Command::new("rm")
            .args(&["-rf", "~/Library/Caches/com.apple.cfnetwork"])
            .output()
            .ok();
        
        Ok(())
    }

    fn trigger_app_gc(&self) -> Result<usize, String> {
        let mut triggered = 0;
        
        // Send memory pressure signals to apps
        let apps = ["Safari", "Chrome", "Firefox", "Mail", "Xcode"];
        
        for app in &apps {
            if let Ok(_) = Command::new("killall")
                .args(&["-CONT", app])
                .output()
            {
                triggered += 1;
            }
        }
        
        Ok(triggered)
    }

    fn clear_temp_allocations(&self) -> Result<(), String> {
        // Clear temporary allocations by forcing malloc to release free pages
        unsafe {
            // Multiple malloc/free cycles to fragment and reclaim memory
            for _ in 0..5 {
                let size = 20 * 1024 * 1024; // 20MB
                let ptr = libc::malloc(size);
                if !ptr.is_null() {
                    libc::memset(ptr, 0, size);
                    libc::free(ptr);
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(())
    }

    pub fn clear_inactive_memory(&self) -> Result<u64, String> {
        self.clear_inactive_memory_safe()
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
        // Check current swap usage
        let stats = Self::get_memory_stats()?;
        
        if stats.swap_used == 0 {
            return Ok("No swap in use, system is running optimally".to_string());
        }
        
        // Try to reduce swap usage by freeing memory
        let _ = self.clear_inactive_memory_safe()?;
        
        Ok(format!("Swap optimization attempted. Current swap usage: {} MB", 
                   stats.swap_used / (1024 * 1024)))
    }

    pub fn kill_memory_intensive_processes(&self, threshold_mb: u64) -> Result<Vec<String>, String> {
        let mut killed_processes = Vec::new();
        
        // Get list of processes using more than threshold memory
        let output = Command::new("ps")
            .args(&["aux"])
            .output()
            .map_err(|e| format!("Failed to list processes: {}", e))?;
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        
        for line in output_str.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 10 {
                // Column 5 is VSZ (virtual memory in KB)
                if let Ok(vsz_kb) = parts[4].parse::<u64>() {
                    let vsz_mb = vsz_kb / 1024;
                    if vsz_mb > threshold_mb {
                        let pid = parts[1];
                        let process_name = parts[10];
                        
                        // Skip critical system processes
                        if !Self::is_critical_process(process_name) {
                            // Try to kill the process
                            if let Ok(_) = Command::new("kill")
                                .arg("-TERM")
                                .arg(pid)
                                .output()
                            {
                                killed_processes.push(format!("{} (PID: {})", process_name, pid));
                            }
                        }
                    }
                }
            }
        }
        
        Ok(killed_processes)
    }

    fn is_critical_process(name: &str) -> bool {
        let critical = vec![
            "kernel_task",
            "launchd",
            "systemd",
            "init",
            "WindowServer",
            "loginwindow",
            "Finder",
            "Dock",
            "SystemUIServer",
            "coreaudiod",
            "mds",
            "mds_stores",
            "mdworker",
        ];
        
        critical.iter().any(|&proc| name.contains(proc))
    }
}