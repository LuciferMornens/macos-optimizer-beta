#[cfg(feature = "parallel-scan")]
use dashmap::DashMap;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
#[cfg(feature = "parallel-scan")]
use std::sync::Arc;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

use chrono::Local;
#[cfg(not(feature = "parallel-scan"))]
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use dirs;
use walkdir::WalkDir;

#[cfg(not(feature = "parallel-scan"))]
use super::safety::{assess_path_risk, calculate_safety_score, RiskLevel};
// Light build: metrics disabled to avoid unused code warnings.
use super::cache::DIR_SIZE_CACHE;
#[cfg(feature = "metadata-cache")]
use super::cache::FILE_METADATA_CACHE;
use super::types::{
    load_rules, load_rules_result, CategoryReport, CategoryRule, CleanableFile, CleanerRules,
    CleaningReport,
};
use tokio_util::sync::CancellationToken;

/// macOS file cleaner with conservative safety heuristics + user override.
pub struct FileCleaner {
    cleanable_files: Vec<CleanableFile>,
    /// Case-insensitive set of paths we've already included
    seen_paths: HashSet<String>,
    /// If a directory is added, we store its lowercased prefix (ending with '/')
    /// and skip any children to avoid double-counting.
    seen_dir_prefixes: Vec<String>,
}

impl FileCleaner {
    pub fn new() -> Self {
        FileCleaner {
            cleanable_files: Vec::new(),
            seen_paths: HashSet::new(),
            seen_dir_prefixes: Vec::new(),
        }
    }

    // Standalone scan methods removed; use `scan_system_with_cancel` which
    // supports both parallel and serial paths with cancellation.

