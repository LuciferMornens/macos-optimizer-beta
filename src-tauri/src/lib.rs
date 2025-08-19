mod system_info;
mod file_cleaner;
mod memory_optimizer;

use system_info::{SystemMonitor, SystemInfo, MemoryInfo, ProcessInfo, DiskInfo, CpuInfo};
use file_cleaner::{FileCleaner, CleanableFile, CleaningReport};
use memory_optimizer::{MemoryOptimizer, MemoryOptimizationResult};
use std::sync::Mutex;
use tauri::State;

// Create a state to manage our system monitor
struct AppState {
    system_monitor: Mutex<SystemMonitor>,
    file_cleaner: Mutex<FileCleaner>,
    memory_optimizer: Mutex<MemoryOptimizer>,
}

#[tauri::command]
fn get_system_info(state: State<AppState>) -> Result<SystemInfo, String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.refresh();
    Ok(monitor.get_system_info())
}

#[tauri::command]
fn get_memory_info(state: State<AppState>) -> Result<MemoryInfo, String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.refresh();
    Ok(monitor.get_memory_info())
}

#[tauri::command]
fn get_cpu_info(state: State<AppState>) -> Result<CpuInfo, String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.refresh();
    Ok(monitor.get_cpu_info())
}

#[tauri::command]
fn get_processes(state: State<AppState>) -> Result<Vec<ProcessInfo>, String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.refresh();
    Ok(monitor.get_processes())
}

#[tauri::command]
fn get_top_memory_processes(state: State<AppState>, limit: usize) -> Result<Vec<ProcessInfo>, String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.refresh();
    Ok(monitor.get_top_memory_processes(limit))
}

#[tauri::command]
fn get_disks(state: State<AppState>) -> Result<Vec<DiskInfo>, String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.refresh();
    Ok(monitor.get_disks())
}

#[tauri::command]
fn kill_process(state: State<AppState>, pid: u32) -> Result<(), String> {
    let mut monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    monitor.kill_process(pid)
}

#[tauri::command]
fn scan_cleanable_files(state: State<AppState>) -> Result<CleaningReport, String> {
    let mut cleaner = state.file_cleaner.lock().map_err(|e| e.to_string())?;
    cleaner.scan_system()
}

#[tauri::command]
fn get_cleanable_files(state: State<AppState>) -> Result<Vec<CleanableFile>, String> {
    let cleaner = state.file_cleaner.lock().map_err(|e| e.to_string())?;
    Ok(cleaner.get_cleanable_files().clone())
}

#[tauri::command]
fn clean_files(state: State<AppState>, file_paths: Vec<String>) -> Result<(u64, usize), String> {
    let cleaner = state.file_cleaner.lock().map_err(|e| e.to_string())?;
    cleaner.clean_files(file_paths)
}

#[tauri::command]
fn empty_trash(state: State<AppState>) -> Result<(u64, usize), String> {
    let cleaner = state.file_cleaner.lock().map_err(|e| e.to_string())?;
    cleaner.empty_trash()
}

#[tauri::command]
fn optimize_memory(state: State<AppState>) -> Result<MemoryOptimizationResult, String> {
    let optimizer = state.memory_optimizer.lock().map_err(|e| e.to_string())?;
    optimizer.optimize_memory()
}

#[tauri::command]
fn optimize_memory_admin(state: State<AppState>) -> Result<MemoryOptimizationResult, String> {
    let optimizer = state.memory_optimizer.lock().map_err(|e| e.to_string())?;
    // Use GUI authentication for admin operations
    optimizer.optimize_memory_with_admin(true)
}

#[tauri::command]
fn clear_inactive_memory(state: State<AppState>) -> Result<u64, String> {
    let optimizer = state.memory_optimizer.lock().map_err(|e| e.to_string())?;
    optimizer.clear_inactive_memory()
}

#[tauri::command]
fn get_memory_pressure(state: State<AppState>) -> Result<f32, String> {
    let optimizer = state.memory_optimizer.lock().map_err(|e| e.to_string())?;
    optimizer.get_memory_pressure()
}

#[tauri::command]
fn get_network_info(state: State<AppState>) -> Result<Vec<system_info::NetworkInfo>, String> {
    let monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    Ok(monitor.get_network_info())
}

#[tauri::command]
fn get_temperatures(state: State<AppState>) -> Result<Vec<system_info::TemperatureInfo>, String> {
    let monitor = state.system_monitor.lock().map_err(|e| e.to_string())?;
    Ok(monitor.get_temperatures())
}

#[tauri::command]
fn kill_memory_intensive_processes(state: State<AppState>, threshold_mb: u64) -> Result<Vec<String>, String> {
    let optimizer = state.memory_optimizer.lock().map_err(|e| e.to_string())?;
    optimizer.kill_memory_intensive_processes(threshold_mb)
}

#[tauri::command]
fn optimize_swap(state: State<AppState>) -> Result<String, String> {
    let optimizer = state.memory_optimizer.lock().map_err(|e| e.to_string())?;
    optimizer.optimize_swap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        system_monitor: Mutex::new(SystemMonitor::new()),
        file_cleaner: Mutex::new(FileCleaner::new()),
        memory_optimizer: Mutex::new(MemoryOptimizer::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
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
            clean_files,
            empty_trash,
            optimize_memory,
            optimize_memory_admin,
            clear_inactive_memory,
            get_memory_pressure,
            get_network_info,
            get_temperatures,
            kill_memory_intensive_processes,
            optimize_swap
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
