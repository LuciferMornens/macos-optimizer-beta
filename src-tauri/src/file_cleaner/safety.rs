use chrono::{DateTime, Utc};
use std::fs::{self, Metadata};
use std::path::Path;

use super::types::CleanableFile;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SafetyPolicy {
    pub auto_select_threshold: u8,
    pub direct_delete_threshold: u8,
    pub max_auto_select_size: Option<u64>,
}

impl SafetyPolicy {
    const fn disabled() -> Self {
        SafetyPolicy {
            auto_select_threshold: 255,
            direct_delete_threshold: 100,
            max_auto_select_size: None,
        }
    }

    pub(crate) fn enforce(&self, file: &mut CleanableFile) {
        if self.auto_select_threshold < 255 {
            let meets_threshold = file.safety_score >= self.auto_select_threshold;
            file.auto_select = file.auto_select && meets_threshold;
        } else {
            file.auto_select = false;
        }

        if file.safe_to_delete && file.safety_score < self.direct_delete_threshold {
            file.safe_to_delete = false;
        }

        if let Some(max_size) = self.max_auto_select_size {
            if file.auto_select && file.size > max_size {
                file.auto_select = false;
            }
        }
    }
}

#[derive(Debug)]
struct PathContext<'a> {
    path: &'a Path,
    lower: String,
    segments: Vec<String>,
    metadata: Option<Metadata>,
}

impl<'a> PathContext<'a> {
    fn new(path: &'a Path) -> Self {
        let lower = path.to_string_lossy().to_lowercase();
        let segments = lower
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();
        let metadata = fs::metadata(path).ok();

        Self {
            path,
            lower,
            segments,
            metadata,
        }
    }

    fn lower(&self) -> &str {
        &self.lower
    }

    fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }

    fn size(&self) -> Option<u64> {
        self.metadata.as_ref().map(|m| m.len())
    }

    fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
    }

    fn segment_contains_any(&self, keywords: &[&str]) -> bool {
        self.segments
            .iter()
            .any(|segment| keywords.iter().any(|keyword| segment.contains(keyword)))
    }

    fn contains_keyword(&self, keyword: &str) -> bool {
        self.lower.contains(keyword)
    }

    fn contains_sequence(&self, sequence: &[&str]) -> bool {
        if sequence.is_empty() {
            return true;
        }
        let target_len = sequence.len();
        if target_len > self.segments.len() {
            return false;
        }

        self.segments
            .windows(target_len)
            .any(|window| Self::sequence_matches(window, sequence))
    }

    fn ends_with_sequence(&self, sequence: &[&str]) -> bool {
        if sequence.is_empty() {
            return true;
        }
        let target_len = sequence.len();
        if target_len > self.segments.len() {
            return false;
        }

        let start = self.segments.len() - target_len;
        Self::sequence_matches(&self.segments[start..], sequence)
    }

    fn age_days_modified(&self) -> Option<i64> {
        self.metadata
            .as_ref()
            .and_then(|md| md.modified().ok())
            .map(|modified| {
                let modified_time = DateTime::<Utc>::from(modified);
                Utc::now().signed_duration_since(modified_time).num_days()
            })
    }

    fn age_days_created(&self) -> Option<i64> {
        self.metadata
            .as_ref()
            .and_then(|md| md.created().ok())
            .map(|created| {
                let created_time = DateTime::<Utc>::from(created);
                Utc::now().signed_duration_since(created_time).num_days()
            })
    }

    fn sequence_matches(window: &[String], sequence: &[&str]) -> bool {
        window
            .iter()
            .zip(sequence.iter())
            .all(|(segment, pattern)| *pattern == "*" || segment == pattern)
    }
}

pub(crate) fn policy_for_category(category: &str) -> SafetyPolicy {
    let c = category.to_lowercase();
    // Always safe/auto buckets
    if c == "trash" {
        return SafetyPolicy {
            auto_select_threshold: 0,
            direct_delete_threshold: 95,
            max_auto_select_size: None,
        };
    }
    if c.contains("cache")
        || c.contains("temporary files")
        || c.contains("user temporary files")
        || c.contains("container temp")
    {
        return SafetyPolicy {
            auto_select_threshold: 90,
            direct_delete_threshold: 95,
            max_auto_select_size: None,
        };
    }
    if c == "incomplete downloads (2d+)" {
        return SafetyPolicy {
            auto_select_threshold: 90,
            direct_delete_threshold: 95,
            max_auto_select_size: None,
        };
    }
    if c.contains("saved application state") {
        return SafetyPolicy {
            auto_select_threshold: 90,
            direct_delete_threshold: 95,
            max_auto_select_size: None,
        };
    }
    if c.contains("logs") || c.contains("crash reports") {
        return SafetyPolicy {
            auto_select_threshold: 80,
            direct_delete_threshold: 95,
            max_auto_select_size: None,
        };
    }
    // Review-only buckets: never auto-select by policy
    if c.contains("old downloads")
        || c.contains("large stale files")
        || c.contains("mail downloads")
        || c.contains("messages attachments")
        || c.contains("ios updates")
        || c.contains("ios backups")
        || c.contains("app support caches (advanced)")
    {
        return SafetyPolicy::disabled();
    }
    // Default: conservative, no auto-select via policy
    SafetyPolicy::disabled()
}

