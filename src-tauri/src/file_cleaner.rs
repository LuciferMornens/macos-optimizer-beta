use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use chrono::{DateTime, Utc, Duration};
use dirs;
use serde_json;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CleanableFile {
    pub path: String,
    pub size: u64,
    pub category: String,
    pub description: String,
    pub last_modified: i64,
    pub safe_to_delete: bool,
    pub safety_score: u8,  // 0-100, where 100 is completely safe
    pub auto_select: bool,  // Should be auto-selected for cleaning
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CleaningReport {
    pub total_size: u64,
    pub files_count: usize,
    pub categories: Vec<CategoryReport>,
    pub advanced_categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryReport {
    pub name: String,
    pub size: u64,
    pub count: usize,
}

pub struct FileCleaner {
    cleanable_files: Vec<CleanableFile>,
    seen_paths: HashSet<String>,
}

impl FileCleaner {
    pub fn new() -> Self {
        FileCleaner {
            cleanable_files: Vec::new(),
            seen_paths: HashSet::new(),
        }
    }

    pub fn scan_system(&mut self) -> Result<CleaningReport, String> {
        self.cleanable_files.clear();
        self.seen_paths.clear();

        // Load rule set (embedded at compile time)
        let raw = include_str!("../rules/cleaner_rules.json");
        let rules: CleanerRules = serde_json::from_str(raw)
            .map_err(|e| format!("Failed to parse cleaner rules: {}", e))?;

        // Apply categories in order (specific first to avoid double counting)
        for rule in rules.categories.iter() {
            for p in &rule.paths {
                if let Some(path) = Self::expand_path(p) {
                    if path.exists() {
                        self.scan_path_with_rule(&path, rule)?;
                    }
                }
            }
        }

        Ok(self.generate_report())
    }
    
    fn scan_path_with_rule(&mut self, path: &Path, rule: &CategoryRule) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }

        let now = Utc::now();
        let max_depth = rule.max_depth.unwrap_or(5);
        let min_age = rule.min_age_days;
        let min_size_bytes_from_rule = rule.min_size_kb.map(|kb| kb * 1024);
        let excludes = rule.excludes.as_ref().map(|v| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>()).unwrap_or_default();
        let exts = rule.extensions.as_ref().map(|v| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>());
        let require_subpaths = rule
            .require_subpaths
            .as_ref()
            .map(|v| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>())
            .unwrap_or_default();

        for entry in WalkDir::new(path)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let file_path = entry.path();
                let key = file_path.to_string_lossy().to_string();

                // De-duplicate across multiple scans/categories
                if self.seen_paths.contains(&key) {
                    continue;
                }

                // Exclude by simple substring match on lowercased path
                let path_lower = key.to_lowercase();
                if excludes.iter().any(|ex| path_lower.contains(ex)) {
                    continue;
                }

                // Require at least one matching subpath if specified
                if !require_subpaths.is_empty()
                    && !require_subpaths.iter().any(|req| path_lower.contains(req))
                {
                    continue;
                }

                if let Ok(metadata) = fs::metadata(file_path) {
                    // Extension filter
                    if let Some(ref allowed_exts) = exts {
                        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) {
                            if !allowed_exts.iter().any(|e| e == &ext) {
                                continue;
                            }
                        } else {
                            // No extension – skip when extensions filter is set
                            continue;
                        }
                    }
                    // Age filter
                    if let Some(days) = min_age {
                        if let Ok(modified) = metadata.modified() {
                            let modified_time = DateTime::<Utc>::from(modified);
                            if now.signed_duration_since(modified_time) < Duration::days(days) {
                                continue;
                            }
                        }
                    }

                    // Size filter (default skip tiny files < 1KB)
                    let file_size = metadata.len();
                    let tiny_threshold = 1024u64;
                    let min_size = min_size_bytes_from_rule.unwrap_or(tiny_threshold).max(tiny_threshold);
                    if file_size < min_size {
                        continue;
                    }

                    let last_modified = metadata
                        .modified()
                        .map(|t| DateTime::<Utc>::from(t).timestamp())
                        .unwrap_or(0);

                    let is_safe = rule.safe && self.is_safe_to_delete(file_path);
                    let (safety_score, auto_select) = self.calculate_safety_score(file_path, &rule.name, rule.min_age_days, is_safe);

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

                    self.cleanable_files.push(cleanable);
                    self.seen_paths.insert(key);
                }
            }
        }

        Ok(())
    }

    fn get_file_description(&self, path: &Path, category: &str) -> String {
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");
        
        match category {
            "System Cache" | "System Cache (Advanced)" => format!("System cache: {}", filename),
            "User Cache" => format!("Cache file: {}", filename),
            "Browser Cache" => format!("Browser cache: {}", filename),
            "App Store Cache" => format!("App Store cache: {}", filename),
            "Music Cache" => format!("Music cache: {}", filename),
            "Trash" => format!("Trashed file: {}", filename),
            "Incomplete Downloads (2d+)" => format!("Incomplete download: {}", filename),
            "Saved Application State (30d+)" => format!("Saved state: {}", filename),
            "Container Caches (Advanced)" => format!("Container cache: {}", filename),
            "Container Temp (Advanced)" => format!("Container tmp: {}", filename),
            "Group Container Caches (Advanced)" => format!("Group container cache: {}", filename),
            "App Support Caches (Advanced)" => format!("App support cache: {}", filename),
            "Dropbox Cache" => format!("Dropbox cache: {}", filename),
            "Old Downloads" | "Old Downloads (90d+)" => format!("Old download: {}", filename),
            "Large Stale Files (Desktop/Downloads)" => format!("Large stale file: {}", filename),
            "Temporary Files" => format!("Temporary file: {}", filename),
            "User Temporary Files" => format!("Temporary file: {}", filename),
            "QuickLook Cache" => format!("QuickLook thumbnail: {}", filename),
            "User Logs (30d+)" => format!("Old log file: {}", filename),
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

    fn is_safe_to_delete(&self, path: &Path) -> bool {
        // Never delete certain critical files
        let path_str = path.to_string_lossy().to_lowercase();
        
        // List of patterns that should never be deleted
        let protected_patterns = vec![
            ".ssh",
            ".gnupg",
            ".keychain",
            "passwords",
            "credentials",
            ".env",
            "config",
            "preferences",
            ".git",
            "node_modules", // Let user decide about node_modules
            "documents",
            "desktop",
            "pictures",
            "movies",
            "music",
            "photos",
            ".pem",
            ".key",
            ".cert",
            ".p12",
            "wallet",
            "vault",
            "backup",
            "archive",
            "important",
            "personal",
            "private",
            "secret",
            ".localized",
            "library/application support",
            "library/preferences",
            "library/keychains",
            "library/accounts",
            "library/cookies",
            "library/mail",
            "library/messages",
            "library/safari",
        ];

        for pattern in protected_patterns {
            if path_str.contains(pattern) {
                return false;
            }
        }

        // Allow safe subpaths within Containers/Group Containers: Caches and tmp
        if (path_str.contains("library/containers") || path_str.contains("library/group containers"))
            && (path_str.contains("/caches/") || path_str.ends_with("/caches") || path_str.contains("/tmp/") || path_str.ends_with("/tmp"))
        {
            return true;
        }

        // Additional checks for specific file extensions
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            let protected_extensions = vec![
                "doc", "docx", "xls", "xlsx", "ppt", "pptx",
                "pdf", "txt", "rtf", "pages", "numbers", "keynote",
                "sqlite", "db", "sql",
                "jpg", "jpeg", "png", "gif", "raw", "psd", "ai",
                "mp3", "mp4", "mov", "avi", "mkv",
                "zip", "rar", "7z", "tar", "gz",
            ];
            
            // Files with these extensions in Downloads are less protected (user can decide)
            let is_downloads = path_str.contains("/downloads/");
            if !is_downloads && protected_extensions.contains(&ext_str.as_str()) {
                return false;
            }
        }

        true
    }

    fn generate_report(&self) -> CleaningReport {
        let mut categories: std::collections::HashMap<String, (u64, usize)> = std::collections::HashMap::new();
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
        let raw = include_str!("../rules/cleaner_rules.json");
        let rules: CleanerRules = serde_json::from_str(raw).unwrap_or(CleanerRules { categories: vec![] });
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

    pub fn clean_files(&self, file_paths: Vec<String>) -> Result<(u64, usize), String> {
        let mut total_freed = 0u64;
        let mut files_deleted = 0usize;
        let mut errors = Vec::new();

        for path_str in file_paths {
            let path = Path::new(&path_str);
            
            // Find the file in our cleanable list to verify it's safe
            let is_safe = self.cleanable_files
                .iter()
                .find(|f| f.path == path_str)
                .map(|f| f.safe_to_delete)
                .unwrap_or(false);

            if !is_safe {
                errors.push(format!("Skipping unsafe file: {}", path_str));
                continue;
            }

            // Get file size before deletion
            let file_size = match fs::metadata(path) {
                Ok(m) => m.len(),
                Err(e) => {
                    // If the file is already gone, treat as success (nothing to do)
                    if e.kind() == ErrorKind::NotFound {
                        files_deleted += 1; // consider it handled
                        continue;
                    }
                    0
                }
            };

            // Attempt to delete the file
            match fs::remove_file(path) {
                Ok(_) => {
                    total_freed += file_size;
                    files_deleted += 1;
                }
                Err(e) => {
                    if e.kind() == ErrorKind::NotFound {
                        // Already deleted by another process; count as handled
                        files_deleted += 1;
                        continue;
                    }
                    // Try to move to trash instead of permanent deletion
                    match self.move_to_trash(path) {
                        Ok(_) => {
                            total_freed += file_size;
                            files_deleted += 1;
                        }
                        Err(trash_err) => {
                            // If the trash move failed because file disappeared in the meantime, treat as handled
                            if trash_err.contains("No such file or directory") {
                                files_deleted += 1;
                            } else {
                                errors.push(format!(
                                    "Failed to delete {}: {} (trash: {})",
                                    path_str, e, trash_err
                                ));
                            }
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            eprintln!("Cleaning errors: {:?}", errors);
        }

        Ok((total_freed, files_deleted))
    }

    fn move_to_trash(&self, path: &Path) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            let trash = home.join(".Trash");
            let filename = path.file_name()
                .ok_or_else(|| "Invalid filename".to_string())?;
            
            let trash_path = trash.join(filename);
            
            fs::rename(path, trash_path)
                .map_err(|e| format!("Failed to move to trash: {}", e))?;
            
            Ok(())
        } else {
            Err("Could not find home directory".to_string())
        }
    }

    pub fn empty_trash(&self) -> Result<(u64, usize), String> {
        use std::process::Command;

        // Get initial trash size and count
        let home = dirs::home_dir()
            .ok_or_else(|| "Could not find home directory".to_string())?;
        let trash_dir = home.join(".Trash");
        
        if !trash_dir.exists() {
            return Ok((0, 0));
        }

        let size_before = self.get_directory_size(&trash_dir).unwrap_or(0);
        let count_before = fs::read_dir(&trash_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);

        // First attempt: Use AppleScript to empty trash properly through Finder
        // This respects macOS trash handling and permissions
        let script = "tell application \"Finder\" to empty trash";
        
        let applescript_output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| format!("Failed to execute AppleScript: {}", e))?;
        
        if !applescript_output.status.success() {
            // If AppleScript fails, try removing contents manually
            // Only remove contents, not the .Trash directory itself
            if let Ok(entries) = fs::read_dir(&trash_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    // Try to remove each item in trash
                    if path.is_dir() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
            
            // If still items remain, try shell command to clear trash contents
            let trash_contents = format!("{}/*", trash_dir.to_str().unwrap());
            let _ = Command::new("sh")
                .arg("-c")
                .arg(format!("rm -rf {}", trash_contents))
                .output();
        }

        // Wait a moment for operations to complete
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Calculate freed space
        let size_after = self.get_directory_size(&trash_dir).unwrap_or(0);
        let count_after = fs::read_dir(&trash_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);
            
        let freed = size_before.saturating_sub(size_after);
        let removed = count_before.saturating_sub(count_after);
        
        Ok((freed, removed))
    }

    fn get_directory_size(&self, path: &Path) -> Result<u64, String> {
        let mut total_size = 0u64;
        
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(metadata) = fs::metadata(entry.path()) {
                    total_size += metadata.len();
                }
            }
        }
        
        Ok(total_size)
    }

    fn calculate_safety_score(&self, path: &Path, category: &str, days_old: Option<i64>, is_safe: bool) -> (u8, bool) {
        if !is_safe {
            return (0, false);
        }

        let mut score: u8;
        let mut auto_select = false;

        // Category-based scoring
        match category {
            "Trash" => {
                score = 100;
                auto_select = true; // Always auto-select trash
            },
            "System Cache" | "System Cache (Advanced)" | "User Cache" | "Browser Cache" | "App Store Cache" | "Music Cache" | 
            "Container Caches (Advanced)" | "Group Container Caches (Advanced)" | "App Support Caches (Advanced)" | "Dropbox Cache" => {
                score = 95;
                auto_select = true; // Caches are very safe to delete
            },
            "Temporary Files" | "User Temporary Files" | "Container Temp (Advanced)" => {
                score = 90;
                auto_select = true; // Temp files are safe
            },
            "Saved Application State (30d+)" => {
                score = 90;
                // Old saved states are safe
                if let Some(days) = days_old { if days >= 30 { auto_select = true; } }
            },
            "Incomplete Downloads (2d+)" => {
                score = 95;
                auto_select = true; // Incomplete downloads with min age are safe
            },
            "User Logs (30d+)" | "System Logs (30d+, Advanced)" | "Crash Reports (30d+)" | "System Crash Reports (30d+, Advanced)" => {
                score = 80;
                // Only auto-select old logs
                if let Some(days) = days_old {
                    if days >= 30 {
                        auto_select = true;
                    }
                }
            },
            "Old Downloads" | "Old Downloads (90d+)" | "Old Installers (30d+)" => {
                score = 60;
                // Don't auto-select downloads, user should review
                auto_select = false;
            },
            "Large Stale Files (Desktop/Downloads)" | "Mail Downloads (Review)" | "Messages Attachments (90d+, Review)" => {
                score = 50;
                auto_select = false; // Review before deleting
            },
            "iOS Updates (Advanced)" => {
                score = 50;
                auto_select = false;
            },
            "iOS Backups (Advanced)" => {
                score = 40;
                auto_select = false;
            },
            _ => {
                score = 40;
                auto_select = false;
            }
        }

        // File age adjustment
        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                let modified_time = DateTime::<Utc>::from(modified);
                let age_days = Utc::now().signed_duration_since(modified_time).num_days();
                
                // Increase safety for very old files in safe categories
                if age_days > 90 && score >= 80 {
                    score = score.min(100);
                    if category != "Old Downloads" && category != "Xcode Archives" {
                        auto_select = true;
                    }
                }
                
                // Decrease safety for recently modified files
                if age_days < 7 && score > 50 {
                    score = score.saturating_sub(20);
                    auto_select = false;
                }
            }
        }

        // File size adjustment for auto-select
        if let Ok(metadata) = fs::metadata(path) {
            let size = metadata.len();
            // Don't auto-select very large files (>500MB) unless they're in trash or cache
            if size > 500 * 1024 * 1024 && !matches!(category, "Trash" | "System Cache" | "Browser Cache") {
                auto_select = false;
            }
        }

        // Path-based adjustments
        let path_str = path.to_string_lossy().to_lowercase();
        
        // Increase safety for known safe patterns
        if path_str.contains(".cache") || path_str.contains("cache/") || path_str.contains("/tmp/") {
            score = score.min(95);
        }
        
        // Decrease safety for patterns that might be important
        if path_str.contains("backup") || path_str.contains("archive") || path_str.contains("export") {
            score = score.saturating_sub(30);
            auto_select = false;
        }

        (score, auto_select)
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
}

// -------- Rule Engine Types & Helpers --------

#[derive(Debug, Deserialize)]
struct CleanerRules {
    categories: Vec<CategoryRule>,
}

#[derive(Debug, Deserialize)]
struct CategoryRule {
    name: String,
    paths: Vec<String>,
    safe: bool,
    #[allow(dead_code)]
    advanced: Option<bool>,
    max_depth: Option<usize>,
    min_age_days: Option<i64>,
    min_size_kb: Option<u64>,
    excludes: Option<Vec<String>>,
    extensions: Option<Vec<String>>,
    // When set, file path must include at least one of these substrings
    require_subpaths: Option<Vec<String>>,
}

impl FileCleaner {
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
}
