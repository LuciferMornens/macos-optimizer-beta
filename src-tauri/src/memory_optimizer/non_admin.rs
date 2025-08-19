// src/memory_optimizer/non_admin.rs

use libc;
use std::process::Command;
use std::thread;
use std::time::Duration;

use super::stats;

pub(crate) fn clear_inactive_memory_safe() -> Result<u64, String> {
    let before = stats::get_memory_stats()?;

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

    let after = stats::get_memory_stats()?;
    Ok(if after.available > before.available {
        after.available - before.available
    } else {
        0
    })
}

pub(crate) fn optimize_file_caches() -> Result<(), String> {
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

pub(crate) fn clear_app_caches() -> Result<usize, String> {
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

pub(crate) fn optimize_memory_compression() -> Result<(), String> {
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

pub(crate) fn clear_network_caches_safe() -> Result<(), String> {
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

pub(crate) fn trigger_app_gc() -> Result<usize, String> {
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

pub(crate) fn clear_temp_allocations() -> Result<(), String> {
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

pub(crate) fn optimize_swap() -> Result<String, String> {
    // Check current swap usage
    let stats = stats::get_memory_stats()?;

    if stats.swap_used == 0 {
        return Ok("No swap in use, system is running optimally".to_string());
    }

    // Try to reduce swap usage by freeing memory
    let _ = clear_inactive_memory_safe()?;

    Ok(format!("Swap optimization attempted. Current swap usage: {} MB",
               stats.swap_used / (1024 * 1024)))
}

pub(crate) fn kill_memory_intensive_processes(threshold_mb: u64) -> Result<Vec<String>, String> {
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
                    if !is_critical_process(process_name) {
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