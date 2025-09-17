use std::time::{Duration, Instant};

use chrono::Utc;
use sysinfo::System;

use super::types::{SampleEnvelope, UptimeSnapshot};

pub fn collect_uptime_sample(_: &mut System) -> SampleEnvelope<UptimeSnapshot> {
    let started = Instant::now();
    let source = "sysinfo::uptime";

    let snapshot = UptimeSnapshot {
        uptime_seconds: System::uptime(),
        boot_time_seconds: System::boot_time(),
    };

    let now = Utc::now();
    let latency = started.elapsed();
    SampleEnvelope::fresh(snapshot, now, Duration::from_millis(1_000), latency, source)
}
