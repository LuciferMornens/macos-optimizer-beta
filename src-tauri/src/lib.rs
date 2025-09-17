mod config;
mod file_cleaner;
mod memory_optimizer;
mod ops;
mod system_info;

use file_cleaner::{
    CleanableFile, CleaningReport, EnhancedCleaningReport, EnhancedFileCleaner, FileCleaner,
    UserAction,
};
use memory_optimizer::{MemoryOptimizationResult, MemoryOptimizer, MemoryStats};
use system_info::{CpuInfo, DiskInfo, MemoryInfo, ProcessInfo, SystemInfo, SystemMonitor};

use file_cleaner::{load_rules_result, DynamicRuleEngine, RuleValidator};
use ops::{OperationKind, OperationRegistry, ThroughputTracker};
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tokio::sync::RwLock;

// Progress event types for real-time operation feedback
#[derive(Clone, Serialize)]
struct ProgressEvent {
    operation_id: String,
    progress: f32, // 0.0 to 100.0
    message: String,
    stage: String,
    can_cancel: bool,
    eta_ms: Option<u32>,
    throughput: Option<Throughput>,
}

#[derive(Clone, Serialize)]
struct OperationStartEvent {
    operation_id: String,
    operation_type: String,
    estimated_duration: Option<u32>, // milliseconds
}

#[derive(Clone, Serialize)]
struct OperationCompleteEvent {
    operation_id: String,
    success: bool,
    message: String,
    duration: u32, // actual duration in ms
    canceled: Option<bool>,
}

#[derive(Clone, Serialize)]
struct Throughput {
    files_per_s: Option<f32>,
    mb_per_s: Option<f32>,
}

// Create a state to manage our system monitor
struct AppState {
    system_monitor: RwLock<SystemMonitor>,
    file_cleaner: RwLock<FileCleaner>,
    enhanced_file_cleaner: RwLock<EnhancedFileCleaner>,
    memory_optimizer: RwLock<MemoryOptimizer>,
    ops: OperationRegistry,
}

#[tauri::command]
async fn get_system_info(state: State<'_, AppState>) -> Result<SystemInfo, String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.refresh();
    Ok(monitor.get_system_info())
}

#[tauri::command]
async fn get_memory_info(state: State<'_, AppState>) -> Result<MemoryInfo, String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.refresh();
    Ok(monitor.get_memory_info())
}

#[tauri::command]
async fn get_cpu_info(state: State<'_, AppState>) -> Result<CpuInfo, String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.refresh();
    Ok(monitor.get_cpu_info())
}

#[tauri::command]
async fn get_processes(state: State<'_, AppState>) -> Result<Vec<ProcessInfo>, String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.refresh();
    Ok(monitor.get_processes())
}

#[tauri::command]
async fn get_top_memory_processes(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<ProcessInfo>, String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.refresh();
    Ok(monitor.get_top_memory_processes(limit))
}

#[tauri::command]
async fn get_disks(state: State<'_, AppState>) -> Result<Vec<DiskInfo>, String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.refresh();
    Ok(monitor.get_disks())
}

#[tauri::command]
async fn kill_process(state: State<'_, AppState>, pid: u32) -> Result<(), String> {
    let mut monitor = state.system_monitor.write().await;
    monitor.kill_process(pid)
}

