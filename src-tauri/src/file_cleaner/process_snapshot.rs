use std::collections::HashSet;
use std::sync::Arc;

use log::debug;
use sysinfo::System;
use tokio::task;

#[derive(Clone, Default)]
pub struct ProcessSnapshot {
    process_names: Arc<HashSet<String>>,
    command_paths: Arc<HashSet<String>>,
}

impl ProcessSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn has_process_named(&self, name: &str) -> bool {
        let target = name.to_lowercase();
        self.process_names.contains(&target)
    }

    pub fn command_contains_path(&self, path: &str) -> bool {
        if self.command_paths.is_empty() {
            return false;
        }
        let needle = path.to_lowercase();
        self.command_paths.iter().any(|cmd| cmd.contains(&needle))
    }

    pub async fn capture() -> Self {
        match task::spawn_blocking(capture_snapshot).await {
            Ok(snapshot) => snapshot,
            Err(join_err) => {
                debug!("Failed to capture process snapshot: {}", join_err);
                Self::empty()
            }
        }
    }
}

fn capture_snapshot() -> ProcessSnapshot {
    let mut system = System::new();
    system.refresh_processes();

    let mut names = HashSet::new();
    let mut command_paths = HashSet::new();

    for process in system.processes().values() {
        let name = process.name().to_lowercase();
        names.insert(name);

        for arg in process.cmd() {
            if arg.contains('/') {
                command_paths.insert(arg.to_lowercase());
            }
        }
    }

    ProcessSnapshot {
        process_names: Arc::new(names),
        command_paths: Arc::new(command_paths),
    }
}
