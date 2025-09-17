use std::path::Path;
use std::time::{Duration, Instant};

use chrono::Utc;
use sysinfo::{DiskKind, Disks};

use super::types::{DiskSnapshot, SampleEnvelope};

pub fn collect_disk_sample() -> SampleEnvelope<Vec<DiskSnapshot>> {
    let started = Instant::now();
    let source = "sysinfo::disks";

    let mut disks = Disks::new_with_refreshed_list();
    disks.refresh();
    let list = disks.list();

    if list.is_empty() {
        let now = Utc::now();
        let latency = started.elapsed();
        return SampleEnvelope::errored(
            now,
            Duration::from_millis(30_000),
            latency,
            source,
            "no disks discovered".to_string(),
        );
    }

    let snapshots = list
        .iter()
        .map(|disk| {
            let total_space = disk.total_space();
            let available_space = disk.available_space();
            let used_space = total_space.saturating_sub(available_space);
            let kind = match disk.kind() {
                DiskKind::SSD => "ssd",
                DiskKind::HDD => "hdd",
                DiskKind::Unknown(_) => "unknown",
            };
            let mount_point = disk.mount_point().to_string_lossy().to_string();

            DiskSnapshot {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: mount_point.clone(),
                total_space,
                available_space,
                used_space,
                file_system: disk.file_system().to_string_lossy().to_string(),
                device: disk.name().to_string_lossy().to_string(),
                kind: kind.to_string(),
                is_removable: disk.is_removable(),
                is_system: Path::new(&mount_point) == Path::new("/"),
            }
        })
        .collect();

    let now = Utc::now();
    let latency = started.elapsed();
    SampleEnvelope::fresh(
        snapshots,
        now,
        Duration::from_millis(30_000),
        latency,
        source,
    )
}
