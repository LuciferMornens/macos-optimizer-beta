// src/file_cleaner/engine_utils.rs

#[cfg(feature = "parallel-scan")]
use super::safety::{assess_path_risk, calculate_safety_score, RiskLevel};
#[cfg(feature = "parallel-scan")]
use super::types::{CategoryRule, CleanableFile};
#[cfg(feature = "parallel-scan")]
use chrono::{DateTime, Duration as ChronoDuration, Utc};
#[cfg(feature = "parallel-scan")]
use walkdir::DirEntry;

#[cfg(feature = "parallel-scan")]
impl super::engine::FileCleaner {
    pub(crate) fn process_entry(
        &self,
        entry: &DirEntry,
        rule: &CategoryRule,
    ) -> Option<CleanableFile> {
        let file_path = entry.path();
        let path_str = file_path.to_string_lossy().to_string();
        let path_lower = path_str.to_lowercase();

        // Check excludes
        if let Some(ref excludes) = rule.excludes {
            let excludes_lower: Vec<_> = excludes.iter().map(|s| s.to_lowercase()).collect();
            if excludes_lower
                .iter()
                .any(|exclude| path_lower.contains(exclude))
            {
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

        let rule_name_lower = rule.name.to_lowercase();
        let is_dir_based_rule = rule_name_lower.contains("folder")
            || rule_name_lower.contains("cache")
            || rule_name_lower.contains("container");

        // Enforce directory/file expectation based on rule intent
        if metadata.is_dir() {
            if !is_dir_based_rule {
                return None;
            }
        } else if is_dir_based_rule {
            // Directory-focused rules shouldn't emit individual files here
            return None;
        }

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
            let relevant_time =
                if rule_name_lower.contains("downloads") || rule_name_lower.contains("desktop") {
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
            self.get_directory_size_blocking(file_path).unwrap_or(0)
        } else {
            super::engine::FileCleaner::metadata_size_bytes(&metadata)
        };
        let threshold_size = if metadata.is_dir() {
            size
        } else {
            metadata.len()
        };

        if let Some(min) = min_size_bytes {
            if threshold_size < min {
                return None;
            }
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
