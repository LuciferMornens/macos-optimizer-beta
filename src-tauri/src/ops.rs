use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub type OperationId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationStatus {
    Pending,
    Running,
    Completed,
    Canceled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    FileScan,
    FileClean,
    EmptyTrash,
    MemOptimize,
    MemOptimizeAdmin,
    DashboardRefresh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpState {
    pub id: OperationId,
    pub kind: OperationKind,
    pub started_at_ms: u128,
    pub stage: String,
    pub progress: f32,
    pub eta_ms: Option<u32>,
    pub details: Option<String>,
    pub cancellable: bool,
    pub status: OperationStatus,
}

#[derive(Debug)]
pub struct OpHandle {
    pub token: CancellationToken,
    pub _started_at: Instant,
}

#[derive(Clone)]
pub struct OperationRegistry {
    inner: Arc<DashMap<OperationId, (OpState, Arc<OpHandle>)>>,
    // Global concurrency guards
    pub scan_sem: Arc<Semaphore>,
    pub clean_sem: Arc<Semaphore>,
    pub opt_sem: Arc<Semaphore>,
}

impl OperationRegistry {
    pub fn new(scan_permits: usize, clean_permits: usize, opt_permits: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            scan_sem: Arc::new(Semaphore::new(scan_permits.max(1))),
            clean_sem: Arc::new(Semaphore::new(clean_permits.max(1))),
            opt_sem: Arc::new(Semaphore::new(opt_permits.max(1))),
        }
    }

    pub fn register(&self, kind: OperationKind, cancellable: bool) -> (OperationId, CancellationToken) {
        let id = uuid::Uuid::new_v4().to_string();
        let token = CancellationToken::new();
        let state = OpState {
            id: id.clone(),
            kind: kind.clone(),
            started_at_ms: now_ms(),
            stage: "pending".into(),
            progress: 0.0,
            eta_ms: None,
            details: None,
            cancellable,
            status: OperationStatus::Pending,
        };
        let handle = Arc::new(OpHandle { token: token.clone(), _started_at: Instant::now() });
        self.inner.insert(id.clone(), (state, handle));
        (id, token)
    }

    pub fn update(&self, id: &str, mut f: impl FnMut(&mut OpState)) {
        if let Some(mut entry) = self.inner.get_mut(id) {
            f(&mut entry.0);
        }
    }

    pub fn get(&self, id: &str) -> Option<OpState> {
        self.inner.get(id).map(|e| e.0.clone())
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Some(entry) = self.inner.get(id) {
            entry.1.token.cancel();
            true
        } else {
            false
        }
    }

    pub fn finish_success(&self, id: &str) {
        self.update(id, |s| {
            s.status = OperationStatus::Completed;
            s.progress = 100.0;
            s.stage = "complete".into();
        });
        // keep short history; remove after completion to avoid growth
        // callers can query final state right after completion event
        let _ = self.inner.remove(id);
    }

    pub fn finish_canceled(&self, id: &str) {
        self.update(id, |s| {
            s.status = OperationStatus::Canceled;
        });
        let _ = self.inner.remove(id);
    }

    pub fn finish_failed(&self, id: &str, _msg: &str) {
        self.update(id, |s| {
            s.status = OperationStatus::Failed;
        });
        let _ = self.inner.remove(id);
    }
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

// Simple throughput helper to compute per-tick metrics
#[derive(Default, Clone)]
pub struct ThroughputTracker {
    last_tick: Option<Instant>,
    last_files: u64,
    last_bytes: u64,
}

impl ThroughputTracker {
    pub fn tick(&mut self, files_done: u64, bytes_done: u64, total_files: u64) -> (Option<u32>, Option<f32>, Option<f32>) {
        let now = Instant::now();
        if let Some(prev) = self.last_tick {
            let dt = now.duration_since(prev).as_secs_f32().max(0.001);
            let df = (files_done.saturating_sub(self.last_files)) as f32;
            let db = (bytes_done.saturating_sub(self.last_bytes)) as f32;
            let files_per_s = df / dt;
            let mb_per_s = (db / 1_048_576.0) / dt;
            let eta_ms = if files_per_s > 0.0 {
                let rem = (total_files.saturating_sub(files_done)) as f32;
                Some(((rem / files_per_s) * 1000.0) as u32)
            } else { None };
            self.last_tick = Some(now);
            self.last_files = files_done;
            self.last_bytes = bytes_done;
            (eta_ms, Some(files_per_s), Some(mb_per_s))
        } else {
            self.last_tick = Some(now);
            self.last_files = files_done;
            self.last_bytes = bytes_done;
            (None, None, None)
        }
    }
}
