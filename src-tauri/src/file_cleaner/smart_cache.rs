use super::process_snapshot::ProcessSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use sysinfo::System;

/// Smart cache detection with validation
pub struct SmartCacheDetector {
    cache_signatures: HashMap<String, CacheSignature>,
    app_activity_checker: AppActivityChecker,
}

impl SmartCacheDetector {
    pub fn new() -> Self {
        let mut cache_signatures = HashMap::new();

        // Define known cache signatures
        cache_signatures.insert(
            "browser_cache".to_string(),
            CacheSignature {
                path_patterns: vec![
                    "/Library/Caches/com.apple.Safari/".to_string(),
                    "/Library/Caches/Google/Chrome/".to_string(),
                    "/Library/Caches/Firefox/".to_string(),
                    "/Library/Caches/com.brave.Browser/".to_string(),
                ],
                file_patterns: vec![
                    "*.cache".to_string(),
                    "*.db".to_string(),
                    "*.sqlite".to_string(),
                ],
                regeneratable: true,
                importance: CacheImportance::Low,
            },
        );

        cache_signatures.insert(
            "xcode_cache".to_string(),
            CacheSignature {
                path_patterns: vec![
                    "/Library/Developer/Xcode/DerivedData/".to_string(),
                    "/Library/Developer/Xcode/Archives/".to_string(),
                    "/Library/Developer/CoreSimulator/Caches/".to_string(),
                ],
                file_patterns: vec!["*.o".to_string(), "*.d".to_string(), "*.dia".to_string()],
                regeneratable: true,
                importance: CacheImportance::Medium,
            },
        );

        cache_signatures.insert(
            "package_manager_cache".to_string(),
            CacheSignature {
                path_patterns: vec![
                    "/.npm/".to_string(),
                    "/Library/Caches/Homebrew/".to_string(),
                    "/.cocoapods/".to_string(),
                    "/Library/Caches/pip/".to_string(),
                    "/go/pkg/mod/cache/".to_string(),
                    "/.cargo/registry/cache/".to_string(),
                ],
                file_patterns: vec![
                    "*.tar.gz".to_string(),
                    "*.tgz".to_string(),
                    "*.zip".to_string(),
                ],
                regeneratable: true,
                importance: CacheImportance::Low,
            },
        );

        Self {
            cache_signatures,
            app_activity_checker: AppActivityChecker::new(),
        }
    }

    pub async fn validate_cache_file(
        &self,
        path: &Path,
        _category: &str,
        process_snapshot: &ProcessSnapshot,
    ) -> CacheValidation {
        let mut validation = CacheValidation {
            is_valid_cache: false,
            confidence: 0.0,
            cache_type: CacheType::Unknown,
            regeneratable: false,
            importance: CacheImportance::Unknown,
            active_app: false,
            last_accessed: None,
            size_bytes: 0,
        };

        // Check if path matches known cache patterns
        let path_str = path.to_string_lossy().to_lowercase();

        for (cache_name, signature) in &self.cache_signatures {
            if signature.matches_path(&path_str) {
                validation.is_valid_cache = true;
                validation.confidence = 0.8;
                validation.cache_type = CacheType::from_name(cache_name);
                validation.regeneratable = signature.regeneratable;
                validation.importance = signature.importance.clone();
                break;
            }
        }

        // Additional validation based on file characteristics
        if let Ok(metadata) = fs::metadata(path) {
            validation.size_bytes = metadata.len();

            if let Ok(accessed) = metadata.accessed() {
                validation.last_accessed = Some(DateTime::<Utc>::from(accessed));
            }

            // Check file headers for cache signatures
            if self.has_cache_headers(path).await {
                validation.confidence = (validation.confidence + 0.2).min(1.0);
                validation.is_valid_cache = true;
            }
        }

        // Check if associated app is active
        validation.active_app = self
            .app_activity_checker
            .is_app_active(path, process_snapshot);

        // Adjust importance based on comprehensive analysis
        validation.importance = self.classify_cache_importance(path, &validation);

        // Analyze modification patterns
        if self.has_cache_modification_pattern(path).await {
            validation.confidence = (validation.confidence + 0.1).min(1.0);
        }

        validation
    }

    pub fn classify_cache_importance(
        &self,
        path: &Path,
        validation: &CacheValidation,
    ) -> CacheImportance {
        let path_str = path.to_string_lossy().to_lowercase();

        // User session caches are more important if recently accessed
        if let Some(last_accessed) = validation.last_accessed {
            let days_since = Utc::now().signed_duration_since(last_accessed).num_days();

            if days_since < 1 && validation.active_app {
                return CacheImportance::Critical;
            } else if days_since < 7 {
                return CacheImportance::High;
            }
        }

        // System caches
        if path_str.contains("/system/library/caches/") {
            return CacheImportance::High;
        }

        // Development tool caches
        if path_str.contains("xcode") || path_str.contains("android") {
            if validation.active_app {
                return CacheImportance::High;
            }
            return CacheImportance::Medium;
        }

        // Package manager caches
        if path_str.contains("homebrew") || path_str.contains("npm") || path_str.contains("pip") {
            return CacheImportance::Low;
        }

        validation.importance.clone()
    }

