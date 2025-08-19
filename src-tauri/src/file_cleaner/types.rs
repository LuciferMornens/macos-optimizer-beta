use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CleanableFile {
    pub path: String,
    pub size: u64,
    pub category: String,
    pub description: String,
    pub last_modified: i64,
    pub safe_to_delete: bool,
    pub safety_score: u8,   // 0-100, where 100 is completely safe
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

// -------- Rule Engine Types & Helpers --------

#[derive(Debug, Deserialize)]
pub(crate) struct CleanerRules {
    pub(crate) categories: Vec<CategoryRule>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CategoryRule {
    pub(crate) name: String,
    pub(crate) paths: Vec<String>,
    pub(crate) safe: bool,
    #[allow(dead_code)]
    pub(crate) advanced: Option<bool>,
    pub(crate) max_depth: Option<usize>,
    pub(crate) min_age_days: Option<i64>,
    pub(crate) min_size_kb: Option<u64>,
    pub(crate) excludes: Option<Vec<String>>,
    pub(crate) extensions: Option<Vec<String>>,
    // When set, file path must include at least one of these substrings
    pub(crate) require_subpaths: Option<Vec<String>>,
}

// Load rules with error propagation (for scan_system)
pub(crate) fn load_rules_result() -> Result<CleanerRules, String> {
    let raw = include_str!("../../rules/cleaner_rules.json");
    serde_json::from_str(raw).map_err(|e| format!("Failed to parse cleaner rules: {}", e))
}

// Load rules with default fallback (for reporting)
pub(crate) fn load_rules() -> CleanerRules {
    load_rules_result().unwrap_or(CleanerRules { categories: vec![] })
}