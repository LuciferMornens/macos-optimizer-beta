use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::advanced_safety::SafetyMetrics;
use super::types::CleanableFile;

/// Intelligent auto-selection engine with machine learning capabilities
pub struct AutoSelectionEngine {
    conservative_defaults: ConservativeDefaults,
    user_pattern_learner: UserPatternLearner,
    _scoring_weights: ScoringWeights,
}

impl AutoSelectionEngine {
    pub fn new() -> Self {
        Self {
            conservative_defaults: ConservativeDefaults::new(),
            user_pattern_learner: UserPatternLearner::new(),
            _scoring_weights: ScoringWeights::default(),
        }
    }

    pub async fn calculate_auto_select_score(
        &self,
        file: &CleanableFile,
        safety_metrics: &SafetyMetrics,
    ) -> AutoSelectScore {
        let mut score = AutoSelectScore::new();

        // Base score from safety analysis
        score.add_safety_score(safety_metrics.base_score, safety_metrics.confidence);

        // Category-based scoring
        score.add_category_score(&file.category);

        // Age modifier (exponential decay)
        let age_days = self.calculate_age_days(&PathBuf::from(&file.path));
        score.apply_age_modifier(age_days);

        // Size modifier (large files need more caution)
        score.apply_size_modifier(file.size);

        // Backup status modifier
        let backup_status = self.check_backup_status(&PathBuf::from(&file.path)).await;
        score.apply_backup_modifier(backup_status);

        // User preference learning
        let user_preference = self.user_pattern_learner.get_user_pattern(file);
        score.apply_user_preference(user_preference);

        // System importance check
        let system_importance = self
            .check_system_importance(&PathBuf::from(&file.path))
            .await;
        score.apply_system_importance(system_importance);

        // Apply conservative defaults
        self.conservative_defaults
            .apply_constraints(&mut score, file);

        // Finalize with confidence calculation
        score.finalize()
    }

    fn calculate_age_days(&self, path: &Path) -> Option<i64> {
        if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                let modified_time = DateTime::<Utc>::from(modified);
                return Some(Utc::now().signed_duration_since(modified_time).num_days());
            }
        }
        None
    }

    async fn check_backup_status(&self, path: &Path) -> BackupStatus {
        // Check Time Machine status
        if let Ok(output) = tokio::process::Command::new("tmutil")
            .arg("isexcluded")
            .arg(path)
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("Excluded") {
                return BackupStatus::NotBacked;
            } else {
                return BackupStatus::BackedUp;
            }
        }

        BackupStatus::Unknown
    }

    async fn check_system_importance(&self, path: &Path) -> SystemImportance {
        let path_str = path.to_string_lossy().to_lowercase();

        // Check for system references
        if path_str.contains("/system/") || path_str.contains("/usr/") {
            return SystemImportance::Critical;
        }

        if path_str.contains("/library/frameworks/") || path_str.contains("/library/preferences/") {
            return SystemImportance::High;
        }

        if path_str.contains("/library/") {
            return SystemImportance::Medium;
        }

        SystemImportance::Low
    }

    pub fn update_from_user_action(&mut self, file: &CleanableFile, action: UserAction) {
        self.user_pattern_learner.record_action(file, action);
    }
}

/// Conservative default settings for auto-selection
pub struct ConservativeDefaults {
    max_auto_select_size: u64,
    min_file_age_hours: i64,
    min_safety_score: u8,
    require_backup_for_large_files: bool,
}

impl ConservativeDefaults {
    pub fn new() -> Self {
        Self {
            max_auto_select_size: 100 * 1024 * 1024, // 100MB
            min_file_age_hours: 24,
            min_safety_score: 95,
            require_backup_for_large_files: true,
        }
    }

    pub fn apply_constraints(&self, score: &mut AutoSelectScore, file: &CleanableFile) {
        // Never auto-select files > max size unless explicitly safe
        if file.size > self.max_auto_select_size && score.raw_score < 98 {
            score.can_auto_select = false;
            score.add_constraint_reason("File too large for auto-selection");
        }

        // Never auto-select recently modified files
        if let Some(age_days) = score.age_days {
            let age_hours = age_days * 24;
            if age_hours < self.min_file_age_hours {
                score.can_auto_select = false;
                score.add_constraint_reason("File modified too recently");
            }
        }

        // Require minimum safety score
        if score.raw_score < self.min_safety_score {
            score.can_auto_select = false;
            score.add_constraint_reason("Safety score below threshold");
        }

        // Large files need backup verification
        if self.require_backup_for_large_files
            && file.size > 50 * 1024 * 1024
            && score.backup_status != BackupStatus::BackedUp
        {
            score.can_auto_select = false;
            score.add_constraint_reason("Large file without backup");
        }
    }
}

