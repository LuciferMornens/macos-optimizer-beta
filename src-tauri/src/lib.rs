mod system_info;
mod file_cleaner;
mod memory_optimizer;

use system_info::{SystemMonitor, SystemInfo, MemoryInfo, ProcessInfo, DiskInfo, CpuInfo};
use file_cleaner::{FileCleaner, CleanableFile, CleaningReport};
use memory_optimizer::{MemoryOptimizer, MemoryOptimizationResult, MemoryStats};

use tauri::{Manager, State};
use tokio::sync::RwLock;

// Create a state to manage our system monitor
struct AppState {
    system_monitor: RwLock<SystemMonitor>,
    file_cleaner: RwLock<FileCleaner>,
    memory_optimizer: RwLock<MemoryOptimizer>,
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
async fn get_top_memory_processes(state: State<'_, AppState>, limit: usize) -> Result<Vec<ProcessInfo>, String> {
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
async fn scan_cleanable_files(state: State<'_, AppState>) -> Result<CleaningReport, String> {
    let mut cleaner = state.file_cleaner.write().await;
    cleaner.scan_system().await
}

#[tauri::command]
async fn get_cleanable_files(state: State<'_, AppState>) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.read().await;
    Ok(cleaner.get_cleanable_files().clone())
}

#[tauri::command]
async fn get_auto_selectable_files(state: State<'_, AppState>) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.read().await;
    Ok(cleaner.get_auto_selectable_files())
}

#[tauri::command]
async fn get_files_by_safety(state: State<'_, AppState>, min_safety_score: u8) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.read().await;
    Ok(cleaner.get_files_by_safety(min_safety_score))
}

#[tauri::command]
async fn clean_files(state: State<'_, AppState>, file_paths: Vec<String>) -> Result<(u64, usize), String> {
    let cleaner = state.file_cleaner.read().await;
    cleaner.clean_files(file_paths).await
}

#[tauri::command]
async fn empty_trash(state: State<'_, AppState>) -> Result<(u64, usize), String> {
    let cleaner = state.file_cleaner.read().await;
    cleaner.empty_trash().await
}

#[tauri::command]
async fn optimize_memory(state: State<'_, AppState>) -> Result<MemoryOptimizationResult, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer.optimize_memory().await
}

#[tauri::command]
async fn optimize_memory_admin(state: State<'_, AppState>) -> Result<MemoryOptimizationResult, String> {
    let optimizer = state.memory_optimizer.read().await;
    // Use GUI authentication for admin operations
    optimizer.optimize_memory_with_admin(true).await
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
async fn get_network_info(state: State<'_, AppState>) -> Result<Vec<system_info::NetworkInfo>, String> {
    let monitor = state.system_monitor.read().await;
    Ok(monitor.get_network_info())
}

#[tauri::command]
async fn get_temperatures(state: State<'_, AppState>) -> Result<Vec<system_info::TemperatureInfo>, String> {
    let monitor = state.system_monitor.read().await;
    Ok(monitor.get_temperatures())
}

#[tauri::command]
async fn kill_memory_intensive_processes(state: State<'_, AppState>, threshold_mb: u64) -> Result<Vec<String>, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer.kill_memory_intensive_processes(threshold_mb).await
}

#[tauri::command]
async fn optimize_swap(state: State<'_, AppState>) -> Result<String, String> {
    let optimizer = state.memory_optimizer.read().await;
    optimizer.optimize_swap().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        system_monitor: RwLock::new(SystemMonitor::new()),
        file_cleaner: RwLock::new(FileCleaner::new()),
        memory_optimizer: RwLock::new(MemoryOptimizer::new()),
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
            get_cleanable_files,
            get_auto_selectable_files,
            get_files_by_safety,
            clean_files,
            empty_trash,
            optimize_memory,
            optimize_memory_admin,
            clear_inactive_memory,
            get_memory_pressure,
            get_memory_stats,
            get_network_info,
            get_temperatures,
            kill_memory_intensive_processes,
               optimize_swap
         ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
