use libc::{kill as libc_kill, SIGKILL, SIGTERM};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use sysinfo::{Components, Networks, Pid, System};

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub hostname: String,
    pub uptime: u64,
    pub boot_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_memory: u64,
    pub used_memory: u64,
    pub available_memory: u64,
    pub free_memory: u64,
    pub total_swap: u64,
    pub used_swap: u64,
    pub free_swap: u64,
    pub memory_pressure: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub virtual_memory: u64,
    pub status: String,
    pub parent_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub used_space: u64,
    pub file_system: String,
    pub is_removable: bool,
    pub device: String,
    pub kind: String,
    pub is_system: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub interface_name: String,
    pub mac_address: String,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
    pub received_packets: u64,
    pub transmitted_packets: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub brand: String,
    pub frequency: u64,
    pub cpu_usage: f32,
    pub core_count: usize,
    pub physical_core_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TemperatureInfo {
    pub label: String,
    pub current: f32,
    pub high: f32,
    pub critical: f32,
}

pub struct SystemMonitor {
    system: System,
    last_full_refresh: Instant,
    last_process_refresh: Instant,
    refresh_interval: Duration,
}

enum RefreshComponent {
    Processes,
    All,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        SystemMonitor {
            system,
            last_full_refresh: Instant::now(),
            last_process_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(5),
        }
    }

    pub fn refresh(&mut self) {
        self.refresh_selective(RefreshComponent::All);
    }

    fn refresh_selective(&mut self, component: RefreshComponent) {
        let now = Instant::now();

        match component {
            RefreshComponent::Processes => {
                if now.duration_since(self.last_process_refresh) > Duration::from_secs(1) {
                    self.system.refresh_processes();
                    self.last_process_refresh = now;
                }
            }
            RefreshComponent::All => {
                if now.duration_since(self.last_full_refresh) > self.refresh_interval {
                    self.system.refresh_all();
                    self.last_full_refresh = now;
                    self.last_process_refresh = now;
                }
            }
        }
    }

    pub fn get_system_info(&self) -> SystemInfo {
        SystemInfo {
            os_name: System::long_os_version().unwrap_or_default(),
            os_version: System::os_version().unwrap_or_default(),
            kernel_version: System::kernel_version().unwrap_or_default(),
            hostname: System::host_name().unwrap_or_default(),
            uptime: System::uptime(),
            boot_time: System::boot_time(),
        }
    }

    pub fn get_processes(&mut self) -> Vec<ProcessInfo> {
        self.refresh_selective(RefreshComponent::Processes);
        self.system
            .processes()
            .iter()
            .map(|(pid, process)| ProcessInfo {
                pid: pid.as_u32(),
                name: process.name().to_string(),
                cpu_usage: process.cpu_usage(),
                memory_usage: process.memory(),
                virtual_memory: process.virtual_memory(),
                status: format!("{:?}", process.status()),
                parent_pid: process.parent().map(|p| p.as_u32()),
            })
            .collect()
    }

    pub fn get_top_memory_processes(&mut self, limit: usize) -> Vec<ProcessInfo> {
        self.refresh_selective(RefreshComponent::Processes);
        let mut processes = self.get_processes();
        processes.sort_by(|a, b| b.memory_usage.cmp(&a.memory_usage));
        processes.truncate(limit);
        processes
    }

    pub fn get_network_info(&self) -> Vec<NetworkInfo> {
        let networks = Networks::new_with_refreshed_list();
        networks
            .iter()
            .map(|(interface_name, data)| NetworkInfo {
                interface_name: interface_name.clone(),
                mac_address: data.mac_address().to_string(),
                received_bytes: data.total_received(),
                transmitted_bytes: data.total_transmitted(),
                received_packets: data.total_packets_received(),
                transmitted_packets: data.total_packets_transmitted(),
            })
            .collect()
    }

    pub fn get_temperatures(&self) -> Vec<TemperatureInfo> {
        let components = Components::new_with_refreshed_list();
        components
            .iter()
            .map(|component| TemperatureInfo {
                label: component.label().to_string(),
                current: component.temperature(),
                high: component.max(),
                critical: component.critical().unwrap_or(100.0),
            })
            .collect()
    }

    pub fn kill_process(&mut self, pid: u32) -> Result<(), String> {
        // Try a graceful termination first
        let term_res = unsafe { libc_kill(pid as i32, SIGTERM) };
        if term_res == 0 {
            return Ok(());
        }

        // If the process no longer exists, consider it terminated
        if self.system.process(Pid::from_u32(pid)).is_none() {
            return Ok(());
        }

        // Escalate to force kill
        let kill_res = unsafe { libc_kill(pid as i32, SIGKILL) };
        if kill_res == 0 {
            Ok(())
        } else {
            Err(format!(
                "Failed to kill process {}: {}",
                pid,
                std::io::Error::last_os_error()
            ))
        }
    }
}
