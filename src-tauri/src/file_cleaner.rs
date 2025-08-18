use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use chrono::{DateTime, Utc, Duration};
use dirs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CleanableFile {
    pub path: String,
    pub size: u64,
    pub category: String,
    pub description: String,
    pub last_modified: i64,
    pub safe_to_delete: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CleaningReport {
    pub total_size: u64,
    pub files_count: usize,
    pub categories: Vec<CategoryReport>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryReport {
    pub name: String,
    pub size: u64,
    pub count: usize,
}

pub struct FileCleaner {
    cleanable_files: Vec<CleanableFile>,
}

impl FileCleaner {
    pub fn new() -> Self {
        FileCleaner {
            cleanable_files: Vec::new(),
        }
    }

    pub fn scan_system(&mut self) -> Result<CleaningReport, String> {
        self.cleanable_files.clear();

        // Scan various locations for cleanable files
        self.scan_cache_files()?;
        self.scan_log_files()?;
        self.scan_trash()?;
        self.scan_downloads_old_files()?;
        self.scan_temporary_files()?;
        self.scan_browser_cache()?;
        self.scan_xcode_derived_data()?;
        self.scan_npm_cache()?;
        self.scan_pip_cache()?;
        self.scan_homebrew_cache()?;

        Ok(self.generate_report())
    }

    fn scan_cache_files(&mut self) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            let cache_dir = home.join("Library/Caches");
            if cache_dir.exists() {
                self.scan_directory(&cache_dir, "System Cache", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_log_files(&mut self) -> Result<(), String> {
        // System logs
        let system_logs = Path::new("/var/log");
        if system_logs.exists() {
            self.scan_directory(system_logs, "System Logs", true, Some(30))?;
        }

        // User logs
        if let Some(home) = dirs::home_dir() {
            let user_logs = home.join("Library/Logs");
            if user_logs.exists() {
                self.scan_directory(&user_logs, "User Logs", true, Some(30))?;
            }
        }
        Ok(())
    }

    fn scan_trash(&mut self) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            let trash = home.join(".Trash");
            if trash.exists() {
                self.scan_directory(&trash, "Trash", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_downloads_old_files(&mut self) -> Result<(), String> {
        if let Some(downloads) = dirs::download_dir() {
            // Scan for old downloads (older than 90 days)
            self.scan_directory(&downloads, "Old Downloads", false, Some(90))?;
        }
        Ok(())
    }

    fn scan_temporary_files(&mut self) -> Result<(), String> {
        // macOS temp directories
        let temp_dirs = vec![
            PathBuf::from("/tmp"),
            PathBuf::from("/var/tmp"),
            PathBuf::from("/private/tmp"),
            PathBuf::from("/private/var/tmp"),
        ];

        for temp_dir in temp_dirs {
            if temp_dir.exists() {
                self.scan_directory(&temp_dir, "Temporary Files", true, Some(7))?;
            }
        }

        // User temp directory
        if let Some(home) = dirs::home_dir() {
            let user_temp = home.join("Library/Caches/TemporaryItems");
            if user_temp.exists() {
                self.scan_directory(&user_temp, "User Temporary Files", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_browser_cache(&mut self) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            // Safari cache
            let safari_cache = home.join("Library/Caches/com.apple.Safari");
            if safari_cache.exists() {
                self.scan_directory(&safari_cache, "Safari Cache", true, None)?;
            }

            // Chrome cache
            let chrome_cache = home.join("Library/Caches/Google/Chrome");
            if chrome_cache.exists() {
                self.scan_directory(&chrome_cache, "Chrome Cache", true, None)?;
            }

            // Firefox cache
            let firefox_cache = home.join("Library/Caches/Firefox");
            if firefox_cache.exists() {
                self.scan_directory(&firefox_cache, "Firefox Cache", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_xcode_derived_data(&mut self) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            let xcode_derived = home.join("Library/Developer/Xcode/DerivedData");
            if xcode_derived.exists() {
                self.scan_directory(&xcode_derived, "Xcode Derived Data", true, None)?;
            }
            
            // Xcode Archives
            let xcode_archives = home.join("Library/Developer/Xcode/Archives");
            if xcode_archives.exists() {
                self.scan_directory(&xcode_archives, "Xcode Archives", false, Some(180))?;
            }
        }
        Ok(())
    }

    fn scan_npm_cache(&mut self) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            let npm_cache = home.join(".npm");
            if npm_cache.exists() {
                self.scan_directory(&npm_cache, "NPM Cache", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_pip_cache(&mut self) -> Result<(), String> {
        if let Some(home) = dirs::home_dir() {
            let pip_cache = home.join("Library/Caches/pip");
            if pip_cache.exists() {
                self.scan_directory(&pip_cache, "Python Pip Cache", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_homebrew_cache(&mut self) -> Result<(), String> {
        let homebrew_caches = vec![
            PathBuf::from("/opt/homebrew/var/cache"),
            PathBuf::from("/usr/local/var/cache/homebrew"),
            PathBuf::from("/Library/Caches/Homebrew"),
        ];

        for cache_dir in homebrew_caches {
            if cache_dir.exists() {
                self.scan_directory(&cache_dir, "Homebrew Cache", true, None)?;
            }
        }

        if let Some(home) = dirs::home_dir() {
            let user_homebrew = home.join("Library/Caches/Homebrew");
            if user_homebrew.exists() {
                self.scan_directory(&user_homebrew, "Homebrew Cache", true, None)?;
            }
        }
        Ok(())
    }

    fn scan_directory(
        &mut self,
        path: &Path,
        category: &str,
        safe_to_delete: bool,
        days_old: Option<i64>,
    ) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }

        let now = Utc::now();
        
        for entry in WalkDir::new(path)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let file_path = entry.path();
                
                // Check if file should be included based on age
                if let Some(days) = days_old {
                    if let Ok(metadata) = fs::metadata(file_path) {
                        if let Ok(modified) = metadata.modified() {
                            let modified_time = DateTime::<Utc>::from(modified);
                            let age = now.signed_duration_since(modified_time);
                            
                            if age < Duration::days(days) {
                                continue; // Skip files newer than threshold
                            }
                        }
                    }
                }

                // Get file size
                if let Ok(metadata) = fs::metadata(file_path) {
                    let file_size = metadata.len();
                    
                    // Skip very small files (less than 1KB)
                    if file_size < 1024 {
                        continue;
                    }

                    let last_modified = metadata
                        .modified()
                        .map(|t| DateTime::<Utc>::from(t).timestamp())
                        .unwrap_or(0);

                    let cleanable = CleanableFile {
                        path: file_path.to_string_lossy().to_string(),
                        size: file_size,
                        category: category.to_string(),
                        description: self.get_file_description(file_path, category),
                        last_modified,
                        safe_to_delete: safe_to_delete && self.is_safe_to_delete(file_path),
                    };

                    self.cleanable_files.push(cleanable);
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
            "System Cache" => format!("Cache file: {}", filename),
            "Browser Cache" => format!("Browser cache: {}", filename),
            "Trash" => format!("Trashed file: {}", filename),
            "Old Downloads" => format!("Old download: {}", filename),
            "Temporary Files" => format!("Temporary file: {}", filename),
            "Xcode Derived Data" => format!("Xcode build artifact: {}", filename),
            "NPM Cache" => format!("NPM package cache: {}", filename),
            "Python Pip Cache" => format!("Python package cache: {}", filename),
            "Homebrew Cache" => format!("Homebrew package cache: {}", filename),
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
        ];

        for pattern in protected_patterns {
            if path_str.contains(pattern) {
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

        CleaningReport {
            total_size,
            files_count: self.cleanable_files.len(),
            categories: category_reports,
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
            let file_size = fs::metadata(path)
                .map(|m| m.len())
                .unwrap_or(0);

            // Attempt to delete the file
            match fs::remove_file(path) {
                Ok(_) => {
                    total_freed += file_size;
                    files_deleted += 1;
                }
                Err(e) => {
                    // Try to move to trash instead of permanent deletion
                    if let Err(trash_err) = self.move_to_trash(path) {
                        errors.push(format!("Failed to delete {}: {} (trash: {})", path_str, e, trash_err));
                    } else {
                        total_freed += file_size;
                        files_deleted += 1;
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
        if let Some(home) = dirs::home_dir() {
            let trash = home.join(".Trash");
            if !trash.exists() {
                return Ok((0, 0));
            }

            let mut total_freed = 0u64;
            let mut files_deleted = 0usize;

            for entry in fs::read_dir(&trash).map_err(|e| e.to_string())? {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    
                    // Get size before deletion
                    let size = if path.is_dir() {
                        self.get_directory_size(&path).unwrap_or(0)
                    } else {
                        fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
                    };

                    // Remove file or directory
                    if path.is_dir() {
                        if fs::remove_dir_all(&path).is_ok() {
                            total_freed += size;
                            files_deleted += 1;
                        }
                    } else {
                        if fs::remove_file(&path).is_ok() {
                            total_freed += size;
                            files_deleted += 1;
                        }
                    }
                }
            }

            Ok((total_freed, files_deleted))
        } else {
            Err("Could not find home directory".to_string())
        }
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
}