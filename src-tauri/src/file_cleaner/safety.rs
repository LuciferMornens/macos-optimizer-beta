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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Safe,
    Review,
    Risky,
}

#[derive(Debug, Clone)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub confidence: u8,
    pub reasons: Vec<String>,
    pub age_modified_days: Option<i64>,
    pub age_created_days: Option<i64>,
}

fn new_assessment(ctx: &PathContext<'_>) -> RiskAssessment {
    RiskAssessment {
        level: RiskLevel::Review,
        confidence: 45,
        reasons: Vec::new(),
        age_modified_days: ctx.age_days_modified(),
        age_created_days: ctx.age_days_created(),
    }
}

pub(crate) fn assess_path_risk(path: &Path) -> RiskAssessment {
    let ctx = PathContext::new(path);
    let mut assessment = new_assessment(&ctx);

    let in_downloads = ctx.contains_sequence(&["downloads"]);
    let extension = ctx.extension();

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
        || PROTECTED_SEQUENCES
            .iter()
            .any(|sequence| ctx.contains_sequence(sequence))
    {
        assessment.level = RiskLevel::Risky;
        assessment.confidence = 98;
        assessment
            .reasons
            .push("Sensitive or personal location".into());
        return assessment;
    }

    let mut safe_hits: Vec<(u8, String)> = Vec::new();
    let mut review_hits: Vec<String> = Vec::new();

    if ctx.contains_sequence(&[".trash"]) || ctx.ends_with_sequence(&[".trash"]) {
        safe_hits.push((96, "Trash".into()));
    }

    let temp_sequences: &[&[&str]] = &[
        &["tmp"],
        &["var", "tmp"],
        &["private", "tmp"],
        &["private", "var", "tmp"],
        &["library", "caches", "temporaryitems"],
    ];
    if temp_sequences
        .iter()
        .any(|sequence| ctx.contains_sequence(sequence) || ctx.ends_with_sequence(sequence))
    {
        safe_hits.push((90, "Temporary location".into()));
    }

    if ctx.contains_sequence(&["library", "caches"])
        || ctx.contains_sequence(&["library", "containers", "data", "library", "caches"])
        || ctx.contains_sequence(&["library", "group containers", "library", "caches"])
    {
        safe_hits.push((90, "Application cache".into()));
    }

    if ctx.contains_sequence(&["library", "caches", "com.apple.quicklook.thumbnailcache"]) {
        safe_hits.push((92, "QuickLook cache".into()));
    }

    if ctx.contains_sequence(&[".dropbox.cache"]) {
        safe_hits.push((85, "Dropbox cache".into()));
    }

    if ctx.contains_sequence(&["library", "developer", "xcode", "deriveddata"]) {
        safe_hits.push((88, "Xcode derived data".into()));
    }

    if ctx.contains_sequence(&["library", "caches", "homebrew"]) {
        safe_hits.push((86, "Homebrew cache".into()));
    }
    if ctx.contains_sequence(&[".npm", "_cacache"]) {
        safe_hits.push((82, "npm cache".into()));
    }
    if ctx.contains_sequence(&["library", "caches", "pip"]) {
        safe_hits.push((82, "pip cache".into()));
    }
    if ctx.contains_sequence(&["library", "caches", "cocoapods"]) {
        safe_hits.push((82, "CocoaPods cache".into()));
    }
    if ctx.contains_sequence(&["library", "caches", "yarn"]) {
        safe_hits.push((82, "Yarn cache".into()));
    }
    if ctx.contains_sequence(&["library", "caches", "go-build"]) {
        safe_hits.push((82, "Go build cache".into()));
    }

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

    if ctx.contains_sequence(&["library", "application support"])
        && ctx.segment_contains_any(APP_SUPPORT_CACHE_KEYWORDS)
    {
        safe_hits.push((84, "Application support cache".into()));
    }

    let incomplete_exts = ["crdownload", "download", "part", "partial", "tmp", "aria2"];
    if in_downloads {
        if let Some(ext) = extension.as_deref() {
            if incomplete_exts.iter().any(|candidate| candidate == &ext) {
                safe_hits.push((80, "Incomplete download stub".into()));
            } else if matches!(ext, "dmg" | "pkg" | "zip") {
                review_hits.push("Installer in Downloads".into());
            } else {
                review_hits.push("User download".into());
            }
        } else {
            review_hits.push("User download".into());
        }
    }

    if ctx.contains_sequence(&["library", "saved application state"]) {
        review_hits.push("Saved application state".into());
    }
    if ctx.contains_sequence(&["library", "logs"]) {
        review_hits.push("Log files".into());
    }

    if let Some((conf, reason)) = safe_hits.iter().max_by_key(|entry| entry.0) {
        assessment.level = RiskLevel::Safe;
        assessment.confidence = *conf;
        assessment.reasons.push(reason.clone());
    } else if !review_hits.is_empty() {
        assessment.level = RiskLevel::Review;
        assessment.confidence = 55;
        assessment.reasons.extend(review_hits);
    } else {
        assessment.reasons.push("Uncategorised location".into());
    }

    if let Some(age) = assessment.age_modified_days {
        if assessment.level == RiskLevel::Safe
            && age < 1
            && !assessment.reasons.iter().any(|reason| {
                reason.contains("Incomplete download")
                    || reason.contains("Trash")
                    || reason.contains("Temporary")
            })
        {
            assessment.level = RiskLevel::Review;
            assessment.confidence = assessment.confidence.min(60);
            assessment.reasons.push("Recently modified (<24h)".into());
        }
        if assessment.level == RiskLevel::Review && age >= 60 {
            assessment.level = RiskLevel::Safe;
            assessment.confidence = assessment.confidence.max(70);
            assessment.reasons.push("Stale (>60d)".into());
        }
    }

    assessment
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

pub(crate) fn calculate_safety_score(
    path: &Path,
    category: &str,
    risk: &RiskAssessment,
    rule_min_age: Option<i64>,
) -> (u8, bool) {
    let ctx = PathContext::new(path);

    let mut score: i16 = match risk.level {
        RiskLevel::Safe => 80 + (risk.confidence as i16 / 3),
        RiskLevel::Review => 55,
        RiskLevel::Risky => 25,
    };

    let allow_auto = matches!(risk.level, RiskLevel::Safe) && risk.confidence >= 65;
    let mut auto_select = false;

    match category {
        "Trash" => {
            score = 100;
            auto_select = allow_auto;
        }
        "System Cache"
        | "System Cache (Advanced)"
        | "User Cache"
        | "Browser Cache"
        | "App Store Cache"
        | "Music Cache"
        | "Container Caches (Advanced)"
        | "Group Container Caches (Advanced)"
        | "Dropbox Cache"
        | "QuickLook Cache" => {
            score = score.max(92);
            auto_select = allow_auto;
        }
        "App Support Caches (Advanced)" => {
            score = score.max(85);
        }
        "Temporary Files" | "User Temporary Files" | "Container Temp (Advanced)" => {
            score = score.max(90);
            auto_select = allow_auto;
        }
        "Saved Application State (30d+)" => {
            score = score.max(88);
            if rule_min_age.unwrap_or(0) >= 30 {
                auto_select = allow_auto;
            }
        }
        "Incomplete Downloads (2d+)" => {
            score = score.max(88);
            auto_select = allow_auto;
        }
        "User Logs (30d+)"
        | "System Logs (30d+, Advanced)"
        | "Crash Reports (30d+)"
        | "System Crash Reports (30d+, Advanced)" => {
            score = score.max(78);
            if rule_min_age.unwrap_or(0) >= 30 {
                auto_select = allow_auto;
            }
        }
        "Old Downloads" | "Old Downloads (90d+)" | "Old Installers (30d+)" => {
            score = score.max(60);
        }
        "Large Stale Files (Desktop/Downloads)"
        | "Mail Downloads (Review)"
        | "Messages Attachments (90d+, Review)"
        | "iOS Updates (Advanced)"
        | "iOS Backups (Advanced)" => {
            score = score.max(45);
        }
        _ => {}
    }

    if category.to_lowercase().contains("cache") || category.to_lowercase().contains("temp") {
        score = score.max(88);
        if allow_auto {
            auto_select = true;
        }
    }

    if let Some(age) = risk.age_modified_days.or(risk.age_created_days) {
        if matches!(risk.level, RiskLevel::Safe) && age < 2 {
            score -= 15;
            auto_select = false;
        }
        if age > 120 && score >= 70 {
            score += 5;
        }
    }

    if let Some(size) = ctx.size() {
        if size > 500 * 1024 * 1024 {
            auto_select = false;
            if score > 70 {
                score -= 5;
            }
        }
    }

    let path_str = ctx.lower();
    if path_str.contains("backup") || path_str.contains("archive") || path_str.contains("export") {
        score -= 25;
        auto_select = false;
    }

    if path_str.contains(".cache") || path_str.contains("cache/") || path_str.contains("/tmp/") {
        score = score.max(92);
    }

    if !matches!(risk.level, RiskLevel::Safe) {
        auto_select = false;
    }

    let clamped = score.clamp(0, 100) as u8;
    (clamped, auto_select)
}
