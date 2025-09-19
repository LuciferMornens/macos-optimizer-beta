use std::path::Path;

use super::context::PathContext;
use super::risk::{RiskAssessment, RiskLevel};
use crate::file_cleaner::types::CleanableFile;

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

pub(crate) fn policy_for_category(category: &str) -> SafetyPolicy {
    let c = category.to_lowercase();
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
