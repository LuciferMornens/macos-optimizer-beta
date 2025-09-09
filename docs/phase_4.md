# Phase 4: User Experience – Full Implementation Plan

Goal: Add end-to-end operation cancellation, advanced progress (multi-stage + ETA), a robust operation queue, and safe background processing — without regressing Phase 1–3 performance and safety guarantees.

This plan is written for the implementing engineer(s) and maps 1:1 to code changes in the current repository. It assumes the current Phase 3 codebase as of September 2025.

---

## Outcomes
- Operation cancellation works for long-running tasks (scan, clean, memory optimize, deep-clean admin) with safe cleanup.
- Unified progress protocol: structured stages, percent, ETA, throughput (files/s, MB/s), and stable operation IDs.
- Backend concurrency limits to keep the app responsive; frontend queue coordinates intents and displays accurate state.
- Background tasks (cache refresh, idle-time warming) integrated behind features and with resource guards.

---

## Architecture Changes

### 1) Operation Manager (backend)
Create a small runtime service to register, track, cancel, and report on operations.

- New file: `src-tauri/src/ops.rs`
- Types:
  - `OperationId = String` (UUID v4).
  - `OperationKind` enum: `FileScan`, `FileClean`, `EmptyTrash`, `MemOptimize`, `MemOptimizeAdmin`, `DashboardRefresh`, …
  - `OpState` struct: `{ id, kind, started_at, stage, progress, eta_ms, details, cancellable, status }` where `status ∈ {Pending, Running, Completed, Canceled, Failed}`.
  - `OpHandle` struct: `{ cancel: CancellationToken, join: JoinHandle<()>, started_at: Instant }`.
- Storage:
  - `OperationRegistry` inside `AppState` with `DashMap<OperationId, (OpState, OpHandle)>`.
  - Global concurrency guards: `Arc<Semaphore>` per class: `scan_sem (permits=1–2)`, `clean_sem (permits=2)`, `opt_sem (permits=1)` configurable via `PerformanceConfig`.
- API (Rust, Tauri commands):
  - `#[tauri::command] fn cancel_operation(id: String) -> Result<(), String>` — flips token and attempts kill of any child process when applicable.
  - `#[tauri::command] fn get_operation_state(id: String) -> Option<OpState>`.
  - Existing long-running commands emit events already; they will also update the registry state on stage changes.

Dependency: add `tokio-util = { version = "0.7", features = ["rt"] }` to use `CancellationToken`.

### 2) Unified Progress Protocol
We already emit `operation:start`, `progress:update`, `operation:complete`. Extend payloads and ensure every long-running path uses the same schema.

- Start: `operation:start` => `{ operation_id, operation_type, estimated_duration?: u32 }` (existing)
- Update: `progress:update` => `{ operation_id, progress, message, stage, can_cancel, eta_ms?: u32, throughput?: { files_per_s?: f32, mb_per_s?: f32 } }`
- Complete: `operation:complete` => `{ operation_id, success, message, duration }` (existing) or `{ …, canceled: true }` when canceled.
- Error: `operation:error` => `{ operation_id, message }` (emit in addition to `complete` with `success=false`).

Emit frequency target: ≤ 10 Hz to avoid event spam.

---

## Backend Implementation Details

### 3) Cancellation – Patterns and Integration

3.1 Add tokens to commands

- In `src-tauri/src/lib.rs`:
  - When starting an operation (e.g., `scan_cleanable_files`, `clean_files`, `optimize_memory`, `optimize_memory_admin`), allocate `operation_id` and `CancellationToken`, store in registry, then spawn the body within a `tokio::spawn` that acquires the appropriate semaphore.
  - Include `operation_id` in every event emitted (already done in Phase 2 for many commands).
  - New command: `cancel_operation(operation_id)` sets token and, if there is an active child process (osascript or shell), calls `child.kill()`.

3.2 Cooperative checks

- File scan (sequential and parallel) `engine.rs`:
  - At the top of loops over entries or batches, check `if cancel.is_cancelled()`; on true, short-circuit with `Err("cancelled")`.
  - Keep partial results safe: scanning is read-only; no cleanup needed.
  - Emit `operation:complete` with `canceled:true`.

- File cleaning `clean_files_batch` and `clean_directory_batch`:
  - Insert cancellation checks between files and after expensive steps (before AppleScript call, before `remove_dir_all`).
  - If canceled mid-batch, return the accumulated `(total_freed, items_removed)` so far. Because deletion/move are atomic per item, no rollback is necessary.

