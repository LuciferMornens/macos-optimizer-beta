// src/file_cleaner/engine_utils.rs

#[cfg(feature = "parallel-scan")]
use walkdir::DirEntry;
#[cfg(feature = "parallel-scan")]
use chrono::{DateTime, Duration as ChronoDuration, Utc};
#[cfg(feature = "parallel-scan")]
use super::types::{CategoryRule, CleanableFile};
#[cfg(feature = "parallel-scan")]
use super::safety::{calculate_safety_score, is_safe_to_delete};

#[cfg(feature = "parallel-scan")]
impl super::engine::FileCleaner {
    pub(crate) fn process_entry(&self, entry: &DirEntry, rule: &CategoryRule) -> Option<CleanableFile> {
        let file_path = entry.path();
        let path_str = file_path.to_string_lossy().to_string();
        let path_lower = path_str.to_lowercase();
        
        // Check excludes
        if let Some(ref excludes) = rule.excludes {
            let excludes_lower: Vec<_> = excludes.iter().map(|s| s.to_lowercase()).collect();
            if excludes_lower.iter().any(|exclude| path_lower.contains(exclude)) {
                return None;
            }
        }
        
        // Check require_subpaths
        if let Some(ref subpaths) = rule.require_subpaths {
            if !subpaths.is_empty() {
                let subpaths_lower: Vec<_> = subpaths.iter().map(|s| s.to_lowercase()).collect();
                if !subpaths_lower.iter().any(|sub| path_lower.contains(sub)) {
                    return None;
                }
            }
        }
        
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => return None,
        };
        
        let now = Utc::now();
        let min_age = rule.min_age_days;
        let min_size_bytes = rule.min_size_kb.map(|kb| kb * 1024);
        
        // Check file type and extensions
        if metadata.is_file() {
            if let Some(ref exts) = rule.extensions {
                if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if !exts.iter().any(|e| e.to_lowercase() == ext_lower) {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        }
        
        // Age filter
        if let Some(days) = min_age {
            let relevant_time = if rule.name.to_lowercase().contains("downloads") || 
                                   rule.name.to_lowercase().contains("desktop") {
                metadata.created().ok().map(|t| DateTime::<Utc>::from(t))
            } else {
                metadata.modified().ok().map(|t| DateTime::<Utc>::from(t))
            };
            
            if let Some(file_time) = relevant_time {
                if now.signed_duration_since(file_time) < ChronoDuration::days(days) {
                    return None;
                }
            }
        }
        
        // Size filter
        let size = if metadata.is_dir() {
            self.get_directory_size(file_path).unwrap_or(0)
        } else {
            metadata.len()
        };
        
        if let Some(min) = min_size_bytes {
            if size < min {
                return None;
            }
        }
        
        let last_modified = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t).timestamp())
            .unwrap_or(0);
        
        let is_safe = rule.safe && is_safe_to_delete(file_path);
        let (safety_score, auto_select) = calculate_safety_score(
            file_path,
            &rule.name,
            rule.min_age_days,
            is_safe,
        );
        
        Some(CleanableFile {
            path: path_str,
            size,
            category: rule.name.clone(),
            description: self.get_file_description(file_path, &rule.name),
            last_modified,
            safe_to_delete: is_safe,
            safety_score,
            auto_select,
        })
    }
}
