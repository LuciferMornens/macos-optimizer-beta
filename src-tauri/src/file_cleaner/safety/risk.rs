use std::path::Path;

use super::context::PathContext;

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

#[derive(Debug, Clone)]
struct Signal {
    confidence: u8,
    reason: String,
}

impl Signal {
    fn new(confidence: u8, reason: impl Into<String>) -> Self {
        Signal {
            confidence,
            reason: reason.into(),
        }
    }
}

fn push_signal(target: &mut Vec<Signal>, confidence: u8, reason: impl Into<String>) {
    let candidate = reason.into();
    if let Some(existing) = target
        .iter_mut()
        .find(|signal| signal.reason.as_str() == candidate.as_str())
    {
        existing.confidence = existing.confidence.max(confidence);
    } else {
        target.push(Signal::new(confidence, candidate));
    }
}

fn max_confidence(signals: &[Signal]) -> u8 {
    signals
        .iter()
        .map(|signal| signal.confidence)
        .max()
        .unwrap_or(0)
}

fn collect_reasons(signals: &[Signal]) -> Vec<String> {
    let mut reasons: Vec<String> = Vec::new();
    for signal in signals {
        if !reasons
            .iter()
            .any(|existing| existing.as_str() == signal.reason.as_str())
        {
            reasons.push(signal.reason.clone());
        }
    }
    reasons
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
    let relevant_age = assessment.age_modified_days.or(assessment.age_created_days);

    let mut safe_signals: Vec<Signal> = Vec::new();
    let mut review_signals: Vec<Signal> = Vec::new();
    let mut risky_signals: Vec<Signal> = Vec::new();

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

    if ctx.contains_sequence(&["system", "library"]) && ctx.contains_keyword("/frameworks/") {
        push_signal(&mut risky_signals, 95, "System framework or component");
    }
    if ctx.contains_sequence(&["library", "preferences"]) {
        push_signal(
            &mut risky_signals,
            94,
            "Application preferences/configuration",
        );
    }

    const BACKUP_KEYWORDS: &[&str] = &[
        "backup",
        "backups",
        "time machine",
        "timemachine",
        "mobilesync",
    ];
    if ctx.segment_contains_any(BACKUP_KEYWORDS)
        || ctx.contains_sequence(&["library", "application support", "mobile sync", "backup"])
        || ctx.contains_sequence(&["library", "itunes", "iphone backups"])
    {
        push_signal(&mut risky_signals, 92, "Backup archive or device snapshot");
    }

    if ctx.contains_sequence(&[".trash"]) || ctx.ends_with_sequence(&[".trash"]) {
        push_signal(
            &mut safe_signals,
            97,
            "Trash (macOS will empty automatically)",
        );
    }

    const TEMP_SEQUENCES: &[(&[&str], &str, u8)] = &[
        (&["tmp"], "Temporary working directory", 92),
        (&["var", "tmp"], "Temporary working directory", 90),
        (
            &["private", "var", "tmp"],
            "Temporary working directory",
            90,
        ),
        (
            &["library", "caches", "temporaryitems"],
            "Temporary items cache",
            90,
        ),
    ];
    for (sequence, reason, confidence) in TEMP_SEQUENCES {
        if ctx.contains_sequence(sequence) || ctx.ends_with_sequence(sequence) {
            push_signal(&mut safe_signals, *confidence, *reason);
        }
    }

    const TEMP_KEYWORDS: &[&str] = &["tmp", "temp", "temporaryitems", "cache.delete"];
    if ctx.segment_contains_any(TEMP_KEYWORDS) {
        push_signal(&mut safe_signals, 88, "Temporary scratch data");
    }

    if ctx.contains_sequence(&["library", "caches"]) {
        let detail = describe_cache_owner(&ctx).unwrap_or_else(|| "Application".into());
        push_signal(
            &mut safe_signals,
            94,
            format!("Application cache ({detail})"),
        );
    }
    if ctx.contains_sequence(&["library", "containers", "*", "data", "library", "caches"]) {
        if let Some(owner) = describe_container_owner(&ctx) {
            push_signal(
                &mut safe_signals,
                93,
                format!("Sandbox container cache ({owner})"),
            );
        }
    }
    if ctx.contains_sequence(&["library", "group containers", "*", "library", "caches"]) {
        if let Some(owner) = describe_group_container_owner(&ctx) {
            push_signal(
                &mut safe_signals,
                93,
                format!("Shared container cache ({owner})"),
            );
        }
    }
    if ctx.contains_sequence(&["library", "caches", "com.apple.quicklook.thumbnailcache"]) {
        push_signal(&mut safe_signals, 97, "QuickLook thumbnail cache");
    }
    if ctx.contains_sequence(&[".dropbox.cache"]) {
        push_signal(&mut safe_signals, 90, "Dropbox sync cache");
    }
    if ctx.contains_sequence(&["library", "developer", "xcode", "deriveddata"]) {
        push_signal(&mut safe_signals, 92, "Xcode DerivedData cache");
    }
    if ctx.contains_sequence(&["library", "caches", "homebrew"]) {
        push_signal(&mut safe_signals, 90, "Homebrew download cache");
    }
    if ctx.contains_sequence(&[".npm", "_cacache"]) {
        push_signal(&mut safe_signals, 88, "npm package cache");
    }
    if ctx.contains_sequence(&["library", "caches", "pip"]) {
        push_signal(&mut safe_signals, 88, "pip package cache");
    }
    if ctx.contains_sequence(&["library", "caches", "cocoapods"]) {
        push_signal(&mut safe_signals, 88, "CocoaPods cache");
    }
    if ctx.contains_sequence(&["library", "caches", "yarn"]) {
        push_signal(&mut safe_signals, 88, "Yarn package cache");
    }
    if ctx.contains_sequence(&["library", "caches", "go-build"]) {
        push_signal(&mut safe_signals, 88, "Go build cache");
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
        let owner =
            describe_application_support_owner(&ctx).unwrap_or_else(|| "Application".into());
        push_signal(
            &mut safe_signals,
            90,
            format!("Application support cache ({owner})"),
        );
    }

    if ctx.segment_contains_any(&[
        "cachestorage",
        "code cache",
        "shadercache",
        "gpucache",
        "webrtc",
    ]) {
        push_signal(&mut safe_signals, 89, "Browser/service worker cache");
    }

    let incomplete_exts = ["crdownload", "download", "part", "partial", "tmp", "aria2"];
    if in_downloads {
        if let Some(ext) = extension.as_deref() {
            if incomplete_exts.iter().any(|candidate| candidate == &ext) {
                push_signal(&mut safe_signals, 86, "Incomplete download stub");
            } else if matches!(ext, "dmg" | "pkg" | "zip" | "ipsw") {
                push_signal(&mut review_signals, 70, "Installer or archive in Downloads");
            } else {
                push_signal(&mut review_signals, 65, "User download");
            }
        } else {
            push_signal(&mut review_signals, 60, "User download");
        }
    }

    if ctx.contains_sequence(&["library", "saved application state"]) {
        if relevant_age.unwrap_or(0) >= 7 {
            push_signal(
                &mut safe_signals,
                86,
                "Saved application state (unused >7d)",
            );
        } else {
            push_signal(&mut review_signals, 65, "Saved application state");
        }
    }

    if ctx.contains_sequence(&["library", "logs"]) {
        if relevant_age.unwrap_or(0) >= 30 {
            push_signal(&mut safe_signals, 84, "Log files (>30d old)");
        } else {
            push_signal(&mut review_signals, 60, "Recent log files");
        }
    }

    if let Some(ext) = extension.as_deref() {
        if matches!(
            ext,
            "log" | "trace" | "crash" | "ips" | "ips1" | "ips2" | "diag" | "dmp"
        ) {
            if relevant_age.unwrap_or(0) >= 30 {
                push_signal(&mut safe_signals, 82, "Crash/log report (>30d old)");
            } else {
                push_signal(&mut review_signals, 60, "Recent crash/log report");
            }
        }
    }

    if ctx.contains_sequence(&["library", "mail", "downloads"]) {
        push_signal(&mut review_signals, 60, "Mail attachment cache");
    }
    if ctx.contains_sequence(&["library", "messages", "attachments"]) {
        push_signal(&mut review_signals, 55, "Messages attachments");
    }

    if !risky_signals.is_empty() {
        assessment.level = RiskLevel::Risky;
        assessment.confidence = max_confidence(&risky_signals).max(85);
        assessment.reasons = collect_reasons(&risky_signals);
        return assessment;
    }

    if !safe_signals.is_empty() {
        assessment.level = RiskLevel::Safe;
        assessment.confidence = max_confidence(&safe_signals).max(70);
        assessment.reasons = collect_reasons(&safe_signals);
    } else if !review_signals.is_empty() {
        assessment.level = RiskLevel::Review;
        assessment.confidence = max_confidence(&review_signals).max(55);
        assessment.reasons = collect_reasons(&review_signals);
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
                    || reason.contains("Cache")
                    || reason.contains("Log")
            })
        {
            assessment.level = RiskLevel::Review;
            assessment.confidence = assessment.confidence.min(60);
            push_signal(&mut review_signals, 55, "Recently modified (<24h)");
            let mut combined = collect_reasons(&review_signals);
            for reason in combined.drain(..) {
                if !assessment
                    .reasons
                    .iter()
                    .any(|existing| existing.as_str() == reason.as_str())
                {
                    assessment.reasons.push(reason);
                }
            }
        }
        if assessment.level == RiskLevel::Review && age >= 60 {
            assessment.level = RiskLevel::Safe;
            assessment.confidence = assessment.confidence.max(72);
            if !assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("Stale (>60d)"))
            {
                assessment.reasons.push("Stale (>60d)".into());
            }
        }
    }

    if assessment.reasons.is_empty() {
        assessment.reasons.push("Uncategorised location".into());
    }

    assessment
}