    async fn has_cache_headers(&self, path: &Path) -> bool {
        if let Ok(mut file) = fs::File::open(path) {
            use std::io::Read;
            let mut header = vec![0u8; 32];
            if file.read(&mut header).is_ok() {
                // Check for SQLite header (common in browser caches)
                if header.starts_with(b"SQLite format 3") {
                    return true;
                }
                // Check for cache database headers
                if header.starts_with(b"CACHE") || header.starts_with(b"cache") {
                    return true;
                }
            }
        }
        false
    }

    async fn has_cache_modification_pattern(&self, path: &Path) -> bool {
        // Check if file has typical cache modification patterns
        // (frequent updates, regular size changes, etc.)
        if let Ok(metadata) = fs::metadata(path) {
            if let (Ok(modified), Ok(created)) = (metadata.modified(), metadata.created()) {
                let modified_time = DateTime::<Utc>::from(modified);
                let created_time = DateTime::<Utc>::from(created);

                // If modified multiple times since creation, likely a cache
                let time_diff = modified_time
                    .signed_duration_since(created_time)
                    .num_hours();

                return time_diff > 24; // Modified after first day
            }
        }
        false
    }
}

/// Checks if apps associated with caches are currently active
pub struct AppActivityChecker {
    app_process_map: HashMap<String, Vec<String>>,
}

impl AppActivityChecker {
    pub fn new() -> Self {
        let mut app_process_map = HashMap::new();

        // Map app identifiers to process names
        app_process_map.insert(
            "xcode".to_string(),
            vec![
                "Xcode".to_string(),
                "xcodebuild".to_string(),
                "swift".to_string(),
                "swiftc".to_string(),
            ],
        );

        app_process_map.insert(
            "safari".to_string(),
            vec!["Safari".to_string(), "com.apple.Safari".to_string()],
        );

        app_process_map.insert(
            "chrome".to_string(),
            vec![
                "Google Chrome".to_string(),
                "Google Chrome Helper".to_string(),
            ],
        );

        app_process_map.insert(
            "node".to_string(),
            vec!["node".to_string(), "npm".to_string(), "yarn".to_string()],
        );

        Self { app_process_map }
    }

    pub fn is_app_active(&self, path: &Path, snapshot: &ProcessSnapshot) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        for (app_key, process_names) in &self.app_process_map {
            if path_str.contains(app_key) {
                if process_names
                    .iter()
                    .any(|name| snapshot.has_process_named(name))
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn get_active_development_tools(&self) -> Vec<String> {
        let mut active_tools = Vec::new();
        let mut system = System::new_all();
        system.refresh_processes();

        let dev_tools = vec![
            "Xcode",
            "xcodebuild",
            "swift",
            "swiftc",
            "node",
            "npm",
            "yarn",
            "pnpm",
            "python",
            "python3",
            "pip",
            "pip3",
            "cargo",
            "rustc",
            "go",
            "golang",
            "gradle",
            "mvn",
            "docker",
            "docker-compose",
        ];

        for tool in &dev_tools {
            if system.processes_by_name(tool).count() > 0 {
                if !active_tools.contains(&tool.to_string()) {
                    active_tools.push(tool.to_string());
                }
            }
        }

        active_tools
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheValidation {
    pub is_valid_cache: bool,
    pub confidence: f32,
    pub cache_type: CacheType,
    pub regeneratable: bool,
    pub importance: CacheImportance,
    pub active_app: bool,
    pub last_accessed: Option<DateTime<Utc>>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CacheType {
    Browser,
    System,
    Application,
    Developer,
    PackageManager,
    Temporary,
    Unknown,
}

impl CacheType {
    fn from_name(name: &str) -> Self {
        match name {
            "browser_cache" => CacheType::Browser,
            "system_cache" => CacheType::System,
            "xcode_cache" => CacheType::Developer,
            "package_manager_cache" => CacheType::PackageManager,
            _ => CacheType::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CacheImportance {
    Critical, // Never auto-delete
    High,     // Requires strong confirmation
    Medium,   // Standard confirmation
    Low,      // Can be auto-selected
    Unknown,
}

#[derive(Debug, Clone)]
struct CacheSignature {
    path_patterns: Vec<String>,
    file_patterns: Vec<String>,
    regeneratable: bool,
    importance: CacheImportance,
}

impl CacheSignature {
    fn matches_path(&self, path: &str) -> bool {
        let path_lc = path.to_lowercase();
        for pattern in &self.path_patterns {
            if path_lc.contains(&pattern.to_lowercase()) {
                return true;
            }
        }
        // Basic suffix matching for common glob-like patterns (e.g., "*.cache", "*.db")
        for pat in &self.file_patterns {
            let pat = pat.trim();
            if let Some(suffix) = pat.strip_prefix("*.") {
                if path_lc.ends_with(&format!(".{}", suffix.to_lowercase())) {
                    return true;
                }
            } else if let Some(suffix) = pat.strip_prefix('*') {
                if path_lc.ends_with(&suffix.to_lowercase()) {
                    return true;
                }
            }
        }
        false
    }
}