- Memory optimizer (non-admin):
  - In `non_admin.rs` loops (`clear_inactive_memory_adaptive`, `optimize_memory_compression`, temp allocations), check token each iteration.
  - For `ps/kill` flows, check before sending signals.

- Memory optimizer (admin):
  - In `admin.rs::run_deep_clean`, spawn `osascript` via `tokio::process::Command` and hold the `Child` handle in the operation registry.
  - On cancel: call `child.kill()` and surface `Canceled`.
  - Add a max timeout (e.g., 20 minutes) via `tokio::time::timeout`. On timeout, kill the process and mark failed.

3.3 Resource guards

- Wrap each operation body with `let _permit = scan_sem.acquire().await;` etc., so cancellation is honored before work starts (if the caller cancels while pending).

### 4) Advanced Progress (Stages, ETA, Throughput)

4.1 Stages (canonical list)

- File scan: `initialization` → `caches` → `temp_files` → `analysis` → `complete` (already partially in place)
- File clean: `grouping` → `trash_or_delete` → `verify` → `complete`
- Memory optimize (non-admin): `initialization` → `cache_clear` → `network_cache` → `memory_compression` → `settling` → `complete`
- Memory optimize (admin): `auth` → `disk_cache` → `network_cache` → `kext_cache` → `restart_services` → `complete`

4.2 ETA and throughput

- `OperationMetrics` (src-tauri/src/config.rs) already exists. Extend with rolling window stats to compute:
  - ETA: `remaining_work / avg_rate` where remaining_work is e.g. files remaining or bytes remaining.
  - Throughput: track per-interval deltas of files processed and bytes freed/read.
- File scan throughput: maintain counters for visited entries and matched items per second.
- File clean throughput: bytes freed per second = delta(total_freed) / delta(time).
- Emit `eta_ms` and `throughput` fields in `progress:update` at ≤ 2 Hz.

4.3 Frontend-friendly messages

- Keep `message` concise, like “Deleting 12 files in ~/Downloads …” or “Scanning Caches (2.3k entries)”.

### 5) Operation Queue and Limits

5.1 Backend semaphore limits (server of truth)

- In `AppState`, add `Arc<Semaphore>` per operation class, initialized from `PERFORMANCE_CONFIG`.
- In command launchers, acquire proper permit before spawning heavy work.

5.2 Frontend queue (already exists at a basic level)

- Enhance: priority levels (UserAction > Background > Prefetch), deduplicate identical ops (e.g., avoid kicking off a second scan while one is running), cancel/replace semantics for dashboard refresh.

### 6) Background Processing (Idle-Time)

- Feature-guarded: enable by default only when `cache-refresh` is turned on.
- `CacheRefresher` exists (file_cleaner/cache.rs) — wire it:
  - On setup in `lib.rs`, if feature is on and `PERFORMANCE_CONFIG.enable_background_refresh`, spawn refresher with `refresh_interval`.
  - Idle check: implement simple CPU usage gate using `system_info.rs` (e.g., `get_cpu_info().cpu_usage < 30%`).
  - Ensure database of paths to warm uses user directories only.

### 7) Errors, Timeouts, and Retries

- Standardize error reporting through `operation:error` and `operation:complete { success:false }` with a short message for the UI.
- Add `timeout_ms` optional parameter to the command layer for destructive ops (clean, admin deep clean).

---

## API Changes Summary (Tauri)

New/updated commands in `src-tauri/src/lib.rs`:

- `#[tauri::command] async fn cancel_operation(operation_id: String) -> Result<(), String>`
- `#[tauri::command] async fn get_operation_state(operation_id: String) -> Result<Option<ops::OpState>, String>`
- Update existing commands to return `operation_id` immediately (fire-and-update pattern) when called from the UI queue, or keep current request/response when invoked synchronously:
  - Option A (recommended): introduce parallel “start_…” commands that return `{ operation_id }` and rely on events for updates.
  - Option B: keep current RPCs but also register in the registry and allow cancellation; caller still receives final result.

Events (no breaking rename; payloads extended as above):
- `operation:start`, `progress:update`, `operation:complete`, `operation:error`.

---

## Frontend Implementation (High-Level)

1) Operation Queue service
- Keep a single source of truth for pending/running ops.
- Assign priorities and coalesce duplicates (e.g., a second “scan” becomes a no-op if one is running; or it cancels/replaces the previous one per UX spec).
- Render each operation with progress bar, stage label, ETA, and cancel button when `can_cancel` is true.