fn describe_container_owner(ctx: &PathContext<'_>) -> Option<String> {
    let idx = ctx
        .segments_lower()
        .iter()
        .position(|segment| segment == "containers")?;
    ctx.original_segment(idx + 1)
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

fn describe_group_container_owner(ctx: &PathContext<'_>) -> Option<String> {
    let idx = ctx
        .segments_lower()
        .iter()
        .position(|segment| segment == "group containers")?;
    ctx.original_segment(idx + 1)
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

fn describe_application_support_owner(ctx: &PathContext<'_>) -> Option<String> {
    if let Some(owner) = describe_container_owner(ctx) {
        return Some(owner);
    }
    if let Some(owner) = describe_group_container_owner(ctx) {
        return Some(owner);
    }

    let idx = ctx
        .segments_lower()
        .iter()
        .position(|segment| segment == "application support")?;

    for next_index in (idx + 1)..ctx.original_segments().len() {
        let candidate = ctx.original_segment(next_index)?;
        let lower = candidate.to_lowercase();
        if matches!(lower.as_str(), "cache" | "caches" | "data" | "tmp" | "temp") {
            continue;
        }
        return Some(candidate.to_string());
    }

    None
}

fn describe_cache_owner(ctx: &PathContext<'_>) -> Option<String> {
    if let Some(owner) = describe_container_owner(ctx) {
        return Some(owner);
    }
    if let Some(owner) = describe_group_container_owner(ctx) {
        return Some(owner);
    }

    if let Some(idx) = ctx
        .segments_lower()
        .iter()
        .rposition(|segment| segment == "caches")
    {
        if let Some(next) = ctx.original_segment(idx + 1) {
            let lower = next.to_lowercase();
            if !matches!(lower.as_str(), "cache" | "caches" | "data" | "library") {
                return Some(next.to_string());
            }
        }
        if idx > 0 {
            if let Some(prev) = ctx.original_segment(idx - 1) {
                let lower = prev.to_lowercase();
                if !matches!(
                    lower.as_str(),
                    "library" | "data" | "containers" | "group containers"
                ) {
                    return Some(prev.to_string());
                }
            }
        }
    }

    if let Some(parent) = ctx.path.parent() {
        if let Some(name) = parent.file_name().and_then(|value| value.to_str()) {
            return Some(name.to_string());
        }
    }

    ctx.path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
}