    /// Cancellable scan wrapper
    pub async fn scan_system_with_cancel(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<CleaningReport, String> {
        #[cfg(feature = "parallel-scan")]
        {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            // Run parallel scan but guard inner workers to early-out
            self.cleanable_files.clear();
            self.seen_paths.clear();
            self.seen_dir_prefixes.clear();

            let rules: CleanerRules = load_rules_result()?;

            let found_files = Arc::new(DashMap::new());
            let seen_paths = Arc::new(DashMap::new());
            let seen_dir_prefixes = Arc::new(DashMap::new());

            let categories: Vec<_> = rules.categories.into_iter().collect();
            let (user_rules, system_rules): (Vec<_>, Vec<_>) =
                categories.into_iter().partition(|r| {
                    !r.paths
                        .iter()
                        .any(|p| p.starts_with("/System") || p.starts_with("/Library"))
                });

            let found_files_clone = found_files.clone();
            let seen_paths_clone = seen_paths.clone();
            let seen_dir_prefixes_clone = seen_dir_prefixes.clone();
            let token = cancel.clone();
            user_rules.par_iter().for_each(|rule| {
                if token.is_cancelled() {
                    return;
                }
                let paths: Vec<_> = rule
                    .paths
                    .iter()
                    .filter_map(|p| Self::expand_path(p))
                    .filter(|path| path.exists())
                    .collect();
                paths.par_iter().for_each(|path| {
                    if token.is_cancelled() {
                        return;
                    }
                    let _ = self.scan_path_parallel_with_cancel(
                        path,
                        rule,
                        found_files_clone.clone(),
                        seen_paths_clone.clone(),
                        seen_dir_prefixes_clone.clone(),
                        &token,
                    );
                });
            });

            let found_files_clone = found_files.clone();
            let seen_paths_clone = seen_paths.clone();
            let seen_dir_prefixes_clone = seen_dir_prefixes.clone();
            let token = cancel.clone();
            system_rules.par_iter().for_each(|rule| {
                if token.is_cancelled() {
                    return;
                }
                let paths: Vec<_> = rule
                    .paths
                    .iter()
                    .filter_map(|p| Self::expand_path(p))
                    .filter(|path| path.exists())
                    .collect();
                for path in paths {
                    if token.is_cancelled() {
                        break;
                    }
                    let _ = self.scan_path_parallel_with_cancel(
                        &path,
                        rule,
                        found_files_clone.clone(),
                        seen_paths_clone.clone(),
                        seen_dir_prefixes_clone.clone(),
                        &token,
                    );
                }
            });

            self.cleanable_files = Self::prune_parallel_results(&found_files, &seen_dir_prefixes);
            self.seen_paths = self
                .cleanable_files
                .iter()
                .map(|file| file.path.to_lowercase())
                .collect();
            self.seen_dir_prefixes = seen_dir_prefixes
                .iter()
                .map(|prefix| prefix.key().clone())
                .collect();
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            Ok(self.generate_report())
        }

        #[cfg(not(feature = "parallel-scan"))]
        {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            self.cleanable_files.clear();
            self.seen_paths.clear();
            self.seen_dir_prefixes.clear();

            let rules: CleanerRules = load_rules_result()?;
            for rule in rules.categories.iter() {
                if cancel.is_cancelled() {
                    return Err("cancelled".into());
                }
                let paths_to_scan: Vec<_> = rule
                    .paths
                    .iter()
                    .filter_map(|p| Self::expand_path(p))
                    .filter(|path| path.exists())
                    .collect();
                if !paths_to_scan.is_empty() {
                    for path in paths_to_scan {
                        if cancel.is_cancelled() {
                            return Err("cancelled".into());
                        }
                        if let Err(_) = self.scan_path_with_rule(&path, rule).await {
                            continue;
                        }
                    }
                    tokio::task::yield_now().await;
                }
            }
            Ok(self.generate_report())
        }
    }

    #[cfg(not(feature = "parallel-scan"))]
    async fn scan_path_with_rule(
        &mut self,
        path: &Path,
        rule: &CategoryRule,
    ) -> Result<(), String> {
        let mut found_files = Vec::new();
        let mut local_seen_paths = HashSet::new();
        let mut local_seen_dirs = Vec::new();

        self.scan_path_internal(
            path,
            rule,
            &mut found_files,
            &mut local_seen_paths,
            &mut local_seen_dirs,
        )?;

        // Merge results
        for file in found_files {
            self.cleanable_files.push(file);
        }
        for path in local_seen_paths {
            self.seen_paths.insert(path);
        }
        for dir in local_seen_dirs {
            self.seen_dir_prefixes.push(dir);
        }

        Ok(())
    }

    #[cfg(feature = "parallel-scan")]
    pub(crate) fn collect_rule_matches_for_path(
        &self,
        path: &Path,
        rule: &CategoryRule,
        cancel: &CancellationToken,
    ) -> Result<Vec<CleanableFile>, String> {
        let found_files = Arc::new(DashMap::new());
        let seen_paths = Arc::new(DashMap::new());
        let seen_dir_prefixes = Arc::new(DashMap::new());
        self.scan_path_parallel_with_cancel(
            path,
            rule,
            found_files.clone(),
            seen_paths,
            seen_dir_prefixes.clone(),
            cancel,
        )?;
        let results = Self::prune_parallel_results(&found_files, &seen_dir_prefixes);
        Ok(results)
    }

    #[cfg(not(feature = "parallel-scan"))]
    pub(crate) fn collect_rule_matches_for_path(
        &self,
        path: &Path,
        rule: &CategoryRule,
        _cancel: &CancellationToken,
    ) -> Result<Vec<CleanableFile>, String> {
        let mut found_files = Vec::new();
        let mut local_seen_paths = HashSet::new();
        let mut local_seen_dirs = Vec::new();

        self.scan_path_internal(
            path,
            rule,
            &mut found_files,
            &mut local_seen_paths,
            &mut local_seen_dirs,
        )?;

        Ok(found_files)
    }

    // The remaining parallel implementation is the cancellable variant below.

    #[cfg(feature = "parallel-scan")]
    fn scan_path_parallel_with_cancel(
        &self,
        path: &Path,
        rule: &CategoryRule,
        found_files: Arc<DashMap<String, CleanableFile>>,
        seen_paths: Arc<DashMap<String, bool>>,
        seen_dir_prefixes: Arc<DashMap<String, bool>>,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }
        let token = cancel.clone();
        WalkDir::new(path)
            .max_depth(rule.max_depth.unwrap_or(10))
            .into_iter()
            .par_bridge()
            .filter_map(|e| e.ok())
            .for_each(|entry| {
                if token.is_cancelled() {
                    return;
                }
                let file_path = entry.path();
                let path_str = file_path.to_string_lossy().to_string();
                let path_lower = path_str.to_lowercase();
                if seen_paths.contains_key(&path_lower) {
                    return;
                }
                if Self::has_seen_parent_prefix(&path_lower, &seen_dir_prefixes) {
                    return;
                }
                if let Some(cleanable) = self.process_entry(&entry, rule) {
                    if entry.file_type().is_dir() {
                        let mut dir_prefix = path_lower.clone();
                        if !dir_prefix.ends_with('/') {
                            dir_prefix.push('/');
                        }
                        seen_dir_prefixes.insert(dir_prefix, true);
                    }
                    found_files.insert(path_str.clone(), cleanable);
                    seen_paths.insert(path_lower, true);
                }
            });
        Ok(())
    }