/// Learns from user selection patterns
pub struct UserPatternLearner {
    selection_history: HashMap<String, SelectionPattern>,
    category_preferences: HashMap<String, f32>,
}

impl UserPatternLearner {
    pub fn new() -> Self {
        Self {
            selection_history: HashMap::new(),
            category_preferences: HashMap::new(),
        }
    }

    pub fn get_user_pattern(&self, file: &CleanableFile) -> UserPreference {
        // Check if we have history for this specific file type/category
        if let Some(pattern) = self.selection_history.get(&file.category) {
            return self.calculate_preference(pattern);
        }

        // Check category-level preferences
        if let Some(&preference) = self.category_preferences.get(&file.category) {
            if preference > 0.8 {
                return UserPreference::UsuallySelects;
            } else if preference < 0.2 {
                return UserPreference::UsuallyDeselects;
            }
        }

        UserPreference::NoPattern
    }

    fn calculate_preference(&self, pattern: &SelectionPattern) -> UserPreference {
        let selection_rate = pattern.selected_count as f32 / pattern.total_count as f32;

        if selection_rate > 0.8 {
            UserPreference::UsuallySelects
        } else if selection_rate < 0.2 {
            UserPreference::UsuallyDeselects
        } else {
            UserPreference::Mixed
        }
    }

    pub fn record_action(&mut self, file: &CleanableFile, action: UserAction) {
        let pattern = self
            .selection_history
            .entry(file.category.clone())
            .or_insert(SelectionPattern {
                category: file.category.clone(),
                total_count: 0,
                selected_count: 0,
                deselected_count: 0,
                last_action: Utc::now(),
            });

        pattern.total_count += 1;
        pattern.last_action = Utc::now();

        match action {
            UserAction::Selected => pattern.selected_count += 1,
            UserAction::Deselected => pattern.deselected_count += 1,
            UserAction::Ignored => {}
        }

        // Update category preference
        let preference = pattern.selected_count as f32 / pattern.total_count as f32;
        self.category_preferences
            .insert(file.category.clone(), preference);
    }

    // Persistence APIs can be added under a feature flag when needed.
}

/// Auto-selection scoring system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSelectScore {
    pub raw_score: u8,
    pub confidence: f32,
    pub can_auto_select: bool,
    pub age_days: Option<i64>,
    pub backup_status: BackupStatus,
    pub user_preference_modifier: f32,
    pub constraint_reasons: Vec<String>,
    pub recommendation: SelectionRecommendation,
}

impl AutoSelectScore {
    pub fn new() -> Self {
        Self {
            raw_score: 50,
            confidence: 0.5,
            can_auto_select: false,
            age_days: None,
            backup_status: BackupStatus::Unknown,
            user_preference_modifier: 0.0,
            constraint_reasons: Vec::new(),
            recommendation: SelectionRecommendation::Review,
        }
    }

    pub fn add_safety_score(&mut self, safety_score: u8, confidence: f32) {
        self.raw_score = safety_score;
        self.confidence = confidence;

        // High safety score enables auto-selection
        if safety_score >= 95 {
            self.can_auto_select = true;
        }
    }

    pub fn add_category_score(&mut self, category: &str) {
        let category_lower = category.to_lowercase();

        let category_modifier: i32 = match category_lower.as_str() {
            cat if cat.contains("trash") => 50,
            cat if cat.contains("cache") => 45,
            cat if cat.contains("temp") => 40,
            cat if cat.contains("log") && cat.contains("30d") => 35,
            cat if cat.contains("download") && cat.contains("old") => -10,
            cat if cat.contains("backup") => -30,
            cat if cat.contains("archive") => -25,
            _ => 0,
        };

        self.raw_score = if category_modifier >= 0 {
            self.raw_score
                .saturating_add(category_modifier as u8)
                .min(100)
        } else {
            self.raw_score
                .saturating_sub(category_modifier.unsigned_abs() as u8)
        };
    }

    pub fn apply_age_modifier(&mut self, age_days: Option<i64>) {
        self.age_days = age_days;

        if let Some(days) = age_days {
            let age_modifier: i32 = match days {
                0..=1 => -30,   // Very recent
                2..=7 => -20,   // Recent
                8..=30 => -10,  // Somewhat recent
                31..=90 => 5,   // Old
                91..=180 => 10, // Very old
                _ => 15,        // Ancient
            };

            self.raw_score = if age_modifier >= 0 {
                self.raw_score.saturating_add(age_modifier as u8).min(100)
            } else {
                self.raw_score
                    .saturating_sub(age_modifier.unsigned_abs() as u8)
            };

            // Boost confidence for very old files
            if days > 90 {
                self.confidence = (self.confidence + 0.1).min(1.0);
            }
        }
    }

