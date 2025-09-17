use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tokio::process::Command;
use tokio::time::{timeout, Duration};

use dirs;

use super::types::CleanableFile;

pub struct DependencyChecker;

impl DependencyChecker {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn verify_no_dependencies(
        &self,
        files: &[CleanableFile],
    ) -> HashMap<PathBuf, Vec<PathBuf>> {
        let mut dependencies = HashMap::new();

        for file in files {
            let path = PathBuf::from(&file.path);
            let deps = self.find_dependencies(&path).await;
            if !deps.is_empty() {
                dependencies.insert(path, deps);
            }
        }

        dependencies
    }

    async fn find_dependencies(&self, path: &Path) -> Vec<PathBuf> {
        let mut deps: HashSet<PathBuf> = HashSet::new();
        let path_string = path.to_string_lossy().to_string();

        // Targeted symlink search with timeouts to avoid full-disk scans
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(parent) = path.parent() {
            roots.push(parent.into());
        }
        roots.push(PathBuf::from("/Applications"));
        roots.push(PathBuf::from("/Library"));
        roots.push(PathBuf::from("/System/Library"));
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join("Applications"));
            roots.push(home.join("Library"));
        }

        for root in roots.into_iter() {
            if !root.exists() {
                continue;
            }
            let mut command = Command::new("find");
            command
                .arg(&root)
                .arg("-maxdepth")
                .arg("6")
                .arg("-type")
                .arg("l")
                .arg("-lname")
                .arg(&path_string)
                .arg("-print")
                .arg("-quit");
            command.kill_on_drop(true);
            match timeout(Duration::from_secs(3), command.output()).await {
                Ok(Ok(output)) if output.status.success() => {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            deps.insert(PathBuf::from(trimmed));
                        }
                    }
                }
                Ok(Err(err)) => {
                    log::warn!("find command failed for {}: {}", path.display(), err);
                }
                Err(_) => {
                    log::warn!(
                        "find command timed out while checking dependencies for {}",
                        path.display()
                    );
                }
                _ => {}
            }
        }

        // Scan common launchd locations for references, capped for speed
        let mut grep = Command::new("grep");
        grep.arg("-R")
            .arg("-l")
            .arg("-m")
            .arg("10")
            .arg(&path_string);
        grep.arg("/Library/LaunchAgents");
        grep.arg("/Library/LaunchDaemons");
        if let Some(home) = dirs::home_dir() {
            grep.arg(home.join("Library/LaunchAgents"));
        }
        grep.kill_on_drop(true);
        if let Ok(result) = timeout(Duration::from_secs(3), grep.output()).await {
            match result {
                Ok(output) if output.status.success() => {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        deps.insert(PathBuf::from(trimmed));
                    }
                }
                Ok(output) => {
                    if !output.stderr.is_empty() {
                        log::debug!(
                            "grep returned status {} while scanning launchd references for {}: {}",
                            output.status,
                            path.display(),
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
                Err(err) => {
                    log::warn!("grep command failed for {}: {}", path.display(), err);
                }
            }
        } else {
            log::warn!(
                "grep command timed out while scanning launchd references for {}",
                path.display()
            );
        }

        deps.into_iter().collect()
    }
}
