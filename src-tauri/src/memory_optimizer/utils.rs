// src/memory_optimizer/utils.rs

use libc;

pub(crate) fn get_page_size() -> u64 {
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

pub(crate) fn extract_number(line: &str) -> Option<u64> {
    line.split_whitespace()
        .last()?
        .trim_end_matches('.')
        .parse::<u64>()
        .ok()
}

pub(crate) fn extract_sysctl_value(line: &str) -> Option<u64> {
    line.split(':')
        .last()?
        .trim()
        .parse::<u64>()
        .ok()
}

pub(crate) fn extract_swap_used(swap_str: &str) -> u64 {
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