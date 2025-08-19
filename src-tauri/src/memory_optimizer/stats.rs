// src/memory_optimizer/stats.rs

use std::process::Command;

use super::{MemoryStats};
use super::utils::{get_page_size, extract_number, extract_sysctl_value, extract_swap_used};

pub(crate) fn get_memory_stats() -> Result<MemoryStats, String> {
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
    let page_size = get_page_size();

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
            pages_free = extract_number(line).unwrap_or(0);
        } else if line.contains("Pages active:") {
            pages_active = extract_number(line).unwrap_or(0);
        } else if line.contains("Pages inactive:") {
            pages_inactive = extract_number(line).unwrap_or(0);
        } else if line.contains("Pages speculative:") {
            pages_speculative = extract_number(line).unwrap_or(0);
        } else if line.contains("Pages wired down:") {
            pages_wired = extract_number(line).unwrap_or(0);
        } else if line.contains("Pages occupied by compressor:") {
            pages_compressed = extract_number(line).unwrap_or(0);
        } else if line.contains("Pages purgeable:") {
            pages_purgeable = extract_number(line).unwrap_or(0);
        } else if line.contains("File-backed pages:") {
            file_backed = extract_number(line).unwrap_or(0);
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
        if let Some(total) = extract_sysctl_value(&total_str) {
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
        stats.swap_used = extract_swap_used(&swap_str);
    }

    Ok(stats)
}