#[tauri::command]
async fn scan_cleanable_files(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CleaningReport, String> {
    let (operation_id, token) = state.ops.register(OperationKind::FileScan, true);
    let start_time = std::time::Instant::now();

    // Emit start event
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "file_scan".to_string(),
                estimated_duration: Some(8000),
            },
        )
        .ok();

    let mut cleaner = state.file_cleaner.write().await;

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 10.0,
                message: "Starting file system scan...".to_string(),
                stage: "initialization".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 25.0,
                message: "Scanning caches directory...".to_string(),
                stage: "caches".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 50.0,
                message: "Scanning temporary files...".to_string(),
                stage: "temp_files".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 75.0,
                message: "Analyzing file safety...".to_string(),
                stage: "analysis".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    // Concurrency: limit scans
    let _permit = state.ops.scan_sem.acquire().await;
    let result = cleaner.scan_system_with_cancel(&token).await;

    let duration = start_time.elapsed().as_millis() as u32;

    match &result {
        Ok(_) => {
            app_handle
                .emit(
                    "progress:update",
                    ProgressEvent {
                        operation_id: operation_id.clone(),
                        progress: 100.0,
                        message: "File scan completed successfully".to_string(),
                        stage: "complete".to_string(),
                        can_cancel: false,
                        eta_ms: Some(0),
                        throughput: None,
                    },
                )
                .ok();

            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: true,
                        message: "File scan completed".to_string(),
                        duration,
                        canceled: Some(false),
                    },
                )
                .ok();
            state.ops.finish_success(&operation_id);
        }
        Err(err) => {
            let canceled = err.contains("cancelled");
            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: false,
                        message: format!("File scan failed: {}", err),
                        duration,
                        canceled: Some(canceled),
                    },
                )
                .ok();
            if canceled {
                state.ops.finish_canceled(&operation_id);
            } else {
                state.ops.finish_failed(&operation_id, &err);
            }
        }
    }

    result
}

#[tauri::command]
async fn scan_cleanable_files_enhanced(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<EnhancedCleaningReport, String> {
    let (operation_id, token) = state.ops.register(OperationKind::FileScan, true);
    let start_time = std::time::Instant::now();

    // Emit start event
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "enhanced_file_scan".to_string(),
                estimated_duration: Some(15000), // Enhanced scan takes longer
            },
        )
        .ok();

    let mut cleaner = state.enhanced_file_cleaner.write().await;

    // Progress updates for enhanced scan
    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 10.0,
                message: "Starting enhanced file system scan with safety analysis...".to_string(),
                stage: "initialization".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    // Perform the enhanced scan
    let op_id = operation_id.clone();
    let app_for_cb = app_handle.clone();
    let progress_cb = move |progress: f32, message: &str, stage: &str| {
        let _ = app_for_cb.emit(
            "progress:update",
            ProgressEvent {
                operation_id: op_id.clone(),
                progress,
                message: message.to_string(),
                stage: stage.to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        );
    };
    let result = cleaner
        .scan_system_enhanced_with_cancel(&token, Some(&progress_cb))
        .await;

    let duration = start_time.elapsed().as_millis() as u32;

    match &result {
        Ok(_) => {
            app_handle
                .emit(
                    "progress:update",
                    ProgressEvent {
                        operation_id: operation_id.clone(),
                        progress: 100.0,
                        message: "Enhanced scan completed with safety analysis".to_string(),
                        stage: "complete".to_string(),
                        can_cancel: false,
                        eta_ms: Some(0),
                        throughput: None,
                    },
                )
                .ok();

            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: true,
                        message: "Enhanced file scan completed".to_string(),
                        duration,
                        canceled: Some(false),
                    },
                )
                .ok();
            state.ops.finish_success(&operation_id);
        }
        Err(err) => {
            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: false,
                        message: format!("Enhanced scan failed: {}", err),
                        duration,
                        canceled: Some(false),
                    },
                )
                .ok();
            if err.contains("cancelled") {
                state.ops.finish_canceled(&operation_id);
            } else {
                state.ops.finish_failed(&operation_id, &err);
            }
        }
    }

    result
}

#[tauri::command]
async fn get_cleanable_files(state: State<'_, AppState>) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.read().await;
    Ok(cleaner.get_cleanable_files().clone())
}

#[tauri::command]
async fn get_auto_selectable_files(
    state: State<'_, AppState>,
) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.read().await;
    Ok(cleaner.get_auto_selectable_files())
}

#[tauri::command]
async fn get_files_by_safety(
    state: State<'_, AppState>,
    min_safety_score: u8,
) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.read().await;
    Ok(cleaner.get_files_by_safety(min_safety_score))
}

