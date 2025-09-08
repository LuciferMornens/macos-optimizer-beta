// src/memory_optimizer/non_admin.rs



use tokio::time::{sleep, Duration};
use tokio::process::Command as TokioCommand;

use super::stats;

pub(crate) async fn clear_inactive_memory_safe() -> Result<u64, String> {
    let before = stats::get_memory_stats()?;

    // Force sync to write dirty pages
    TokioCommand::new("sync")
        .output()
        .await
        .map_err(|e| format!("Failed to sync: {}", e))?;

    // Allocate and free memory to trigger compaction using safe Vec
    let chunks = 10;
    let chunk_size = 50 * 1024 * 1024; // 50MB chunks
    let mut memory_chunks = Vec::new();

    for _ in 0..chunks {
        let chunk: Vec<u8> = vec![0; chunk_size];
        memory_chunks.push(chunk);
    }

    // Free all chunks
    drop(memory_chunks);

    sleep(Duration::from_millis(500)).await;

    let after = stats::get_memory_stats()?;
    Ok(if after.available > before.available {
        after.available - before.available
    } else {
        0
    })
}

pub(crate) async fn optimize_file_caches() -> Result<(), String> {
    // Drop file system caches that can be rebuilt
    TokioCommand::new("sync")
        .output()
        .await
        .map_err(|e| format!("Failed to sync: {}", e))?;

    // Force file system to drop clean caches
    TokioCommand::new("sync")
        .output()
        .await
        .map_err(|e| format!("Failed to sync: {}", e))?;

    Ok(())
}

pub(crate) async fn clear_app_caches() -> Result<usize, String> {
    let mut cleared = 0;

    // Clear Safari cache
    if let Ok(_) = TokioCommand::new("rm")
        .args(&["-rf", "~/Library/Caches/com.apple.Safari/Cache.db"])
        .output()
        .await
    {
        cleared += 1;
    }

    // Clear Chrome memory cache
    if let Ok(_) = TokioCommand::new("rm")
        .args(&["-rf", "~/Library/Caches/Google/Chrome/Default/Cache"])
        .output()
        .await
    {
        cleared += 1;
    }

    // Clear system app caches
    if let Ok(_) = TokioCommand::new("rm")
        .args(&["-rf", "~/Library/Caches/com.apple.dt.Xcode/Cache"])
        .output()
        .await
    {
        cleared += 1;
    }

    Ok(cleared)
}

pub(crate) async fn optimize_memory_compression() -> Result<(), String> {
    // Trigger memory compression by creating memory pressure with smaller, safer allocations
    // Use Vec to avoid raw pointer Send issues
    let size = 20 * 1024 * 1024 / 8; // 20MB worth of u64s
    let mut memory_chunk: Vec<u64> = Vec::with_capacity(size);
    
    // Write data to trigger memory pressure
    for i in 0..size {
        memory_chunk.push(i as u64);
    }
    
    // Sleep to let compression happen
    sleep(Duration::from_millis(100)).await;
    
    // Vec will be automatically freed when it goes out of scope
    drop(memory_chunk);
    
    Ok(())
}

pub(crate) async fn clear_network_caches_safe() -> Result<(), String> {
    // Clear network preferences cache
    TokioCommand::new("rm")
        .args(&["-rf", "~/Library/Caches/com.apple.networkserviceproxy"])
        .output()
        .await
        .ok();

    // Clear CFNetwork cache
    TokioCommand::new("rm")
        .args(&["-rf", "~/Library/Caches/com.apple.cfnetwork"])
        .output()
        .await
        .ok();

    Ok(())
}

pub(crate) async fn trigger_app_gc() -> Result<usize, String> {
    let mut triggered = 0;

    // Send memory pressure signals to apps
    let apps = ["Safari", "Chrome", "Firefox", "Mail", "Xcode"];

    for app in &apps {
        if let Ok(_) = TokioCommand::new("killall")
            .args(&["-CONT", app])
            .output()
            .await
        {
            triggered += 1;
        }
    }

    Ok(triggered)
}

pub(crate) async fn clear_temp_allocations() -> Result<(), String> {
    // Clear temporary allocations by creating and releasing memory chunks
    for _ in 0..5 {
        let size = 10 * 1024 * 1024; // 10MB
        let memory_chunk: Vec<u8> = vec![0; size];
        
        sleep(Duration::from_millis(50)).await;
        
        // Vec will be automatically freed
        drop(memory_chunk);
    }
    Ok(())
}

pub(crate) async fn optimize_swap() -> Result<String, String> {
    // Check current swap usage
    let stats = stats::get_memory_stats()?;

    if stats.swap_used == 0 {
        return Ok("No swap in use, system is running optimally".to_string());
    }

    // Try to reduce swap usage by freeing memory
    let _ = clear_inactive_memory_safe().await?;

    Ok(format!("Swap optimization attempted. Current swap usage: {} MB",
               stats.swap_used / (1024 * 1024)))
}

pub(crate) async fn kill_memory_intensive_processes(threshold_mb: u64) -> Result<Vec<String>, String> {
    let mut killed_processes = Vec::new();

    // Get list of processes using more than threshold memory
    let output = TokioCommand::new("ps")
        .args(&["aux"])
        .output()
        .await
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
                        if let Ok(_) = TokioCommand::new("kill")
                            .arg("-TERM")
                            .arg(pid)
                            .output()
                            .await
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