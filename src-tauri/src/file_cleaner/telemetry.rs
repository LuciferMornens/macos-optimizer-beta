use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelemetrySnapshot {
    pub total_scans: u64,
    pub last_scan_ms: u64,
    pub total_deselections: u64,
}

pub struct SafetyMetricsCollector {
    snapshot: TelemetrySnapshot,
    scan_timer: Option<Instant>,
}

impl SafetyMetricsCollector {
    pub fn new() -> Self {
        Self {
            snapshot: TelemetrySnapshot::default(),
            scan_timer: None,
        }
    }

    pub fn start_scan(&mut self) {
        self.scan_timer = Some(Instant::now());
    }

    pub fn finish_scan(&mut self) {
        self.snapshot.total_scans = self.snapshot.total_scans.saturating_add(1);
        if let Some(t0) = self.scan_timer.take() {
            self.snapshot.last_scan_ms = t0.elapsed().as_millis() as u64;
        }
        let _ = self.persist();
    }

    pub fn track_deselection(&mut self) {
        self.snapshot.total_deselections = self.snapshot.total_deselections.saturating_add(1);
        let _ = self.persist();
    }

    pub fn get_snapshot(&self) -> TelemetrySnapshot {
        self.snapshot.clone()
    }

    fn persist(&self) -> std::io::Result<()> {
        if let Some(mut path) = dirs::data_dir() {
            path.push("macos-optimizer");
            let _ = fs::create_dir_all(&path);
            path.push("telemetry.json");
            let data = serde_json::to_vec_pretty(&self.snapshot).unwrap_or_else(|_| b"{}".to_vec());
            fs::write(path, data)?;
        }
        Ok(())
    }
}