#[tauri::command]
async fn clean_files_enhanced(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    file_paths: Vec<String>,
    allow_low_safety: Option<bool>,
) -> Result<file_cleaner::enhanced_engine::CleaningResult, String> {
    let (operation_id, token) = state.ops.register(OperationKind::FileClean, true);
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "enhanced_file_clean".into(),
                estimated_duration: None,
            },
        )
        .ok();

    let mut cleaner = state.enhanced_file_cleaner.write().await;
    let allow_low_safety = allow_low_safety.unwrap_or(false);

    // Use enhanced cleaning with validation and recovery
    let result = cleaner
        .clean_files_enhanced(file_paths, Some(&token), allow_low_safety)
        .await;

    match &result {
        Ok(cleaning_result) => {
            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: true,
                        message: format!(
                            "Enhanced cleaning completed: {} files deleted, {} MB freed",
                            cleaning_result.deleted_count,
                            cleaning_result.total_freed / (1024 * 1024)
                        ),
                        duration: 0,
                        canceled: Some(false),
                    },
                )
                .ok();
            state.ops.finish_success(&operation_id);
        }
        Err(err) => {
            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: false,
                        message: format!("Enhanced cleaning failed: {}", err),
                        duration: 0,
                        canceled: Some(false),
                    },
                )
                .ok();
            if err.contains("cancelled") {
                state.ops.finish_canceled(&operation_id);
            } else {
                state.ops.finish_failed(&operation_id, &err);
            }
        }
    }

    result
}

#[tauri::command]
async fn prepare_deletion_enhanced(
    state: State<'_, AppState>,
    file_paths: Vec<String>,
) -> Result<file_cleaner::enhanced_engine::DeletionPreparation, String> {
    let mut cleaner = state.enhanced_file_cleaner.write().await;
    cleaner.prepare_deletion_by_paths(&file_paths).await
}

#[tauri::command]
async fn record_user_feedback(
    state: State<'_, AppState>,
    file_path: String,
    action: String,
) -> Result<(), String> {
    let mut cleaner = state.enhanced_file_cleaner.write().await;

    let user_action = match action.as_str() {
        "selected" => UserAction::Selected,
        "deselected" => UserAction::Deselected,
        "ignored" => UserAction::Ignored,
        _ => return Err("Invalid action".to_string()),
    };

    cleaner.record_user_feedback(&file_path, user_action);
    Ok(())
}

#[tauri::command]
async fn get_active_development_tools(_state: State<'_, AppState>) -> Result<Vec<String>, String> {
    // This provides information about active development tools
    let checker = file_cleaner::smart_cache::AppActivityChecker::new();
    Ok(checker.get_active_development_tools())
}

#[tauri::command]
async fn preview_rules(
) -> Result<(Vec<file_cleaner::RuleConflict>, file_cleaner::DryRunReport), String> {
    let base = load_rules_result()?;
    let dyn_eng = DynamicRuleEngine::new();
    let mut adapted = dyn_eng.adapt_rules_to_system(&base);
    let mut extra = dyn_eng.generate_app_specific_rules();
    adapted.categories.append(&mut extra);
    let validator = RuleValidator::new();
    let conflicts = validator.validate_rule_consistency(&adapted);
    let report = validator.dry_run_rules(&adapted);
    Ok((conflicts, report))
}

#[tauri::command]
async fn get_enhanced_telemetry(
    state: State<'_, AppState>,
) -> Result<file_cleaner::telemetry::TelemetrySnapshot, String> {
    let cleaner = state.enhanced_file_cleaner.read().await;
    Ok(cleaner.telemetry_snapshot())
}