    #[cfg(feature = "parallel-scan")]
    fn has_seen_parent_prefix(path_lower: &str, seen_dir_prefixes: &DashMap<String, bool>) -> bool {
        seen_dir_prefixes
            .iter()
            .any(|prefix| path_lower.starts_with(prefix.key()))
    }

    #[cfg(feature = "parallel-scan")]
    fn prune_parallel_results(
        found_files: &DashMap<String, CleanableFile>,
        seen_dir_prefixes: &DashMap<String, bool>,
    ) -> Vec<CleanableFile> {
        found_files
            .iter()
            .filter_map(|entry| {
                let path_lower = entry.key().to_lowercase();
                if Self::has_seen_parent_prefix(&path_lower, seen_dir_prefixes) {
                    return None;
                }
                Some(entry.value().clone())
            })
            .collect()
    }

    #[cfg(not(feature = "parallel-scan"))]
    fn scan_path_internal(
        &self,
        path: &Path,
        rule: &CategoryRule,
        found_files: &mut Vec<CleanableFile>,
        local_seen_paths: &mut HashSet<String>,
        local_seen_dirs: &mut Vec<String>,
    ) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }

        let now = Utc::now();
        let min_age = rule.min_age_days;
        let min_size_bytes_from_rule = rule.min_size_kb.map(|kb| kb * 1024);

        let excludes = rule
            .excludes
            .as_ref()
            .map(|v| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>())
            .unwrap_or_default();
        let exts = rule
            .extensions
            .as_ref()
            .map(|v| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>());
        let require_subpaths = rule
            .require_subpaths
            .as_ref()
            .map(|v| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>())
            .unwrap_or_default();

        // Build the walker - limit depth for better performance
        let walker = WalkDir::new(path);
        let iter = if let Some(d) = rule.max_depth {
            walker.max_depth(d).into_iter()
        } else {
            walker.max_depth(10).into_iter() // Default max depth for performance
        };

        // Process entries in parallel batches for better performance
        let entries: Vec<_> = iter.filter_map(|e| e.ok()).collect();

        // Use chunks to process in batches
        for chunk in entries.chunks(100) {
            for entry in chunk {
                let file_type = entry.file_type();
                let file_path = entry.path();

                // Skip symlinks to avoid unintended deletions
                if let Ok(md) = fs::symlink_metadata(file_path) {
                    if md.file_type().is_symlink() {
                        continue;
                    }
                }

                // Determine whether this is a file or directory match
                let is_dir_based_rule = rule.name.to_lowercase().contains("folder")
                    || rule.name.to_lowercase().contains("cache")
                    || rule.name.to_lowercase().contains("container");

                let consider_as_result = if is_dir_based_rule && file_type.is_dir() {
                    true
                } else if !is_dir_based_rule && file_type.is_file() {
                    true
                } else {
                    false
                };

                if !consider_as_result {
                    continue;
                }

                let path_str = file_path.to_string_lossy();
                let key = path_str.to_string();
                let path_lower = path_str.to_lowercase();

                // Skip if already seen
                if self.seen_paths.contains(&path_lower) || local_seen_paths.contains(&path_lower) {
                    continue;
                }

                // Check if this path is a child of any seen directory prefix
                if self
                    .seen_dir_prefixes
                    .iter()
                    .any(|prefix| path_lower.starts_with(prefix))
                    || local_seen_dirs
                        .iter()
                        .any(|prefix| path_lower.starts_with(prefix))
                {
                    continue;
                }

                // Apply exclude filters
                if excludes.iter().any(|ex| path_lower.contains(ex)) {
                    continue;
                }

                // Apply subpath requirements
                if !require_subpaths.is_empty() {
                    if !require_subpaths.iter().any(|req| path_lower.contains(req)) {
                        continue;
                    }
                }

                // Metadata checks
                let metadata = match fs::metadata(file_path) {
                    Ok(m) => m,
                    Err(e) => {
                        if e.kind() == ErrorKind::NotFound {
                            continue;
                        }
                        continue;
                    }
                };

                if file_type.is_dir() {
                    // Directory processing
                    let mut dir_prefix = path_lower.clone();
                    if !dir_prefix.ends_with('/') {
                        dir_prefix.push('/');
                    }
                    local_seen_dirs.push(dir_prefix);

                    // Age filter
                    if let Some(days) = min_age {
                        if let Ok(m) = metadata.modified() {
                            if now.signed_duration_since(DateTime::<Utc>::from(m))
                                < ChronoDuration::days(days)
                            {
                                continue;
                            }
                        }
                    }

                    let dir_size = self.get_directory_size_blocking(file_path).unwrap_or(0);
                    let min_size = min_size_bytes_from_rule.unwrap_or(0);
                    if dir_size < min_size {
                        continue;
                    }

                    let risk = assess_path_risk(file_path);
                    let path_is_safe = matches!(risk.level, RiskLevel::Safe);
                    let is_safe = rule.safe && path_is_safe;
                    let (safety_score, mut auto_select) =
                        calculate_safety_score(file_path, &rule.name, &risk, rule.min_age_days);
                    auto_select = auto_select && rule.safe && path_is_safe;

                    let last_modified = metadata
                        .modified()
                        .map(|t| DateTime::<Utc>::from(t).timestamp())
                        .unwrap_or(0);

                    let cleanable = CleanableFile {
                        path: key.clone(),
                        size: dir_size,
                        category: rule.name.clone(),
                        description: self.get_file_description(file_path, &rule.name),
                        last_modified,
                        safe_to_delete: is_safe,
                        safety_score,
                        auto_select,
                    };

                    found_files.push(cleanable);
                    local_seen_paths.insert(path_lower);
                } else {
                    // File processing
                    if let Some(ref allowed_exts) = exts {
                        if let Some(ext) = file_path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_lowercase())
                        {
                            if !allowed_exts.iter().any(|e| e == &ext) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    // Age filter
                    if let Some(days) = min_age {
                        let relevant_time = (|| {
                            let name_lower = rule.name.to_lowercase();
                            if name_lower.contains("downloads") || name_lower.contains("desktop") {
                                if let Ok(created) = metadata.created() {
                                    return Some(DateTime::<Utc>::from(created));
                                }
                            }
                            metadata.modified().ok().map(|t| DateTime::<Utc>::from(t))
                        })();

                        if let Some(file_time) = relevant_time {
                            if now.signed_duration_since(file_time) < ChronoDuration::days(days) {
                                continue;
                            }
                        }
                    }

                    // Size filter
                    let file_size = metadata.len();
                    let min_size = min_size_bytes_from_rule.unwrap_or(0);
                    if file_size < min_size {
                        continue;
                    }

                    let last_modified = metadata
                        .modified()
                        .map(|t| DateTime::<Utc>::from(t).timestamp())
                        .unwrap_or(0);

                    let risk = assess_path_risk(file_path);
                    let path_is_safe = matches!(risk.level, RiskLevel::Safe);
                    let is_safe = rule.safe && path_is_safe;
                    let (safety_score, mut auto_select) =
                        calculate_safety_score(file_path, &rule.name, &risk, rule.min_age_days);
                    auto_select = auto_select && rule.safe && path_is_safe;

                    let cleanable = CleanableFile {
                        path: key.clone(),
                        size: file_size,
                        category: rule.name.clone(),
                        description: self.get_file_description(file_path, &rule.name),
                        last_modified,
                        safe_to_delete: is_safe,
                        safety_score,
                        auto_select,
                    };

                    found_files.push(cleanable);
                    local_seen_paths.insert(path_lower);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn get_file_description(&self, path: &Path, category: &str) -> String {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");

        match category {
            "System Cache" | "System Cache (Advanced)" => format!("System cache: {}", filename),
            "User Cache" => format!("Cache file: {}", filename),
            "Browser Cache" => format!("Browser cache: {}", filename),
            "App Store Cache" => format!("App Store cache: {}", filename),
            "Music Cache" => format!("Music cache: {}", filename),
            "Trash" => format!("Trashed item: {}", filename),
            "Incomplete Downloads (2d+)" => format!("Incomplete download: {}", filename),
            "Saved Application State (30d+)" => format!("Saved state: {}", filename),
            "Container Caches (Advanced)" => format!("Container cache: {}", filename),
            "Container Temp (Advanced)" => format!("Container tmp: {}", filename),
            "Group Container Caches (Advanced)" => format!("Group container cache: {}", filename),
            "App Support Caches (Advanced)" => format!("App support cache: {}", filename),
            "Dropbox Cache" => format!("Dropbox cache: {}", filename),
            "Old Downloads" | "Old Downloads (90d+)" => format!("Old download: {}", filename),
            "Large Stale Files (Desktop/Downloads)" => format!("Large stale item: {}", filename),
            "Temporary Files" => format!("Temporary item: {}", filename),
            "User Temporary Files" => format!("Temporary item: {}", filename),
            "QuickLook Cache" => format!("QuickLook thumbnail: {}", filename),
            "User Logs (30d+)" => format!("Old log: {}", filename),
            "System Logs (30d+, Advanced)" => format!("System log: {}", filename),
            "Crash Reports (30d+)" => format!("Crash report: {}", filename),
            "System Crash Reports (30d+, Advanced)" => format!("System crash report: {}", filename),
            "Mail Downloads (Review)" => format!("Mail attachment: {}", filename),
            "Old Installers (30d+)" => format!("Old installer: {}", filename),
            "Messages Attachments (90d+, Review)" => format!("Messages attachment: {}", filename),
            "iOS Backups (Advanced)" => format!("iOS backup: {}", filename),
            "iOS Updates (Advanced)" => format!("iOS update file: {}", filename),
            _ => format!("{}: {}", category, filename),
        }
    }

    fn generate_report(&self) -> CleaningReport {
        let mut categories: std::collections::HashMap<String, (u64, usize)> =
            std::collections::HashMap::new();
        let mut total_size = 0u64;

        for file in &self.cleanable_files {
            total_size += file.size;
            let entry = categories.entry(file.category.clone()).or_insert((0, 0));
            entry.0 += file.size;
            entry.1 += 1;
        }

        let category_reports: Vec<CategoryReport> = categories
            .into_iter()
            .map(|(name, (size, count))| CategoryReport { name, size, count })
            .collect();

        // Load advanced categories from rules for UI toggling
        let rules: CleanerRules = load_rules();
        let advanced: Vec<String> = rules
            .categories
            .into_iter()
            .filter(|r| r.advanced.unwrap_or(false))
            .map(|r| r.name)
            .collect();

        CleaningReport {
            total_size,
            files_count: self.cleanable_files.len(),
            categories: category_reports,
            advanced_categories: advanced,
        }
    }

    pub fn get_cleanable_files(&self) -> &Vec<CleanableFile> {
        &self.cleanable_files
    }

    /// Remove selected items in cancellable batches via `clean_files_with_cancel`.
    async fn clean_directory_batch(&self, _dir: PathBuf, files: Vec<String>) -> (u64, usize) {
        let mut total_freed = 0u64;
        let mut items_removed = 0usize;
        let mut errors = Vec::new();

        // Collect items that failed due to permissions to retry once with elevation
        #[cfg(target_os = "macos")]
        #[derive(Clone)]
        struct PendingElevated {
            path: String,
            size: u64,
            is_dir: bool,
        }

        #[cfg(target_os = "macos")]
        let mut pending_elevated: Vec<PendingElevated> = Vec::new();

        for path_str in files {
            // cooperative cancellation is injected by outer wrappers
            let path = Path::new(&path_str);

            // Only allow deleting items that were part of the latest scan
            let maybe_item = self.cleanable_files.iter().find(|f| f.path == path_str);
            if maybe_item.is_none() {
                errors.push(format!("Skipping unknown item: {}", path_str));
                continue;
            }
            let is_dir = path.is_dir();

            // Get size before deletion (directories need recursive sizing)
            let item_size = if is_dir {
                self.get_directory_size_async(path).await.unwrap_or(0)
            } else {
                match fs::metadata(path) {
                    Ok(m) => m.len(),
                    Err(e) => {
                        if e.kind() == ErrorKind::NotFound {
                            items_removed += 1;
                            continue;
                        }
                        0
                    }
                }
            };

            // Prefer moving to Trash for safety; fallback to direct removal if needed
            match self.move_to_trash(path).await {
                Ok(_) => {
                    total_freed += item_size;
                    items_removed += 1;
                    if let Some(parent) = path.parent() {
                        Self::invalidate_scan_caches(parent).await;
                    }
                    continue;
                }
                Err(_trash_err) => {
                    // If trash move failed, try direct removal appropriate to type
                    let res = if is_dir {
                        fs::remove_dir_all(path)
                    } else {
                        fs::remove_file(path)
                    };

                    match res {
                        Ok(_) => {
                            total_freed += item_size;
                            items_removed += 1;
                            if let Some(parent) = path.parent() {
                                Self::invalidate_scan_caches(parent).await;
                            }
                        }
                        Err(e) => {
                            use std::io::ErrorKind::*;
                            match e.kind() {
                                NotFound => {
                                    items_removed += 1;
                                }
                                PermissionDenied => {
                                    // Queue for a single elevated removal attempt later
                                    #[cfg(target_os = "macos")]
                                    {
                                        pending_elevated.push(PendingElevated {
                                            path: path_str.clone(),
                                            size: item_size,
                                            is_dir,
                                        });
                                    }

                                    #[cfg(not(target_os = "macos"))]
                                    {
                                        errors.push(format!(
                                            "Missing permissions to remove {} and elevation is unavailable on this platform",
                                            path_str
                                        ));
                                    }
                                }
                                _ => {
                                    errors.push(format!("Failed to remove {}: {}", path_str, e));
                                }
                            }
                        }
                    }
                }
            }
        }

        // On macOS, retry permission-denied items once using a single admin prompt
        #[cfg(target_os = "macos")]
        if !pending_elevated.is_empty() {
            match Self::remove_with_admin(
                &pending_elevated
                    .iter()
                    .map(|p| p.path.as_str())
                    .collect::<Vec<_>>(),
            )
            .await
            {
                Ok(_) => {
                    // Verify and account freed sizes
                    for p in pending_elevated.iter() {
                        let path = Path::new(&p.path);
                        if !path.exists() {
                            total_freed += p.size;
                            items_removed += 1;
                            if let Some(parent) = Path::new(&p.path).parent() {
                                Self::invalidate_scan_caches(parent).await;
                            }
                        } else {
                            // Fallback check: try a final direct removal if elevation succeeded partially
                            let _ = if p.is_dir {
                                fs::remove_dir_all(path)
                            } else {
                                fs::remove_file(path)
                            };
                            if !path.exists() {
                                total_freed += p.size;
                                items_removed += 1;
                                if let Some(parent) = path.parent() {
                                    Self::invalidate_scan_caches(parent).await;
                                }
                            } else {
                                errors.push(format!(
                                    "Failed to remove {} even with admin rights",
                                    p.path
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("Admin removal failed: {}", e));
                }
            }
        }

        if !errors.is_empty() {
            eprintln!("Cleaning errors: {:?}", errors);
        }

        (total_freed, items_removed)
    }

    #[cfg(target_os = "macos")]
    async fn remove_with_admin(paths: &[&str]) -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }

        // Restrict to user's home directory for safety
        let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
        let home_str = home.to_string_lossy().to_string();

        // Build AppleScript that constructs a single shell script and runs it once with admin rights.
        // Use 'quoted form of POSIX path' for robust escaping.
        let mut script = String::from("set cmd to \"\"\n");
        for p in paths {
            // Only include paths within the user's home directory
            if p.starts_with(&home_str) {
                let escaped = p.replace("\\", "\\\\").replace("\"", "\\\"");
                script.push_str(&format!(
                    "set cmd to cmd & \"rm -rf \" & quoted form of POSIX path of \"{}\" & \"\n\"\n",
                    escaped
                ));
            }
        }

        // If nothing eligible, bail out
        if !script.contains("rm -rf") {
            return Ok(());
        }

        script.push_str("do shell script cmd with administrator privileges\n");

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .await
            .map_err(|e| format!("Failed to run osascript: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "osascript failed (status: {:?}): {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Move a file or directory to the macOS Trash.
    /// 1) Ask Finder (osascript) — handles per-volume trash & name collisions.
    /// 2) Fallback: rename into ~/.Trash with a unique name.
    async fn move_to_trash(&self, path: &Path) -> Result<(), String> {
        // Try Finder first
        if !is_osascript_disabled() {
            let path_str = path.to_string_lossy();
            let escaped = path_str.replace("\\", "\\\\").replace("\"", "\\\"");
            let script = format!(
                "tell application \"Finder\" to move POSIX file \"{}\" to trash",
                escaped
            );

            match Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    return Ok(());
                }
                Ok(output) => {
                    log::warn!(
                        "Finder trash command failed (status {:?}): {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                Err(err) => {
                    log::warn!("Failed to execute AppleScript for trash move: {}", err);
                }
            }
        }

        // Fallback: rename into ~/.Trash with a unique name
        let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
        let trash = home.join(".Trash");
        if !trash.exists() {
            fs::create_dir_all(&trash)
                .map_err(|e| format!("Failed to create trash directory: {}", e))?;
        }

        let original_name = path
            .file_name()
            .ok_or_else(|| "Invalid filename".to_string())?;
        let mut target = trash.join(original_name);

        // Ensure unique filename
        if target.exists() {
            let stem = original_name.to_string_lossy().to_string();
            let (base, ext) = split_name_ext(&stem);
            let ts = Local::now().format("%Y%m%d-%H%M%S").to_string();
            let mut counter = 1u32;
            loop {
                let candidate = if ext.is_empty() {
                    format!("{} ({}-{})", base, ts, counter)
                } else {
                    format!("{} ({}-{}).{}", base, ts, counter, ext)
                };
                target = trash.join(candidate);
                if !target.exists() {
                    break;
                }
                counter += 1;
            }
        }

        fs::rename(path, &target).map_err(|e| format!("Failed to move to trash: {}", e))?;
        Ok(())
    }

    /// Empty Trash using Finder (preferred), with safe fallbacks.
    pub async fn empty_trash(&self) -> Result<(u64, usize), String> {
        // Get initial trash size and count
        let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
        let trash_dir = home.join(".Trash");

        if !trash_dir.exists() {
            return Ok((0, 0));
        }

        let size_before = self.get_directory_size_async(&trash_dir).await.unwrap_or(0);
        let count_before = fs::read_dir(&trash_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);

        // First attempt: Use AppleScript to empty trash properly through Finder
        let mut emptied_via_finder = false;
        if !is_osascript_disabled() {
            match Command::new("osascript")
                .arg("-e")
                .arg("tell application \"Finder\" to empty trash")
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    emptied_via_finder = true;
                }
                Ok(output) => {
                    log::warn!(
                        "Finder empty trash failed (status {:?}): {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                Err(err) => {
                    log::warn!("Failed to execute Finder empty trash AppleScript: {}", err);
                }
            }
        }

        if !emptied_via_finder {
            // If AppleScript fails, try removing contents manually
            if let Ok(entries) = fs::read_dir(&trash_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }

            // If still items remain, try shell command to clear trash contents
            if let Some(trash_str) = trash_dir.to_str() {
                let trash_contents = format!("{}/{}", trash_str, "*");
                let _ = Command::new("sh")
                    .arg("-c")
                    .arg(format!("rm -rf {}", trash_contents))
                    .output()
                    .await;
            }
        }

        // Wait a moment for operations to complete
        sleep(Duration::from_millis(500)).await;

        // Ensure cached directory sizes reflect the latest state before measuring again
        DIR_SIZE_CACHE.invalidate(&trash_dir).await;

        // Calculate freed space
        let size_after = self.get_directory_size_async(&trash_dir).await.unwrap_or(0);
        let count_after = fs::read_dir(&trash_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);

        let freed = size_before.saturating_sub(size_after);
        let removed = count_before.saturating_sub(count_after);

        Ok((freed, removed))
    }

    // Cancellable wrappers for cleaning and trash
    pub async fn clean_files_with_cancel(
        &self,
        file_paths: Vec<String>,
        cancel: &CancellationToken,
    ) -> Result<(u64, usize), String> {
        let mut total_freed = 0u64;
        let mut items_removed = 0usize;
        let mut files_by_dir: HashMap<PathBuf, Vec<String>> = HashMap::new();
        for p in &file_paths {
            let path = Path::new(p);
            let parent = path
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf();
            files_by_dir
                .entry(parent)
                .or_insert_with(Vec::new)
                .push(p.clone());
        }
        for (dir, files) in files_by_dir.into_iter() {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            let (f, n) = self.clean_directory_batch(dir, files).await;
            total_freed += f;
            items_removed += n;
        }
        Ok((total_freed, items_removed))
    }

    pub async fn empty_trash_with_cancel(
        &self,
        cancel: &CancellationToken,
    ) -> Result<(u64, usize), String> {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let (freed, removed) = self.empty_trash().await?;
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        Ok((freed, removed))
    }

    pub async fn get_directory_size_async(&self, path: &Path) -> Result<u64, String> {
        DIR_SIZE_CACHE
            .get_or_calculate(path, |p| {
                let mut total = 0u64;
                for entry in WalkDir::new(p).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() {
                        if let Ok(md) = fs::metadata(entry.path()) {
                            total += md.len();
                        }
                    }
                }
                Ok(total)
            })
            .await
    }

    pub(crate) fn get_directory_size_blocking(&self, path: &Path) -> Result<u64, String> {
        // Synchronous calculation without touching the async runtime (safe in rayon/scan contexts)
        let mut total = 0u64;
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Ok(md) = fs::metadata(entry.path()) {
                    total += md.len();
                }
            }
        }
        Ok(total)
    }

    pub fn get_auto_selectable_files(&self) -> Vec<CleanableFile> {
        self.cleanable_files
            .iter()
            .filter(|f| f.auto_select && f.safe_to_delete)
            .cloned()
            .collect()
    }

    pub fn get_files_by_safety(&self, min_safety_score: u8) -> Vec<CleanableFile> {
        self.cleanable_files
            .iter()
            .filter(|f| f.safety_score >= min_safety_score && f.safe_to_delete)
            .cloned()
            .collect()
    }

    fn expand_path(input: &str) -> Option<PathBuf> {
        if input.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return Some(home.join(&input[2..]));
            } else {
                return None;
            }
        }
        Some(PathBuf::from(input))
    }

    async fn invalidate_scan_caches(path: &Path) {
        DIR_SIZE_CACHE.invalidate(path).await;
        #[cfg(feature = "metadata-cache")]
        FILE_METADATA_CACHE.invalidate(path).await;
    }
}

/// Utility: split a name into (base, ext) without touching the filesystem.
fn split_name_ext(name: &str) -> (String, String) {
    if let Some(idx) = name.rfind('.') {
        let (base, ext) = name.split_at(idx);
        (base.to_string(), ext.trim_start_matches('.').to_string())
    } else {
        (name.to_string(), String::new())
    }
}

fn is_osascript_disabled() -> bool {
    env::var("MACOS_OPTIMIZER_DISABLE_OSA")
        .map(|value| {
            let lowercase = value.trim().to_ascii_lowercase();
            lowercase == "1" || lowercase == "true" || lowercase == "yes"
        })
        .unwrap_or(false)
}
