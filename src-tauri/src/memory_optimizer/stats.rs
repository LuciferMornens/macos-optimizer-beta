use std::process::Command;

use log::warn;

use crate::metrics::{collect_memory_sample, MemoryStats};

use super::utils::{extract_number, extract_sysctl_value, get_page_size};

pub(crate) fn get_memory_stats() -> Result<MemoryStats, String> {
    let sample = collect_memory_sample();
    if let Some(stats) = sample.value.clone() {
        return Ok(stats);
    }

    let reason = sample
        .error
        .clone()
        .unwrap_or_else(|| "unknown Mach error".to_string());
    warn!(
        "Mach memory sampler failed: {}. Falling back to vm_stat",
        reason
    );

    collect_memory_stats_vm_stat().map_err(|err| format!("fallback vm_stat failed: {}", err))
}

fn collect_memory_stats_vm_stat() -> Result<MemoryStats, String> {
    let output = Command::new("vm_stat")
        .output()
        .map_err(|e| format!("Failed to execute vm_stat: {}", e))?;

    if !output.status.success() {
        return Err("vm_stat command failed".to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
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

    let page_size = get_page_size();

    let wired = pages_wired * page_size;
    let compressed = pages_compressed * page_size;
    let cache_files = file_backed * page_size;
    let available = (pages_free + pages_inactive + pages_purgeable + pages_speculative) * page_size;
    let app_memory = pages_active * page_size;

    let total = extract_sysctl_value("hw.memsize")
        .ok_or_else(|| "fall back path unable to read hw.memsize via sysctl".to_string())?;
    let used = total.saturating_sub(available);
    let pressure_percent = if total > 0 {
        (used as f64 / total as f64 * 100.0) as f32
    } else {
        0.0
    };

    let (swap_total, swap_used, swap_free) = parse_swap_usage();

    Ok(MemoryStats {
        total,
        used,
        available,
        wired,
        compressed,
        swap_total,
        swap_used,
        swap_free,
        app_memory,
        cache_files,
        pressure_percent,
        pressure_state: MemoryStats::pressure_state(pressure_percent),
    })
}

fn parse_swap_usage() -> (u64, u64, u64) {
    let output = Command::new("sysctl").arg("vm.swapusage").output();
    if let Ok(output) = output {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let total = parse_swap_field(&text, "total = ").unwrap_or(0);
            let used = parse_swap_field(&text, "used = ").unwrap_or(0);
            let free = parse_swap_field(&text, "free = ").unwrap_or(0);
            return (total, used, free);
        }
    }
    (0, 0, 0)
}

fn parse_swap_field(text: &str, label: &str) -> Option<u64> {
    let start = text.find(label)? + label.len();
    let rest = &text[start..];
    let token = rest.split_whitespace().next()?;
    parse_size_token(token)
}

fn parse_size_token(token: &str) -> Option<u64> {
    let mut numeric = String::new();
    let mut suffix = 'B';

    for ch in token.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            numeric.push(ch);
        } else {
            suffix = ch;
            break;
        }
    }

    let value: f64 = numeric.parse().ok()?;
    let multiplier = match suffix {
        'G' | 'g' => 1024f64 * 1024f64 * 1024f64,
        'M' | 'm' => 1024f64 * 1024f64,
        'K' | 'k' => 1024f64,
        _ => 1.0,
    };

    Some((value * multiplier) as u64)
}