#[tauri::command]
async fn clean_files(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    file_paths: Vec<String>,
) -> Result<(u64, usize), String> {
    let (operation_id, token) = state.ops.register(OperationKind::FileClean, true);
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "file_clean".into(),
                estimated_duration: None,
            },
        )
        .ok();

    // Concurrency control
    let _permit = state.ops.clean_sem.acquire().await;

    // Pre-compute totals for ETA/throughput
    use std::fs;
    let total_files = file_paths.len() as u64;
    let mut total_bytes: u64 = 0;
    for p in &file_paths {
        if let Ok(md) = fs::metadata(p) {
            total_bytes = total_bytes.saturating_add(md.len());
        }
    }
    let mut files_done = 0u64;
    let mut bytes_done = 0u64;
    let mut tracker = ThroughputTracker::default();

    let chunk_size = 50usize;
    let mut total_freed = 0u64;
    let mut total_removed = 0usize;
    let cleaner = state.file_cleaner.read().await;
    for chunk in file_paths.chunks(chunk_size) {
        if token.is_cancelled() {
            break;
        }
        let (freed, removed) = cleaner
            .clean_files_with_cancel(chunk.to_vec(), &token)
            .await?;
        total_freed += freed;
        total_removed += removed;
        files_done += chunk.len() as u64;
        bytes_done = bytes_done.saturating_add(freed);
        let (eta_ms, fps, mbs) = tracker.tick(files_done, bytes_done, total_files);
        let progress = if total_files > 0 {
            (files_done as f32 / total_files as f32) * 100.0
        } else {
            100.0
        };
        app_handle
            .emit(
                "progress:update",
                ProgressEvent {
                    operation_id: operation_id.clone(),
                    progress,
                    message: format!("Cleaning files… {}/{}", files_done, total_files),
                    stage: "deleting".into(),
                    can_cancel: true,
                    eta_ms,
                    throughput: Some(Throughput {
                        files_per_s: fps,
                        mb_per_s: mbs,
                    }),
                },
            )
            .ok();
    }

    let canceled = token.is_cancelled();
    app_handle
        .emit(
            "operation:complete",
            OperationCompleteEvent {
                operation_id: operation_id.clone(),
                success: !canceled,
                message: if canceled {
                    "Cleaning canceled".into()
                } else {
                    "Cleaning completed".into()
                },
                duration: 0,
                canceled: Some(canceled),
            },
        )
        .ok();
    if canceled {
        state.ops.finish_canceled(&operation_id);
    } else {
        state.ops.finish_success(&operation_id);
    }
    Ok((total_freed, total_removed))
}

#[tauri::command]
async fn empty_trash(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(u64, usize), String> {
    let (operation_id, token) = state.ops.register(OperationKind::EmptyTrash, true);
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "empty_trash".into(),
                estimated_duration: None,
            },
        )
        .ok();
    let _permit = state.ops.clean_sem.acquire().await;
    let cleaner = state.file_cleaner.read().await;
    let res = cleaner.empty_trash_with_cancel(&token).await;
    let canceled = token.is_cancelled();
    app_handle
        .emit(
            "operation:complete",
            OperationCompleteEvent {
                operation_id: operation_id.clone(),
                success: res.is_ok() && !canceled,
                message: if canceled {
                    "Trash empty canceled".into()
                } else {
                    "Trash emptied".into()
                },
                duration: 0,
                canceled: Some(canceled),
            },
        )
        .ok();
    if canceled {
        state.ops.finish_canceled(&operation_id);
    } else if res.is_ok() {
        state.ops.finish_success(&operation_id);
    } else {
        state.ops.finish_failed(&operation_id, "empty_trash failed");
    }
    res
}

#[tauri::command]
async fn restore_from_trash(
    _app_handle: tauri::AppHandle,
    _state: State<'_, AppState>,
    file_names: Vec<String>,
) -> Result<usize, String> {
    // Naive restore: move files from ~/.Trash back to user's Downloads directory
    // This can be enhanced later to track original locations from recovery metadata
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let trash = home.join(".Trash");
    let downloads = home.join("Downloads");
    let mut restored = 0usize;

    for name in file_names {
        let src = trash.join(&name);
        if !src.exists() {
            continue;
        }
        let mut target = downloads.join(&name);
        // Ensure unique name
        let mut counter = 1u32;
        while target.exists() {
            let (base, ext) = {
                if let Some(idx) = name.rfind('.') {
                    let (b, e) = name.split_at(idx);
                    (b.to_string(), e.trim_start_matches('.').to_string())
                } else {
                    (name.clone(), String::new())
                }
            };
            let candidate = if ext.is_empty() {
                format!("{} (restored-{})", base, counter)
            } else {
                format!("{} (restored-{}).{}", base, counter, ext)
            };
            target = downloads.join(candidate);
            counter += 1;
        }
        if std::fs::rename(&src, &target).is_ok() {
            restored += 1;
        }
    }

    Ok(restored)
}

