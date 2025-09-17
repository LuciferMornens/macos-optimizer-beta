use std::ffi::CString;
use std::fmt;
use std::mem::MaybeUninit;
use std::ptr;
use std::time::{Duration, Instant};

use chrono::Utc;
use libc::{
    c_void, host_statistics64, mach_msg_type_number_t, vm_statistics64, HOST_VM_INFO64,
    HOST_VM_INFO64_COUNT,
};

use super::types::{MemoryStats, SampleEnvelope};

#[derive(Debug)]
pub enum MemorySampleError {
    MachCallFailed(&'static str, i32),
    SysctlError(String, std::io::Error),
}

impl fmt::Display for MemorySampleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemorySampleError::MachCallFailed(call, code) => {
                write!(f, "{} returned {}", call, code)
            }
            MemorySampleError::SysctlError(name, err) => {
                write!(f, "sysctl {} failed: {}", name, err)
            }
        }
    }
}

impl std::error::Error for MemorySampleError {}

#[repr(C)]
struct XswUsage {
    total: u64,
    avail: u64,
    used: u64,
    pagesize: i32,
    encrypted: i32,
}

pub fn collect_memory_sample() -> SampleEnvelope<MemoryStats> {
    let started = Instant::now();
    let source = "mach::host_statistics64";

    unsafe {
        #[allow(deprecated)]
        let host = libc::mach_host_self();

        let mut stats = MaybeUninit::<vm_statistics64>::uninit();
        let mut count: mach_msg_type_number_t = HOST_VM_INFO64_COUNT;
        let result = host_statistics64(
            host,
            HOST_VM_INFO64,
            stats.as_mut_ptr() as *mut _,
            &mut count,
        );
        if result != 0 {
            let now = Utc::now();
            let latency = started.elapsed();
            return SampleEnvelope::errored(
                now,
                Duration::from_millis(5_000),
                latency,
                source,
                MemorySampleError::MachCallFailed("host_statistics64", result).to_string(),
            );
        }
        let stats = stats.assume_init();

        let page_size_raw = libc::sysconf(libc::_SC_PAGESIZE);
        let page_size = if page_size_raw > 0 {
            page_size_raw as u64
        } else {
            4096
        };

        let total = match read_sysctl_u64("hw.memsize") {
            Ok(value) => value,
            Err(err) => {
                let now = Utc::now();
                let latency = started.elapsed();
                return SampleEnvelope::errored(
                    now,
                    Duration::from_millis(5_000),
                    latency,
                    source,
                    err.to_string(),
                );
            }
        };

        let swap = match read_swap_usage() {
            Ok(value) => value,
            Err(err) => {
                let now = Utc::now();
                let latency = started.elapsed();
                return SampleEnvelope::errored(
                    now,
                    Duration::from_millis(5_000),
                    latency,
                    source,
                    err.to_string(),
                );
            }
        };

        let free = stats.free_count as u64 * page_size;
        let inactive = stats.inactive_count as u64 * page_size;
        let speculative = stats.speculative_count as u64 * page_size;
        let purgeable = stats.purgeable_count as u64 * page_size;
        let wired = stats.wire_count as u64 * page_size;
        let active = stats.active_count as u64 * page_size;
        let compressed = stats.compressor_page_count as u64 * page_size;
        let external = stats.external_page_count as u64 * page_size;

        let available = free + inactive + speculative + purgeable;
        let used = total.saturating_sub(available);
        let pressure_percent = if total > 0 {
            (used as f64 / total as f64 * 100.0) as f32
        } else {
            0.0
        };

        let snapshot = MemoryStats {
            total,
            used,
            available,
            wired,
            compressed,
            swap_total: swap.total,
            swap_used: swap.used,
            swap_free: swap.avail,
            app_memory: active,
            cache_files: external,
            pressure_percent,
            pressure_state: MemoryStats::pressure_state(pressure_percent),
        };

        let now = Utc::now();
        let latency = started.elapsed();
        SampleEnvelope::fresh(snapshot, now, Duration::from_millis(5_000), latency, source)
    }
}

fn read_sysctl_u64(name: &str) -> Result<u64, MemorySampleError> {
    let c_name = CString::new(name).expect("sysctl name");
    let mut size: libc::size_t = std::mem::size_of::<u64>() as libc::size_t;
    let mut value: u64 = 0;
    let result = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            &mut value as *mut u64 as *mut c_void,
            &mut size as *mut libc::size_t,
            ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return Err(MemorySampleError::SysctlError(
            name.to_string(),
            std::io::Error::last_os_error(),
        ));
    }
    Ok(value)
}

fn read_swap_usage() -> Result<XswUsage, MemorySampleError> {
    let c_name = CString::new("vm.swapusage").expect("sysctl name");
    let mut usage = MaybeUninit::<XswUsage>::uninit();
    let mut size: libc::size_t = std::mem::size_of::<XswUsage>() as libc::size_t;
    let result = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            usage.as_mut_ptr() as *mut c_void,
            &mut size as *mut libc::size_t,
            ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return Err(MemorySampleError::SysctlError(
            "vm.swapusage".to_string(),
            std::io::Error::last_os_error(),
        ));
    }
    let usage = unsafe { usage.assume_init() };
    Ok(usage)
}
