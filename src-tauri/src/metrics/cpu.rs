use std::collections::VecDeque;
use std::time::{Duration, Instant};

use chrono::Utc;
use sysinfo::System;
use tokio::time::sleep;

use super::types::{CpuSnapshot, SampleEnvelope};

pub struct CpuSamplerState {
    warmed_up: bool,
    window: VecDeque<f32>,
    window_size: usize,
}

impl CpuSamplerState {
    pub fn new(window_size: usize) -> Self {
        CpuSamplerState {
            warmed_up: false,
            window: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    fn record(&mut self, value: f32) -> (f32, f32) {
        if self.window.len() == self.window_size {
            self.window.pop_front();
        }
        self.window.push_back(value);
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for v in &self.window {
            if *v < min {
                min = *v;
            }
            if *v > max {
                max = *v;
            }
        }
        if min == f32::MAX {
            min = value;
        }
        if max == f32::MIN {
            max = value;
        }
        (min, max)
    }
}

pub async fn collect_cpu_sample(
    system: &mut System,
    state: &mut CpuSamplerState,
) -> SampleEnvelope<CpuSnapshot> {
    let started = Instant::now();
    let source = "sysinfo::cpu";

    if !state.warmed_up {
        system.refresh_cpu();
        sleep(Duration::from_millis(125)).await;
        system.refresh_cpu();
        state.warmed_up = true;
    } else {
        system.refresh_cpu();
    }

    let cpus = system.cpus();
    if cpus.is_empty() {
        let now = Utc::now();
        let latency = started.elapsed();
        return SampleEnvelope::errored(
            now,
            Duration::from_millis(1000),
            latency,
            source,
            "cpu list empty".to_string(),
        );
    }

    let per_core_usage: Vec<f32> = cpus.iter().map(|cpu| cpu.cpu_usage()).collect();
    let total_usage = per_core_usage.iter().copied().sum::<f32>() / per_core_usage.len() as f32;
    let (rolling_min, rolling_max) = state.record(total_usage);

    let snapshot = CpuSnapshot {
        total_usage,
        per_core_usage,
        core_count: cpus.len(),
        physical_core_count: system.physical_core_count().unwrap_or(0),
        brand: cpus
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_default(),
        frequency_hz: cpus.first().map(|cpu| cpu.frequency()).unwrap_or_default(),
        rolling_min,
        rolling_max,
    };

    let now = Utc::now();
    let latency = started.elapsed();
    SampleEnvelope::fresh(snapshot, now, Duration::from_millis(1000), latency, source)
}
