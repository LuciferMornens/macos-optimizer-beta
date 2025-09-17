# macOS Optimizer

macOS Optimizer is a native-feeling desktop utility built with Tauri 2 and Rust that helps keep Macs responsive. It combines a real-time telemetry stack, a memory optimisation pipeline, and a safety-first storage cleaner behind a lightweight JavaScript UI.

## Highlights
- **Real-time telemetry** – a background sampler gathers CPU, memory, disk, and uptime data using native Mach/sysinfo calls and exposes consistent metrics to the dashboard and CLI commands.
- **Memory optimisation modes** – run safe, non-admin cleanups (inactive pages, caches, compression tuning) or escalate to an admin workflow that executes a hardened maintenance script.
- **Storage cleaner with risk scoring** – adaptive rules classify caches, logs, temporary items, developer artefacts, and downloads so you can review, auto-select, and purge confidently.
- **Operations built for trust** – progress events, cancellable queues, and structured notifications keep users informed while long-running optimisations execute in parallel.
- **macOS-first safety** – every destructive action is guard-railed: files are moved to the Trash, protected paths are skipped, and system-level tweaks fall back gracefully if APIs fail.

## Architecture
- **Frontend (`src/`)** – Vanilla ES modules drive the WebView UI (dashboard, memory, storage). A thin `operationQueue` serialises backend calls, and toast/confirm helpers deliver native-feeling UX.
- **Backend (`src-tauri/`)** – Rust modules expose Tauri commands:
  - `metrics/` owns the background sampler and snapshot types.
  - `memory_optimizer/` orchestrates non-admin and admin optimise flows plus rich stats.
  - `file_cleaner/` performs scanning, scoring, and cleanup with cancel support.
  - `ops/` tracks long-lived operations, throttles concurrency, and emits progress events.
- **Docs (`docs/`)** – product requirements and design notes (e.g., telemetry PRD) live here.

## Prerequisites
- macOS 12 Monterey or later (Intel or Apple Silicon).
- [Node.js](https://nodejs.org/) 18+ with npm (Tauri CLI requires modern Node).
- [Rust toolchain](https://rustup.rs/) (stable channel). `rustup` installs automatically via Tauri if missing.
- Xcode Command Line Tools (`xcode-select --install`) so codesign and system headers are available.

## Getting Started
```bash
# Clone the repository
git clone https://github.com/yourusername/macos-optimizer.git
cd macos-optimizer

# Install JS dependencies and the Tauri CLI
npm install

# Start the development build (disables noisy OS_ACTIVITY logs by default)
npm run dev
```
This spins up Vite+Tauri in "dev" mode: the Rust backend recompiles on change and the WebView hot-reloads.

## Building a Release
```bash
# Option 1: via package script
npm run build

# Option 2: helper script (wraps the command above)
./build.sh
```
The signed `.app` bundle lands in `src-tauri/target/release/bundle/macos/`. Drag it to `/Applications` to install.

## Key Workflows
### Dashboard & Telemetry
- `MetricsSampler` polls Mach APIs (`host_statistics64`, `sysinfo`) on staged cadences (1s CPU/uptime, 5s memory, 30s disk).
- Snapshots include freshness metadata, collection latency, and error state so the UI can surface stale or degraded metrics.
- `get_metrics_snapshot` powers the dashboard and memory panel; `get_system_info` augments it with OS metadata.

### Memory Optimiser
- **Quick optimise** (`optimize_memory`) runs in parallel: clears inactive pages, trims caches, triggers GC hooks, and respects cancellation tokens.
- **Admin optimise** (`optimize_memory_admin`) prompts for credentials then executes a curated maintenance script (swap purge, DNS flush, etc.) via safe subprocess orchestration.
- `MemoryOptimizer::get_memory_stats` now reuses the sampler’s Mach-backed stats with a `vm_stat` fallback for resilience.

### Storage Cleaner
- Enhanced scans (`scan_cleanable_files_enhanced`) walk caches, downloads, logs, and developer tool artefacts via `walkdir` and heuristics.
- Every candidate receives a safety grade; risky items are opt-in and always routed through the Trash.
- Progress events keep the UI responsive during multi-stage scans, and a preview endpoint lets users inspect the generated cleanup plan.

### Processes & System Tools
- View top memory consumers, kill runaway PIDs, inspect network/temperature telemetry, and clear inactive RAM directly from the UI.
- Operations are registered with `OperationRegistry`, providing cancellation handles, throttling, and consistent progress telemetry for front-end consumers.

## Development Notes
- **Project layout**
  - `src/` – UI scripts, styles, and components.
  - `src-tauri/src/metrics/` – telemetry sampler, snapshot structs, async runtime bootstrap.
  - `src-tauri/src/memory_optimizer/` – non/admin optimisations and stats helpers.
  - `src-tauri/src/file_cleaner/` – scanning engines, rule engines, and tests.
  - `docs/` – product specs and improvement logs.
- **Logging** – enable detailed backend logs with `RUST_LOG=debug npm run dev`.
- **Environment** – most commands are macOS-specific; running on other platforms is not supported.

## Testing
### Rust backend (default)
```bash
# From the repository root
cargo test --manifest-path src-tauri/Cargo.toml
```
This command compiles the Tauri workspace in test mode and executes every unit and integration test, including the storage cleaner suite. The build takes place entirely in `src-tauri/`, so the top-level Node dependencies do not need to be rebuilt beforehand.

### Targeted test runs
```bash
# Example: run only the telemetry sampler test
cargo test --manifest-path src-tauri/Cargo.toml metrics::tests::sampler_emits_recent_snapshot

# Example: run just the storage cleaner tests
cargo test --manifest-path src-tauri/Cargo.toml --test storage_cleaner
```
If you are iterating on a single module, use the fully qualified test path (as in the sampler example) or pass `--test <name>` to select an integration test binary. The storage cleaner tests exercise macOS-specific paths and expect a standard user environment, so run them on a Mac with typical user folders present.

## Troubleshooting
- **Build fails / codesign errors** – confirm Xcode Command Line Tools are installed and that you’ve accepted the license (`sudo xcodebuild -license`).
- **Admin optimise prompts repeatedly** – advanced maintenance steps require an unlocked keychain and admin rights; cancel the flow if elevated access is unavailable.
- **Slow scans on first run** – caches rebuild rule metadata; subsequent scans reuse warmed data.
- **Verbose OS logging** – override `OS_ACTIVITY_MODE` if you need raw system logs: `OS_ACTIVITY_MODE=default npm run dev`.

## Contributing
Issues and pull requests are welcome. Please:
1. Target feature branches and keep PRs focused.
2. Run `cargo fmt` / `cargo test` before submitting.
3. Note any UI-visible changes so screenshots/docs stay current.

---
macOS Optimizer is macOS-only free software. Use responsibly—always review the cleanup plan before deleting files.