#[tauri::command]
async fn optimize_memory(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<MemoryOptimizationResult, String> {
    let (operation_id, token) = state.ops.register(OperationKind::MemOptimize, true);
    let start_time = std::time::Instant::now();

    // Emit start event
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "memory_optimization".to_string(),
                estimated_duration: Some(3000),
            },
        )
        .ok();

    let optimizer = state.memory_optimizer.read().await;

    // Progress: Starting optimization
    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 10.0,
                message: "Starting memory optimization...".to_string(),
                stage: "initialization".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Progress: Clearing caches
    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 30.0,
                message: "Clearing application caches...".to_string(),
                stage: "cache_clear".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    // Perform the actual optimization with cancel + concurrency guard
    let _permit = state.ops.opt_sem.acquire().await;
    let result = optimizer.optimize_memory_with_cancel(&token).await;

    let duration = start_time.elapsed().as_millis() as u32;

    match &result {
        Ok(_) => {
            app_handle
                .emit(
                    "progress:update",
                    ProgressEvent {
                        operation_id: operation_id.clone(),
                        progress: 100.0,
                        message: "Memory optimization completed successfully".to_string(),
                        stage: "complete".to_string(),
                        can_cancel: false,
                        eta_ms: Some(0),
                        throughput: None,
                    },
                )
                .ok();

            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: true,
                        message: "Memory optimization completed".to_string(),
                        duration,
                        canceled: Some(false),
                    },
                )
                .ok();
            state.ops.finish_success(&operation_id);
        }
        Err(err) => {
            let canceled = err.contains("cancelled");
            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: false,
                        message: format!("Memory optimization failed: {}", err),
                        duration,
                        canceled: Some(canceled),
                    },
                )
                .ok();
            if canceled {
                state.ops.finish_canceled(&operation_id);
            } else {
                state.ops.finish_failed(&operation_id, &err);
            }
        }
    }

    result
}

#[tauri::command]
async fn optimize_memory_admin(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<MemoryOptimizationResult, String> {
    let (operation_id, token) = state.ops.register(OperationKind::MemOptimizeAdmin, true);
    let start_time = std::time::Instant::now();

    // Emit start event
    app_handle
        .emit(
            "operation:start",
            OperationStartEvent {
                operation_id: operation_id.clone(),
                operation_type: "memory_optimization_admin".to_string(),
                estimated_duration: Some(5000),
            },
        )
        .ok();

    let optimizer = state.memory_optimizer.read().await;

    // Progress stages for admin optimization
    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 15.0,
                message: "Requesting administrator privileges...".to_string(),
                stage: "auth".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 35.0,
                message: "Purging disk caches...".to_string(),
                stage: "disk_cache".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 55.0,
                message: "Clearing DNS and network caches...".to_string(),
                stage: "network_cache".to_string(),
                can_cancel: true,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    app_handle
        .emit(
            "progress:update",
            ProgressEvent {
                operation_id: operation_id.clone(),
                progress: 75.0,
                message: "Optimizing memory compression...".to_string(),
                stage: "memory_compression".to_string(),
                can_cancel: false,
                eta_ms: None,
                throughput: None,
            },
        )
        .ok();

    // Perform the actual admin optimization
    let _permit = state.ops.opt_sem.acquire().await;
    let result = optimizer.optimize_memory_with_admin_cancel(&token).await;

    let duration = start_time.elapsed().as_millis() as u32;

    match &result {
        Ok(_) => {
            app_handle
                .emit(
                    "progress:update",
                    ProgressEvent {
                        operation_id: operation_id.clone(),
                        progress: 100.0,
                        message: "Deep clean optimization completed successfully".to_string(),
                        stage: "complete".to_string(),
                        can_cancel: false,
                        eta_ms: Some(0),
                        throughput: None,
                    },
                )
                .ok();

            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: true,
                        message: "Deep clean optimization completed".to_string(),
                        duration,
                        canceled: Some(false),
                    },
                )
                .ok();
            state.ops.finish_success(&operation_id);
        }
        Err(err) => {
            let canceled = err.contains("cancelled");
            app_handle
                .emit(
                    "operation:complete",
                    OperationCompleteEvent {
                        operation_id: operation_id.clone(),
                        success: false,
                        message: format!("Deep clean optimization failed: {}", err),
                        duration,
                        canceled: Some(canceled),
                    },
                )
                .ok();
            if canceled {
                state.ops.finish_canceled(&operation_id);
            } else {
                state.ops.finish_failed(&operation_id, &err);
            }
        }
    }

    result
}

