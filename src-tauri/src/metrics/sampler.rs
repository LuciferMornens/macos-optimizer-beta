use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use log::warn;
use sysinfo::System;
use tokio::select;
use tokio::sync::{Notify, RwLock};
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::cpu::{collect_cpu_sample, CpuSamplerState};
use super::disk::collect_disk_sample;
use super::memory::collect_memory_sample;
use super::types::MetricsSnapshot;
use super::uptime::collect_uptime_sample;

const CPU_PERIOD: Duration = Duration::from_secs(1);
const MEMORY_PERIOD: Duration = Duration::from_secs(5);
const DISK_PERIOD: Duration = Duration::from_secs(30);
const UPTIME_PERIOD: Duration = Duration::from_secs(1);

pub struct MetricsSamplerHandle {
    snapshot: Arc<RwLock<MetricsSnapshot>>,
    ready: Arc<AtomicBool>,
    notify_ready: Arc<Notify>,
    cancel: CancellationToken,
    _runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl MetricsSamplerHandle {
    pub fn spawn() -> Self {
        let snapshot = Arc::new(RwLock::new(MetricsSnapshot::stale()));
        let ready = Arc::new(AtomicBool::new(false));
        let notify_ready = Arc::new(Notify::new());
        let cancel = CancellationToken::new();

        let inner = Arc::new(MetricsSamplerInner {
            snapshot: Arc::clone(&snapshot),
            ready: Arc::clone(&ready),
            notify_ready: Arc::clone(&notify_ready),
            cancel: cancel.clone(),
        });

        let fut = MetricsSamplerInner::run(Arc::clone(&inner));

        let runtime_guard = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(fut);
                None
            }
            Err(_) => {
                let runtime = Arc::new(
                    tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .expect("failed to build metrics sampler runtime"),
                );
                runtime.spawn(fut);
                Some(runtime)
            }
        };

        MetricsSamplerHandle {
            snapshot,
            ready,
            notify_ready,
            cancel,
            _runtime: runtime_guard,
        }
    }

    pub async fn latest_snapshot(&self) -> MetricsSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn wait_until_ready(&self) {
        if self.ready.load(Ordering::SeqCst) {
            return;
        }
        self.notify_ready.notified().await;
    }
}

struct MetricsSamplerInner {
    snapshot: Arc<RwLock<MetricsSnapshot>>,
    ready: Arc<AtomicBool>,
    notify_ready: Arc<Notify>,
    cancel: CancellationToken,
}

impl MetricsSamplerInner {
    async fn run(self: Arc<Self>) {
        let mut system = System::new_all();
        system.refresh_all();
        let mut cpu_state = CpuSamplerState::new(12);
        let mut current = MetricsSnapshot::stale();

        current.cpu = collect_cpu_sample(&mut system, &mut cpu_state).await;
        current.memory = collect_memory_sample();
        current.disks = collect_disk_sample();
        current.uptime = collect_uptime_sample(&mut system);
        current.captured_at = Utc::now();
        self.store_snapshot(&current).await;
        self.ready.store(true, Ordering::SeqCst);
        self.notify_ready.notify_waiters();

        let mut cpu_interval = interval(CPU_PERIOD);
        cpu_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        cpu_interval.tick().await;

        let mut memory_interval = interval(MEMORY_PERIOD);
        memory_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        memory_interval.tick().await;

        let mut disk_interval = interval(DISK_PERIOD);
        disk_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        disk_interval.tick().await;

        let mut uptime_interval = interval(UPTIME_PERIOD);
        uptime_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        uptime_interval.tick().await;

        loop {
            select! {
                _ = self.cancel.cancelled() => {
                    break;
                }
                _ = cpu_interval.tick() => {
                    current.cpu = collect_cpu_sample(&mut system, &mut cpu_state).await;
                    if let Some(err) = current.cpu.error.as_ref() {
                        warn!("cpu sampler error: {}", err);
                    }
                    current.captured_at = Utc::now();
                    self.store_snapshot(&current).await;
                }
                _ = memory_interval.tick() => {
                    current.memory = collect_memory_sample();
                    if let Some(err) = current.memory.error.as_ref() {
                        warn!("memory sampler error: {}", err);
                    }
                    current.captured_at = Utc::now();
                    self.store_snapshot(&current).await;
                }
                _ = disk_interval.tick() => {
                    current.disks = collect_disk_sample();
                    if let Some(err) = current.disks.error.as_ref() {
                        warn!("disk sampler error: {}", err);
                    }
                    current.captured_at = Utc::now();
                    self.store_snapshot(&current).await;
                }
                _ = uptime_interval.tick() => {
                    current.uptime = collect_uptime_sample(&mut system);
                    if let Some(err) = current.uptime.error.as_ref() {
                        warn!("uptime sampler error: {}", err);
                    }
                    current.captured_at = Utc::now();
                    self.store_snapshot(&current).await;
                }
            }
        }
    }

    async fn store_snapshot(&self, snapshot: &MetricsSnapshot) {
        let mut guard = self.snapshot.write().await;
        *guard = snapshot.clone();
    }
}

pub type MetricsSampler = MetricsSamplerHandle;

impl Drop for MetricsSamplerHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