2) Event handling
- Subscribe to the four events and update the queue store.
- Compute derived ETA smoothing (exponential moving average) when backend does not provide `eta_ms`.

3) UX details
- Cancel confirmation for destructive tasks (cleaning) unless only moving to Trash.
- Sticky, unobtrusive operation tray with per-op rows and history of last 10.

---

## Pseudocode / Implementation Sketches

### Cancellation token injection
```rust
// src-tauri/src/ops.rs (new)
use tokio_util::sync::CancellationToken;
use std::{time::Instant, sync::Arc};

pub struct OperationRegistry { /* DashMap<OperationId, Entry> */ }

impl OperationRegistry {
    pub fn new() -> Self { /* … */ }
    pub fn register(&self, kind: OperationKind) -> (String, CancellationToken) { /* … UUID + token */ }
    pub fn cancel(&self, id: &str) -> bool { /* flip token; kill child if tracked */ }
}
```

### Using the token in a loop
```rust
// file scan batches
for chunk in entries.chunks(100) {
    if cancel.is_cancelled() { return Err("cancelled".into()); }
    for entry in chunk { /* … */ }
}
```

### Killing a child process on cancel
```rust
// admin.rs
let mut child = Command::new("osascript").arg("-e").arg(script).spawn()?;
tokio::select! {
    res = child.wait_with_output() => { /* normal completion */ }
    _ = cancel.cancelled() => {
        let _ = child.kill().await; // best-effort
        return Err("cancelled".into());
    }
}
```

### Throughput + ETA emission
```rust
let now = Instant::now();
let dt = now - last_tick;
let d_files = files_done - last_files_done;
let d_bytes = bytes_done - last_bytes_done;
let files_per_s = d_files as f32 / dt.as_secs_f32();
let mb_per_s = (d_bytes as f32 / 1_048_576.0) / dt.as_secs_f32();
let eta_ms = if files_per_s > 0.0 { ((total_files - files_done) as f32 / files_per_s * 1000.0) as u32 } else { 0 };
emit(progress_update { throughput: { files_per_s, mb_per_s }, eta_ms });
```

---

## Testing & Validation

1) Unit tests (Rust)
- Cancellation cooperates: inject a token and cancel mid-loop → functions return `Err("cancelled")` and do not panic.
- Admin kill: spawn a dummy long-running child (e.g., `sleep 10`) and verify cancellation kills it.
- Registry consistency: register→cancel→cleanup removes entries.

2) Integration tests (manual / scripted)
- Scan while canceling at different stages; ensure no partial UI dead-ends.
- Start two cleans; queue/semaphore limit prevents more than configured concurrency.
- Admin deep clean cancel shows “canceled” and leaves no lingering `osascript` processes.

3) Performance sanity
- Ensure cancel checks do not introduce hot-path overhead (only 1–2 checks per 100 items processed).
- Event rate capped (~10 Hz).

---

## Rollout & Feature Flags

- Ship cancellation and progress extensions on by default.
- Keep background refresher (`cache-refresh`) and metadata cache (`metadata-cache`) behind features.
- Provide config toggles in `PerformanceConfig` for semaphores and event rate caps.

---

## Acceptance Criteria

- Cancel button appears for all long-running operations and works reliably within <250 ms.
- No orphan child processes remain after cancel (verified via logs and `ps`).
- Progress events include `stage`, `eta_ms` (when measurable), and `throughput` for scan/clean.
- Concurrency limits enforced (no more than configured simultaneous heavy operations).
- Background refresher runs only when enabled and when system is “idle”.

---

## Tasks Checklist

- [ ] Add `ops.rs` (registry + types) and wire into `AppState`.
- [ ] Add `tokio-util` dependency; update `Cargo.toml`.
- [ ] Add `cancel_operation`, `get_operation_state` Tauri commands.
- [ ] Update `lib.rs` command launchers to register operations, emit start/complete, and respect semaphores.
- [ ] Thread `CancellationToken` through: file scan, clean, empty trash, non-admin optimizer, admin deep clean.
- [ ] Insert cooperative cancel checks in loops and between heavy steps.
- [ ] Track child `osascript` and kill on cancel; add overall timeout.
- [ ] Extend `OperationMetrics` for throughput calculations; emit ETA.
- [ ] Cap event emission rate; ensure can_cancel reflects cancelable windows.
- [ ] Frontend: upgrade queue, priorities, UI (ETA, throughput, cancel).
- [ ] Background refresher wiring behind `cache-refresh` and `PerformanceConfig`.
- [ ] Docs: update ROADMAP “Phase 4 (In Progress)” when work starts.

