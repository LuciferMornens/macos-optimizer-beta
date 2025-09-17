use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SafetyPolicy {
    pub auto_select_threshold: u8,
    pub direct_delete_threshold: u8,
    pub max_auto_select_size: Option<u64>,
}

impl SafetyPolicy {
    const fn disabled() -> Self {
        SafetyPolicy { auto_select_threshold: 255, direct_delete_threshold: 100, max_auto_select_size: None }
    }
}

pub(crate) fn policy_for_category(category: &str) -> SafetyPolicy {
    let c = category.to_lowercase();
    // Always safe/auto buckets
    if c == "trash" {
        return SafetyPolicy { auto_select_threshold: 0, direct_delete_threshold: 95, max_auto_select_size: None };
    }
    if c.contains("cache") || c.contains("temporary files") || c.contains("user temporary files") || c.contains("container temp") {
        return SafetyPolicy { auto_select_threshold: 90, direct_delete_threshold: 95, max_auto_select_size: None };
    }
    if c == "incomplete downloads (2d+)" {
        return SafetyPolicy { auto_select_threshold: 90, direct_delete_threshold: 95, max_auto_select_size: None };
    }
    if c.contains("saved application state") {
        return SafetyPolicy { auto_select_threshold: 90, direct_delete_threshold: 95, max_auto_select_size: None };
    }
    if c.contains("logs") || c.contains("crash reports") {
        return SafetyPolicy { auto_select_threshold: 80, direct_delete_threshold: 95, max_auto_select_size: None };
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
    // Be conservative: only return true for well-known safe locations/types.
    // Everything else requires explicit user review (return false).
    let path_str = path.to_string_lossy().to_lowercase();

    // Quick denylist for clearly sensitive locations (avoid overly broad patterns)
    let protected_patterns = vec![
        ".ssh",
        ".gnupg",
        ".keychain",
        "passwords",
        "credentials",
        ".env",
        "/config/",
        "/preferences/",
        ".git/",
        "/node_modules/", // let user decide about node_modules separately
        "/documents/",
        "/desktop/",
        "/pictures/",
        "/movies/",
        "/music/",
        "/photos/",
        ".pem",
        ".key",
        ".cert",
        ".p12",
        "wallet",
        "vault",
        "/backup/",
        "/archives/",
        "/archive/",
        "important",
        "personal",
        "private",
        "secret",
        ".localized",
        // NOTE: Removed "library/application support" (too broad — hides many legit caches).
        "library/preferences",
        "library/keychains",
        "library/accounts",
        "library/cookies",
        "library/mail",
        "library/messages",
        "library/safari",
    ];
    for pat in protected_patterns {
        if path_str.contains(pat) {
            return false;
        }
    }

    // Helper: compute file age in days (by modified time)
    let file_age_days = || -> Option<i64> {
        if let Ok(md) = fs::metadata(path) {
            if let Ok(modified) = md.modified() {
                let modified_time = DateTime::<Utc>::from(modified);
                return Some(
                    Utc::now()
                        .signed_duration_since(modified_time)
                        .num_days(),
                );
            }
        }
        None
    };

    // Helper: compute file age in days using creation time if available
    let file_age_days_created = || -> Option<i64> {
        if let Ok(md) = fs::metadata(path) {
            if let Ok(created) = md.created() {
                let created_time = DateTime::<Utc>::from(created);
                return Some(
                    Utc::now()
                        .signed_duration_since(created_time)
                        .num_days(),
                );
            }
        }
        None
    };

    // Helper: string-based path tests with clear boundaries
    let is_in_trash = path_str.contains("/.trash/") || path_str.ends_with("/.trash");
    let is_tmp = path_str.starts_with("/tmp/")
        || path_str == "/tmp"
        || path_str.starts_with("/var/tmp/")
        || path_str == "/var/tmp"
        || path_str.starts_with("/private/tmp/")
        || path_str == "/private/tmp"
        || path_str.starts_with("/private/var/tmp/")
        || path_str == "/private/var/tmp"
        || path_str.contains("/library/caches/temporaryitems")
        || path_str.ends_with("/library/caches/temporaryitems");

    // Explicit allow for Library/Caches and system caches paths
    let is_library_caches =
        path_str.contains("/library/caches/") || path_str.ends_with("/library/caches");

    // Explicit allow for Containers/Group Containers caches and tmp
    let is_container_caches = (path_str.contains("/library/containers/")
        || path_str.contains("/library/group containers/"))
        && (path_str.contains("/library/caches/")
            || path_str.ends_with("/library/caches")
            || path_str.contains("/tmp/")
            || path_str.ends_with("/tmp"));

    // QuickLook thumbnail cache
    let is_quicklook_cache =
        path_str.contains("/library/caches/com.apple.quicklook.thumbnailcache");

    // App Store / Music caches
    let is_specific_caches = path_str.contains("/library/caches/com.apple.appstore")
        || path_str.contains("/library/caches/com.apple.music");

    // Dropbox cache
    let is_dropbox_cache =
        path_str.contains("/.dropbox.cache/") || path_str.ends_with("/.dropbox.cache");

    // Saved Application State: allow if older than 30 days
    let is_saved_state = path_str.contains("/library/saved application state/")
        || path_str.ends_with("/library/saved application state");

    // DiagnosticReports and generic Logs: allow only when older than 30 days
    let is_diag_reports = path_str.contains("/library/logs/diagnosticreports/")
        || path_str.ends_with("/library/logs/diagnosticreports")
        || path_str.contains("/library/diagnosticreports/")
        || path_str.ends_with("/library/diagnosticreports");
    let is_logs_path = path_str.contains("/logs/") || path_str.ends_with("/logs");

    // Incomplete downloads within Downloads
    let mut is_incomplete_download = false;
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        let in_downloads = path_str.contains("/downloads/") || path_str.ends_with("/downloads");
        let incomplete_exts = ["crdownload", "download", "part", "partial", "tmp", "aria2"];
        if in_downloads && incomplete_exts.iter().any(|e| e == &ext) {
            if let Some(d) = file_age_days() {
                if d >= 2 {
                    is_incomplete_download = true;
                }
            }
        }
    }

    // iOS Updates (.ipsw) in iTunes updates directories
    let mut is_ios_update = false;
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        if ext == "ipsw"
            && (path_str.contains("/library/itunes/iphone software updates/")
                || path_str.contains("/library/itunes/ipad software updates/"))
        {
            if let Some(d) = file_age_days() {
                if d >= 30 {
                    is_ios_update = true;
                }
            }
        }
    }

    // Old installers in Downloads: only .dmg/.pkg, 30d+ (creation time preferred)
    let mut is_old_installer = false;
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        let in_downloads = path_str.contains("/downloads/") || path_str.ends_with("/downloads");
        if in_downloads && (ext == "dmg" || ext == "pkg") {
            if let Some(d) = file_age_days_created().or_else(|| file_age_days()) {
                if d >= 30 {
                    is_old_installer = true;
                }
            }
        }
    }

    // Additional developer & package-manager caches (commonly safe)
    let is_xcode_deriveddata = path_str.contains("/library/developer/xcode/deriveddata");
    let is_xcode_devicesupport = path_str.contains("/library/developer/xcode/ios devicesupport");
    let is_coresim = path_str.contains("/library/developer/coresimulator/");
    let is_coresim_cachey = is_coresim
        && (path_str.contains("/library/caches/")
            || path_str.contains("/tmp/")
            || path_str.contains("/cache"));
    let is_homebrew_cache = path_str.contains("/library/caches/homebrew");
    let is_npm_cache = path_str.contains("/.npm/_cacache");
    let is_pip_cache = path_str.contains("/library/caches/pip");
    let is_cocoapods_cache = path_str.contains("/library/caches/cocoapods");
    let is_yarn_cache = path_str.contains("/library/caches/yarn");
    let is_go_build_cache = path_str.contains("/library/caches/go-build");

    // Cache-like storage under Application Support (e.g., Slack/Service Worker/CacheStorage)
    let is_app_support_cache_like = path_str.contains("/library/application support/")
        && (path_str.contains("/cache")
            || path_str.contains("/caches")
            || path_str.contains("cachestorage")
            || path_str.contains("/tmp")
            || path_str.contains("/temporary")
            || path_str.contains("/temp"));

    // Age-gated allowances
    let xcode_devicesupport_old = is_xcode_devicesupport
        && file_age_days().map(|d| d >= 90).unwrap_or(false);
    let coresim_logs_old = (is_coresim && (path_str.contains("/logs/") || path_str.ends_with("/logs")))
        && file_age_days().map(|d| d >= 30).unwrap_or(false);

    // Decide: allow only known-safe scenarios
    if is_in_trash
        || is_tmp
        || is_library_caches
        || is_container_caches
        || is_quicklook_cache
        || is_specific_caches
        || is_dropbox_cache
        || (is_saved_state && file_age_days().map(|d| d >= 30).unwrap_or(false))
        || (is_diag_reports && file_age_days().map(|d| d >= 30).unwrap_or(false))
        || (is_logs_path && file_age_days().map(|d| d >= 30).unwrap_or(false))
        || is_incomplete_download
        || is_ios_update
        || is_old_installer
        || is_homebrew_cache
        || is_npm_cache
        || is_pip_cache
        || is_cocoapods_cache
        || is_yarn_cache
        || is_go_build_cache
        || is_xcode_deriveddata
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
        "Large Stale Files (Desktop/Downloads)" | "Mail Downloads (Review)" | "Messages Attachments (90d+, Review)" => {
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
    if let Ok(metadata) = fs::metadata(path) {
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
    if let Ok(metadata) = fs::metadata(path) {
        let size = metadata.len();
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
    let path_str = path.to_string_lossy().to_lowercase();

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