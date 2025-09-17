use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleEnvelope<T> {
    pub value: Option<T>,
    pub collected_at: DateTime<Utc>,
    pub valid_for_ms: u32,
    pub source: String,
    pub latency_ms: u32,
    pub error: Option<String>,
}

impl<T> SampleEnvelope<T> {
    pub fn fresh(
        value: T,
        collected_at: DateTime<Utc>,
        valid_for: Duration,
        latency: Duration,
        source: &str,
    ) -> Self {
        SampleEnvelope {
            value: Some(value),
            collected_at,
            valid_for_ms: valid_for.as_millis().min(u32::MAX as u128) as u32,
            source: source.to_string(),
            latency_ms: latency.as_millis().min(u32::MAX as u128) as u32,
            error: None,
        }
    }

    pub fn errored(
        collected_at: DateTime<Utc>,
        valid_for: Duration,
        latency: Duration,
        source: &str,
        error: String,
    ) -> Self {
        SampleEnvelope {
            value: None,
            collected_at,
            valid_for_ms: valid_for.as_millis().min(u32::MAX as u128) as u32,
            source: source.to_string(),
            latency_ms: latency.as_millis().min(u32::MAX as u128) as u32,
            error: Some(error),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub wired: u64,
    pub compressed: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub swap_free: u64,
    pub app_memory: u64,
    pub cache_files: u64,
    pub pressure_percent: f32,
    pub pressure_state: String,
}

impl MemoryStats {
    pub fn pressure_state(pressure: f32) -> String {
        if pressure >= 90.0 {
            "critical".to_string()
        } else if pressure >= 75.0 {
            "high".to_string()
        } else if pressure >= 60.0 {
            "elevated".to_string()
        } else {
            "normal".to_string()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub total_usage: f32,
    pub per_core_usage: Vec<f32>,
    pub core_count: usize,
    pub physical_core_count: usize,
    pub brand: String,
    pub frequency_hz: u64,
    pub rolling_min: f32,
    pub rolling_max: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub used_space: u64,
    pub file_system: String,
    pub device: String,
    pub kind: String,
    pub is_removable: bool,
    pub is_system: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UptimeSnapshot {
    pub uptime_seconds: u64,
    pub boot_time_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub captured_at: DateTime<Utc>,
    pub cpu: SampleEnvelope<CpuSnapshot>,
    pub memory: SampleEnvelope<MemoryStats>,
    pub disks: SampleEnvelope<Vec<DiskSnapshot>>,
    pub uptime: SampleEnvelope<UptimeSnapshot>,
}

impl MetricsSnapshot {
    pub fn stale() -> Self {
        let now = Utc::now();
        MetricsSnapshot {
            captured_at: now,
            cpu: SampleEnvelope {
                value: None,
                collected_at: now,
                valid_for_ms: 1000,
                source: "uninitialized".to_string(),
                latency_ms: 0,
                error: Some("metrics sampler not yet initialised".to_string()),
            },
            memory: SampleEnvelope {
                value: None,
                collected_at: now,
                valid_for_ms: 5000,
                source: "uninitialized".to_string(),
                latency_ms: 0,
                error: Some("metrics sampler not yet initialised".to_string()),
            },
            disks: SampleEnvelope {
                value: None,
                collected_at: now,
                valid_for_ms: 30000,
                source: "uninitialized".to_string(),
                latency_ms: 0,
                error: Some("metrics sampler not yet initialised".to_string()),
            },
            uptime: SampleEnvelope {
                value: None,
                collected_at: now,
                valid_for_ms: 1000,
                source: "uninitialized".to_string(),
                latency_ms: 0,
                error: Some("metrics sampler not yet initialised".to_string()),
            },
        }
    }
}