pub(crate) fn is_safe_to_delete(path: &Path) -> bool {
    let ctx = PathContext::new(path);

    let age_modified = ctx.age_days_modified();
    let age_created = ctx.age_days_created();
    let age_for_downloads = age_created.or(age_modified);
    let in_downloads = ctx.contains_sequence(&["downloads"]);
    let extension = ctx.extension();

    // High confidence allow-list checks (safe regardless of deny keywords)
    let is_in_trash = ctx.contains_sequence(&[".trash"]) || ctx.ends_with_sequence(&[".trash"]);
    if is_in_trash {
        return true;
    }

    let temp_sequences: &[&[&str]] = &[
        &["tmp"],
        &["var", "tmp"],
        &["private", "tmp"],
        &["private", "var", "tmp"],
        &["library", "caches", "temporaryitems"],
    ];
    let is_tmp = temp_sequences
        .iter()
        .any(|sequence| ctx.contains_sequence(sequence) || ctx.ends_with_sequence(sequence));
    if is_tmp {
        return true;
    }

    let is_library_caches = ctx.contains_sequence(&["library", "caches"]);
    let is_container_root = ctx.contains_sequence(&["library", "containers"])
        || ctx.contains_sequence(&["library", "group containers"]);
    let is_container_caches = is_container_root
        && (ctx.contains_sequence(&["library", "caches"])
            || ctx.segment_contains_any(&["cache", "tmp", "temp", "temporary"]));
    if is_library_caches || is_container_caches {
        return true;
    }

    let is_quicklook_cache =
        ctx.contains_sequence(&["library", "caches", "com.apple.quicklook.thumbnailcache"]);
    if is_quicklook_cache {
        return true;
    }

    let is_app_store_cache = ctx.contains_sequence(&["library", "caches", "com.apple.appstore"]);
    let is_music_cache = ctx.contains_sequence(&["library", "caches", "com.apple.music"]);
    if is_app_store_cache || is_music_cache {
        return true;
    }

    let is_dropbox_cache = ctx.contains_sequence(&[".dropbox.cache"]);
    if is_dropbox_cache {
        return true;
    }

    let is_xcode_deriveddata =
        ctx.contains_sequence(&["library", "developer", "xcode", "deriveddata"]);
    if is_xcode_deriveddata {
        return true;
    }

    let is_homebrew_cache = ctx.contains_sequence(&["library", "caches", "homebrew"]);
    let is_npm_cache = ctx.contains_sequence(&[".npm", "_cacache"]);
    let is_pip_cache = ctx.contains_sequence(&["library", "caches", "pip"]);
    let is_cocoapods_cache = ctx.contains_sequence(&["library", "caches", "cocoapods"]);
    let is_yarn_cache = ctx.contains_sequence(&["library", "caches", "yarn"]);
    let is_go_build_cache = ctx.contains_sequence(&["library", "caches", "go-build"]);
    if is_homebrew_cache
        || is_npm_cache
        || is_pip_cache
        || is_cocoapods_cache
        || is_yarn_cache
        || is_go_build_cache
    {
        return true;
    }

    // Deny sensitive patterns before evaluating age-based allowances
    const PROTECTED_KEYWORDS: &[&str] = &[
        ".ssh",
        ".gnupg",
        ".keychain",
        "passwords",
        "credentials",
        ".env",
        "/config/",
        ".git/",
        ".pem",
        ".key",
        ".cert",
        ".p12",
        "wallet",
        "vault",
        "important",
        "personal",
        "secret",
    ];

    const PROTECTED_SEQUENCES: &[&[&str]] = &[
        &["documents"],
        &["desktop"],
        &["pictures"],
        &["movies"],
        &["music"],
        &["photos"],
        &["node_modules"],
        &["library", "preferences"],
        &["library", "keychains"],
        &["library", "accounts"],
        &["library", "cookies"],
        &["library", "mail"],
        &["library", "messages"],
        &["library", "safari"],
    ];

    if PROTECTED_KEYWORDS
        .iter()
        .any(|keyword| ctx.contains_keyword(keyword))
    {
        return false;
    }

    if PROTECTED_SEQUENCES
        .iter()
        .any(|sequence| ctx.contains_sequence(sequence))
    {
        return false;
    }

    // Age-gated and contextual allowances
    let is_saved_state = ctx.contains_sequence(&["library", "saved application state"]);
    let saved_state_old = is_saved_state && age_modified.map(|days| days >= 30).unwrap_or(false);

    let is_diag_reports = ctx.contains_sequence(&["library", "logs", "diagnosticreports"])
        || ctx.contains_sequence(&["library", "diagnosticreports"]);
    let diag_reports_old = is_diag_reports && age_modified.map(|days| days >= 30).unwrap_or(false);

    let is_logs_path = ctx.contains_sequence(&["logs"]);
    let logs_old = is_logs_path && age_modified.map(|days| days >= 30).unwrap_or(false);

    let incomplete_exts = ["crdownload", "download", "part", "partial", "tmp", "aria2"];
    let is_incomplete_download = in_downloads
        && extension
            .as_deref()
            .map(|ext| incomplete_exts.iter().any(|candidate| candidate == &ext))
            .unwrap_or(false)
        && age_modified.map(|days| days >= 2).unwrap_or(false);

    let is_ios_update = extension
        .as_deref()
        .map(|ext| ext == "ipsw")
        .unwrap_or(false)
        && (ctx.contains_sequence(&["library", "itunes", "iphone software updates"])
            || ctx.contains_sequence(&["library", "itunes", "ipad software updates"]))
        && age_modified.map(|days| days >= 30).unwrap_or(false);

    let is_old_installer = in_downloads
        && extension
            .as_deref()
            .map(|ext| matches!(ext, "dmg" | "pkg" | "zip"))
            .unwrap_or(false)
        && age_for_downloads.map(|days| days >= 30).unwrap_or(false);

    let is_xcode_devicesupport =
        ctx.contains_sequence(&["library", "developer", "xcode", "ios devicesupport"]);
    let xcode_devicesupport_old =
        is_xcode_devicesupport && age_modified.map(|days| days >= 90).unwrap_or(false);

    let is_coresim = ctx.contains_sequence(&["library", "developer", "coresimulator"]);
    let is_coresim_cachey = is_coresim
        && (ctx.contains_sequence(&["logs"])
            && age_modified.map(|days| days >= 30).unwrap_or(false)
            || ctx.segment_contains_any(&["cache", "tmp", "temp", "dlog"]));
    let coresim_logs_old = is_coresim
        && ctx.contains_sequence(&["logs"])
        && age_modified.map(|days| days >= 30).unwrap_or(false);

    const APP_SUPPORT_CACHE_KEYWORDS: &[&str] = &[
        "cache",
        "caches",
        "cachestorage",
        "code cache",
        "gpu",
        "shadercache",
        "tmp",
        "temp",
        "temporary",
        "dawncache",
    ];

    let is_app_support_cache_like = ctx.contains_sequence(&["library", "application support"])
        && ctx.segment_contains_any(APP_SUPPORT_CACHE_KEYWORDS);

    if saved_state_old
        || diag_reports_old
        || logs_old
        || is_incomplete_download
        || is_ios_update
        || is_old_installer
        || xcode_devicesupport_old
        || is_coresim_cachey
        || coresim_logs_old
        || is_app_support_cache_like
    {
        return true;
    }

    false
}