#[tauri::command]
async fn clear_inactive_memory(state: State<'_, AppState>) -> Result<u64, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer.clear_inactive_memory().await
}

#[tauri::command]
async fn get_memory_pressure(state: State<'_, AppState>) -> Result<f32, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer.get_memory_pressure()
}

#[tauri::command]
async fn get_memory_stats(state: State<'_, AppState>) -> Result<MemoryStats, String> {
    // We don't need the optimizer instance, but lock to keep API consistent
    drop(state.memory_optimizer.read().await);
    MemoryOptimizer::get_memory_stats()
}

#[tauri::command]
async fn cancel_operation(state: State<'_, AppState>, operation_id: String) -> Result<(), String> {
    if state.ops.cancel(&operation_id) {
        Ok(())
    } else {
        Err("Unknown operation".into())
    }
}

#[tauri::command]
async fn get_operation_state(
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<Option<ops::OpState>, String> {
    Ok(state.ops.get(&operation_id))
}

#[tauri::command]
async fn get_network_info(
    state: State<'_, AppState>,
) -> Result<Vec<system_info::NetworkInfo>, String> {
    let monitor = state.system_monitor.read().await;
    Ok(monitor.get_network_info())
}

#[tauri::command]
async fn get_temperatures(
    state: State<'_, AppState>,
) -> Result<Vec<system_info::TemperatureInfo>, String> {
    let monitor = state.system_monitor.read().await;
    Ok(monitor.get_temperatures())
}

#[tauri::command]
async fn kill_memory_intensive_processes(
    state: State<'_, AppState>,
    threshold_mb: u64,
) -> Result<Vec<String>, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer
        .kill_memory_intensive_processes(threshold_mb)
        .await
}

#[tauri::command]
async fn optimize_swap(state: State<'_, AppState>) -> Result<String, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer.optimize_swap().await
}

// Batched dashboard endpoint for Phase 3
#[tauri::command]
async fn get_dashboard_data(state: State<'_, AppState>) -> Result<DashboardData, String> {
    // Execute all dashboard queries in parallel
    let (memory_info, cpu_info, disk_info, top_processes) = tokio::join!(
        async {
            let mut m = state.system_monitor.write().await;
            m.get_memory_info()
        },
        async {
            let mut m = state.system_monitor.write().await;
            m.get_cpu_info()
        },
        async {
            let m = state.system_monitor.write().await;
            m.get_disks()
        },
        async {
            let mut m = state.system_monitor.write().await;
            m.get_top_memory_processes(5)
        }
    );

    Ok(DashboardData {
        memory: memory_info,
        cpu: cpu_info,
        disks: disk_info,
        top_processes,
    })
}

#[derive(Clone, serde::Serialize)]
struct DashboardData {
    memory: MemoryInfo,
    cpu: CpuInfo,
    disks: Vec<DiskInfo>,
    top_processes: Vec<ProcessInfo>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        system_monitor: RwLock::new(SystemMonitor::new()),
        file_cleaner: RwLock::new(FileCleaner::new()),
        enhanced_file_cleaner: RwLock::new(EnhancedFileCleaner::new()),
        memory_optimizer: RwLock::new(MemoryOptimizer::new()),
        ops: OperationRegistry::new(1, 2, 1),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(|app| {
            // Ensure main window is visible and focused before heavy rendering starts.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            get_memory_info,
            get_cpu_info,
            get_processes,
            get_top_memory_processes,
            get_disks,
            kill_process,
            scan_cleanable_files,
            scan_cleanable_files_enhanced,
            get_cleanable_files,
            get_auto_selectable_files,
            get_files_by_safety,
            clean_files,
            clean_files_enhanced,
            prepare_deletion_enhanced,
            preview_rules,
            get_enhanced_telemetry,
            record_user_feedback,
            get_active_development_tools,
            empty_trash,
            restore_from_trash,
            optimize_memory,
            optimize_memory_admin,
            clear_inactive_memory,
            get_memory_pressure,
            get_memory_stats,
            get_network_info,
            get_temperatures,
            kill_memory_intensive_processes,
            optimize_swap,
            get_dashboard_data,
            cancel_operation,
            get_operation_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