    pub fn apply_size_modifier(&mut self, size: u64) {
        let size_mb = size / (1024 * 1024);

        let size_modifier: i32 = match size_mb {
            0..=10 => 5,       // Small files
            11..=100 => 0,     // Medium files
            101..=500 => -10,  // Large files
            501..=1000 => -20, // Very large files
            _ => -30,          // Huge files
        };

        self.raw_score = if size_modifier >= 0 {
            self.raw_score.saturating_add(size_modifier as u8).min(100)
        } else {
            self.raw_score
                .saturating_sub(size_modifier.unsigned_abs() as u8)
        };

        // Large files should not be auto-selected without high confidence
        if size_mb > 100 && self.confidence < 0.8 {
            self.can_auto_select = false;
            self.add_constraint_reason("Large file with insufficient confidence");
        }
    }

    pub fn apply_backup_modifier(&mut self, status: BackupStatus) {
        self.backup_status = status;

        match status {
            BackupStatus::BackedUp => {
                self.raw_score = self.raw_score.saturating_add(10).min(100);
                self.confidence = (self.confidence + 0.1).min(1.0);
            }
            BackupStatus::NotBacked => {
                self.raw_score = self.raw_score.saturating_sub(15);
                self.can_auto_select = false;
                self.add_constraint_reason("File not backed up");
            }
            BackupStatus::Unknown => {
                // No modification for unknown status
            }
        }
    }

    pub fn apply_user_preference(&mut self, preference: UserPreference) {
        self.user_preference_modifier = match preference {
            UserPreference::UsuallySelects => 0.2,
            UserPreference::UsuallyDeselects => -0.3,
            UserPreference::Mixed => 0.0,
            UserPreference::NoPattern => 0.0,
        };

        // Apply modifier to confidence
        self.confidence = (self.confidence + self.user_preference_modifier).clamp(0.0, 1.0);

        // Strong deselection pattern prevents auto-selection
        if matches!(preference, UserPreference::UsuallyDeselects) {
            self.can_auto_select = false;
            self.add_constraint_reason("User usually deselects this category");
        }
    }

    pub fn apply_system_importance(&mut self, importance: SystemImportance) {
        match importance {
            SystemImportance::Critical => {
                self.raw_score = 0;
                self.can_auto_select = false;
                self.add_constraint_reason("Critical system component");
            }
            SystemImportance::High => {
                self.raw_score = self.raw_score.saturating_sub(30);
                self.can_auto_select = false;
                self.add_constraint_reason("High system importance");
            }
            SystemImportance::Medium => {
                self.raw_score = self.raw_score.saturating_sub(15);
            }
            SystemImportance::Low => {
                // No modification
            }
        }
    }

    pub fn add_constraint_reason(&mut self, reason: &str) {
        self.constraint_reasons.push(reason.to_string());
    }

    pub fn finalize(mut self) -> Self {
        // Determine final recommendation
        self.recommendation = if self.can_auto_select && self.raw_score >= 95 {
            SelectionRecommendation::AutoSelect
        } else if self.raw_score >= 80 {
            SelectionRecommendation::Recommend
        } else if self.raw_score >= 60 {
            SelectionRecommendation::Review
        } else if self.raw_score >= 40 {
            SelectionRecommendation::Caution
        } else {
            SelectionRecommendation::DoNotSelect
        };

        self
    }
}

// Supporting data structures

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub safety_weight: f32,
    pub age_weight: f32,
    pub size_weight: f32,
    pub backup_weight: f32,
    pub user_preference_weight: f32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            safety_weight: 0.4,
            age_weight: 0.2,
            size_weight: 0.15,
            backup_weight: 0.15,
            user_preference_weight: 0.1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BackupStatus {
    BackedUp,
    NotBacked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserPreference {
    UsuallySelects,
    UsuallyDeselects,
    Mixed,
    NoPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemImportance {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SelectionRecommendation {
    AutoSelect,
    Recommend,
    Review,
    Caution,
    DoNotSelect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionPattern {
    pub category: String,
    pub total_count: u32,
    pub selected_count: u32,
    pub deselected_count: u32,
    pub last_action: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserAction {
    Selected,
    Deselected,
    Ignored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPatternData {
    pub selection_history: HashMap<String, SelectionPattern>,
    pub category_preferences: HashMap<String, f32>,
}