pub(crate) fn calculate_safety_score(
    path: &Path,
    category: &str,
    days_old: Option<i64>,
    is_safe: bool,
) -> (u8, bool) {
    if !is_safe {
        return (0, false);
    }

    let ctx = PathContext::new(path);
    let mut score: u8;
    let mut auto_select = false;

    // Category-based scoring
    match category {
        "Trash" => {
            score = 100;
            auto_select = true; // Always auto-select trash
        }
        "System Cache"
        | "System Cache (Advanced)"
        | "User Cache"
        | "Browser Cache"
        | "App Store Cache"
        | "Music Cache"
        | "Container Caches (Advanced)"
        | "Group Container Caches (Advanced)"
        | "Dropbox Cache" => {
            score = 95;
            auto_select = true; // Caches are very safe to delete
        }
        "App Support Caches (Advanced)" => {
            score = 85;
            auto_select = false; // Be conservative: review before auto-select
        }
        "Temporary Files" | "User Temporary Files" | "Container Temp (Advanced)" => {
            score = 90;
            auto_select = true; // Temp files are safe
        }
        "Saved Application State (30d+)" => {
            score = 90;
            // Old saved states are safe
            if let Some(days) = days_old {
                if days >= 30 {
                    auto_select = true;
                }
            }
        }
        "Incomplete Downloads (2d+)" => {
            score = 95;
            auto_select = true; // Incomplete downloads with min age are safe
        }
        "User Logs (30d+)"
        | "System Logs (30d+, Advanced)"
        | "Crash Reports (30d+)"
        | "System Crash Reports (30d+, Advanced)" => {
            score = 80;
            // Only auto-select old logs
            if let Some(days) = days_old {
                if days >= 30 {
                    auto_select = true;
                }
            }
        }
        "Old Downloads" | "Old Downloads (90d+)" | "Old Installers (30d+)" => {
            score = 60;
            // Don't auto-select downloads, user should review
            auto_select = false;
        }
        "Large Stale Files (Desktop/Downloads)"
        | "Mail Downloads (Review)"
        | "Messages Attachments (90d+, Review)" => {
            score = 50;
            auto_select = false; // Review before deleting
        }
        "iOS Updates (Advanced)" => {
            score = 50;
            auto_select = false;
        }
        "iOS Backups (Advanced)" => {
            score = 40;
            auto_select = false;
        }
        _ => {
            score = 40;
            auto_select = false;
        }
    }

    // Boost caches/temp even if category name isn't one of the explicit matches
    let cat_lower = category.to_lowercase();
    if is_safe && (cat_lower.contains("cache") || cat_lower.contains("temp")) {
        // Ensure a high score for safe cache/temp buckets
        score = score.max(90);
        // Prefer auto-select for generic cache/temp categories
        auto_select = true;
    }

    // File age adjustment
    if let Some(metadata) = ctx.metadata() {
        // Prefer creation time for Downloads/Desktop-related categories; fallback to modified time
        let age_days_opt = (|| {
            let name_lower = category.to_lowercase();
            if name_lower.contains("downloads") || name_lower.contains("desktop") {
                if let Ok(created) = metadata.created() {
                    let created_time = DateTime::<Utc>::from(created);
                    return Some(Utc::now().signed_duration_since(created_time).num_days());
                }
            }
            if let Ok(modified) = metadata.modified() {
                let modified_time = DateTime::<Utc>::from(modified);
                return Some(Utc::now().signed_duration_since(modified_time).num_days());
            }
            None
        })();

        if let Some(age_days) = age_days_opt {
            // Increase safety for very old files in safe categories
            if age_days > 90 && score >= 80 {
                // cap at 100
                score = score.min(100);
                if category != "Old Downloads" && category != "Xcode Archives" {
                    auto_select = true;
                }
            }

            // Decrease safety for recently modified files (but do NOT penalize incomplete downloads)
            if age_days < 7 && score > 50 {
                let name_lower = category.to_lowercase();
                if name_lower != "incomplete downloads (2d+)" {
                    score = score.saturating_sub(20);
                    auto_select = false;
                }
            }
        }
    }

    // File size adjustment for auto-select
    if let Some(size) = ctx.size() {
        // Don't auto-select very large files (>500MB) unless they're in clearly safe buckets
        // Exempt incomplete downloads bucket
        if size > 500 * 1024 * 1024
            && !matches!(
                category,
                "Trash"
                    | "System Cache"
                    | "System Cache (Advanced)"
                    | "User Cache"
                    | "Browser Cache"
                    | "Temporary Files"
                    | "User Temporary Files"
                    | "QuickLook Cache"
                    | "Dropbox Cache"
                    | "Container Caches (Advanced)"
                    | "Container Temp (Advanced)"
                    | "Group Container Caches (Advanced)"
                    | "Incomplete Downloads (2d+)"
            )
        {
            auto_select = false;
        }
    }

    // Path-based adjustments
    let path_str = ctx.lower();

    // Increase safety for known safe patterns (FIX: use max, not min)
    if path_str.contains(".cache") || path_str.contains("cache/") || path_str.contains("/tmp/") {
        score = score.max(95);
    }

    // Decrease safety for patterns that might be important
    if path_str.contains("backup") || path_str.contains("archive") || path_str.contains("export") {
        score = score.saturating_sub(30);
        auto_select = false;
    }

    (score, auto_select)
}
