use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::advanced_safety::{SafetyAnalyzer, SafetyMetrics, SafetyRecommendation};
use super::auto_selection::{AutoSelectScore, AutoSelectionEngine, UserAction};
use super::cache::DIR_SIZE_CACHE;
use super::enhanced_rules::{DynamicRuleEngine, RuleValidator};
use super::macos_integration::{
    BackupStatus, CloudStatus, FileAssociation, MacOSIntegration, SpotlightInfo,
};
use super::safety::policy_for_category;
use super::smart_cache::{CacheValidation, DuplicateDetector, DuplicateGroup, SmartCacheDetector};
use super::telemetry::{SafetyMetricsCollector, TelemetrySnapshot};
use super::types::{load_rules_result, CategoryReport, CleanableFile, CleaningReport};
use super::validation::{
    FileValidationState, PreDeletionValidator, RecoveryManager, ValidationResult,
};
use dirs;
use tokio_util::sync::CancellationToken;

/// Enhanced file cleaner with all safety features
pub struct EnhancedFileCleaner {
    cleanable_files: Vec<EnhancedCleanableFile>,
    seen_paths: HashSet<String>,
    seen_dir_prefixes: Vec<String>,

    // Enhanced components
    safety_analyzer: SafetyAnalyzer,
    cache_detector: SmartCacheDetector,
    validator: PreDeletionValidator,
    recovery_manager: RecoveryManager,
    auto_selector: AutoSelectionEngine,
    macos_integration: MacOSIntegration,
    duplicate_detector: DuplicateDetector,
    telemetry: SafetyMetricsCollector,
}

impl EnhancedFileCleaner {
    pub fn new() -> Self {
        Self {
            cleanable_files: Vec::new(),
            seen_paths: HashSet::new(),
            seen_dir_prefixes: Vec::new(),

            safety_analyzer: SafetyAnalyzer::new(),
            cache_detector: SmartCacheDetector::new(),
            validator: PreDeletionValidator::new(),
            recovery_manager: RecoveryManager::new(),
            auto_selector: AutoSelectionEngine::new(),
            macos_integration: MacOSIntegration::new(),
            duplicate_detector: DuplicateDetector::new(),
            telemetry: SafetyMetricsCollector::new(),
        }
    }

    /// Prepare deletion by filtering currently scanned files with provided paths.
    pub async fn prepare_deletion_by_paths(
        &mut self,
        file_paths: &[String],
    ) -> Result<DeletionPreparation, String> {
        let files_to_clean: Vec<EnhancedCleanableFile> = self
            .cleanable_files
            .iter()
            .filter(|f| file_paths.contains(&f.base.path))
            .cloned()
            .collect();
        self.validate_and_prepare_deletion(&files_to_clean).await
    }

    /// Enhanced system scan with multi-layer safety analysis (cancellable)
    pub async fn scan_system_enhanced_with_cancel(
        &mut self,
        token: &CancellationToken,
        progress: Option<&(dyn Fn(f32, &str, &str) + Send + Sync)>,
    ) -> Result<EnhancedCleaningReport, String> {
        self.cleanable_files.clear();
        self.seen_paths.clear();
        self.seen_dir_prefixes.clear();
        self.telemetry.start_scan();

        // Load base rules
        let base_rules = load_rules_result()?;

        // Adapt and augment rules dynamically
        let dynamic = DynamicRuleEngine::new();
        let mut rules_adapted = dynamic.adapt_rules_to_system(&base_rules);
        let mut app_specific = dynamic.generate_app_specific_rules();
        rules_adapted.categories.append(&mut app_specific);

        // Validate rule consistency (non-fatal)
        let validator = RuleValidator::new();
        let _conflicts = validator.validate_rule_consistency(&rules_adapted);
        let _preview = validator.dry_run_rules(&rules_adapted);

        if token.is_cancelled() {
            return Err("cancelled".into());
        }
        if let Some(cb) = progress {
            cb(10.0, "Discovering files", "discovery");
        }

        // Phase 1: Initial file discovery
        for rule in rules_adapted.categories.iter() {
            if token.is_cancelled() {
                return Err("cancelled".into());
            }
            let paths_to_scan: Vec<_> = rule
                .paths
                .iter()
                .filter_map(|p| Self::expand_path(p))
                .filter(|path| path.exists())
                .collect();

            for path in paths_to_scan {
                if token.is_cancelled() {
                    return Err("cancelled".into());
                }
                if let Err(_) = self.scan_path_enhanced(&path, &rule.name).await {
                    continue;
                }
            }

            tokio::task::yield_now().await;
        }

        // Phase 2: Duplicate detection
        if token.is_cancelled() {
            return Err("cancelled".into());
        }
        if let Some(cb) = progress {
            cb(45.0, "Detecting duplicates", "duplicates");
        }
        let all_paths: Vec<PathBuf> = self
            .cleanable_files
            .iter()
            .map(|f| PathBuf::from(&f.base.path))
            .collect();
        let duplicate_groups = self.duplicate_detector.find_duplicates(&all_paths).await;

        // Phase 3: Safety analysis for each file
        if token.is_cancelled() {
            return Err("cancelled".into());
        }
        if let Some(cb) = progress {
            cb(65.0, "Analyzing safety", "safety");
        }
        for file in &mut self.cleanable_files {
            if token.is_cancelled() {
                return Err("cancelled".into());
            }
            let path = PathBuf::from(&file.base.path);

            // Multi-layer safety analysis
            file.safety_metrics = self
                .safety_analyzer
                .analyze(&path, &file.base.category)
                .await;

            // Cache validation if applicable
            if file.base.category.to_lowercase().contains("cache") {
                file.cache_validation = Some(
                    self.cache_detector
                        .validate_cache_file(&path, &file.base.category)
                        .await,
                );
            }

            // macOS integration checks
            file.macos_status = Some(MacOSFileStatus {
                is_sip_protected: self.macos_integration.check_sip_protection(&path),
                spotlight_info: self
                    .macos_integration
                    .check_spotlight_importance(&path)
                    .await,
                time_machine_status: self
                    .macos_integration
                    .check_time_machine_status(&path)
                    .await,
                icloud_status: self.macos_integration.check_icloud_status(&path).await,
                file_associations: self.macos_integration.get_file_associations(&path).await,
            });

            // Auto-selection scoring
            file.auto_select_score = self
                .auto_selector
                .calculate_auto_select_score(&file.base, &file.safety_metrics)
                .await;

            // Update base file with enhanced safety data
            file.base.safe_to_delete = matches!(
                file.safety_metrics.recommendation,
                SafetyRecommendation::SafeToAutoDelete
                    | SafetyRecommendation::SafeWithUserConfirmation
            );
            file.base.safety_score = file.safety_metrics.base_score;
            file.base.auto_select = file.auto_select_score.can_auto_select;

            // Enforce policy gates (auto-select threshold, never-auto), without overriding hard blocks
            let policy = policy_for_category(&file.base.category);
            // Apply policy guardrails without overriding other subsystems'
            // decisions beyond the defined safety boundaries.
            policy.enforce(&mut file.base);
        }

        if let Some(cb) = progress {
            cb(90.0, "Scoring and summarizing", "scoring");
        }
        // Generate enhanced report
        let report = self.generate_enhanced_report(duplicate_groups);
        self.telemetry.finish_scan();
        Ok(report)
    }

    async fn scan_path_enhanced(&mut self, path: &Path, category: &str) -> Result<(), String> {
        // Skip if already seen
        let path_lower = path.to_string_lossy().to_lowercase();
        if self.seen_paths.contains(&path_lower) {
            return Ok(());
        }

        // Check if it's a subdirectory of an already-added directory
        for prefix in &self.seen_dir_prefixes {
            if path_lower.starts_with(prefix) {
                return Ok(());
            }
        }

        if path.is_file() {
            // Process single file
            if let Ok(metadata) = fs::metadata(path) {
                let enhanced_file = EnhancedCleanableFile {
                    base: CleanableFile {
                        path: path.to_string_lossy().to_string(),
                        size: metadata.len(),
                        category: category.to_string(),
                        description: format!("File in {}", category),
                        last_modified: metadata
                            .modified()
                            .map(|t| DateTime::<Utc>::from(t).timestamp())
                            .unwrap_or(0),
                        safe_to_delete: false, // Will be updated after safety analysis
                        safety_score: 0,       // Will be updated after safety analysis
                        auto_select: false,    // Will be updated after auto-selection scoring
                    },
                    safety_metrics: SafetyMetrics {
                        base_score: 0,
                        confidence: 0.0,
                        risk_factors: Vec::new(),
                        safety_flags: Default::default(),
                        recommendation: SafetyRecommendation::DoNotDelete,
                    },
                    cache_validation: None,
                    auto_select_score: AutoSelectScore::new(),
                    macos_status: None,
                    validation_state: None,
                };

                self.cleanable_files.push(enhanced_file);
                self.seen_paths.insert(path_lower);
            }
        } else if path.is_dir() {
            // Process directory
            let dir_size = self.calculate_dir_size(path).await;

            if dir_size > 0 {
                let enhanced_file = EnhancedCleanableFile {
                    base: CleanableFile {
                        path: path.to_string_lossy().to_string(),
                        size: dir_size,
                        category: category.to_string(),
                        description: format!("Directory in {}", category),
                        last_modified: fs::metadata(path)
                            .and_then(|m| m.modified())
                            .map(|t| DateTime::<Utc>::from(t).timestamp())
                            .unwrap_or(0),
                        safe_to_delete: false,
                        safety_score: 0,
                        auto_select: false,
                    },
                    safety_metrics: SafetyMetrics {
                        base_score: 0,
                        confidence: 0.0,
                        risk_factors: Vec::new(),
                        safety_flags: Default::default(),
                        recommendation: SafetyRecommendation::DoNotDelete,
                    },
                    cache_validation: None,
                    auto_select_score: AutoSelectScore::new(),
                    macos_status: None,
                    validation_state: None,
                };

                self.cleanable_files.push(enhanced_file);
                self.seen_paths.insert(path_lower.clone());

                // Mark as directory prefix to skip subdirectories
                let mut dir_prefix = path_lower;
                if !dir_prefix.ends_with('/') {
                    dir_prefix.push('/');
                }
                self.seen_dir_prefixes.push(dir_prefix);
            }
        }

        Ok(())
    }

    async fn calculate_dir_size(&self, path: &Path) -> u64 {
        // Use the cache's get_or_calculate method
        DIR_SIZE_CACHE
            .get_or_calculate(path, |p| {
                let mut total_size = 0u64;

                if let Ok(entries) = fs::read_dir(p) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() {
                                total_size += metadata.len();
                            } else if metadata.is_dir() {
                                // Note: This is a simplified recursive calculation
                                // In production, should use async recursion properly
                                if let Ok(subdir_size) =
                                    Self::calculate_dir_size_sync(&entry.path())
                                {
                                    total_size += subdir_size;
                                }
                            }
                        }
                    }
                }

                Ok(total_size)
            })
            .await
            .unwrap_or(0)
    }

    fn calculate_dir_size_sync(path: &Path) -> Result<u64, String> {
        let mut total_size = 0u64;

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_size += metadata.len();
                    } else if metadata.is_dir() {
                        if let Ok(subdir_size) = Self::calculate_dir_size_sync(&entry.path()) {
                            total_size += subdir_size;
                        }
                    }
                }
            }
        }

        Ok(total_size)
    }

    /// Pre-deletion validation with recovery point creation
    pub async fn validate_and_prepare_deletion(
        &mut self,
        files: &[EnhancedCleanableFile],
    ) -> Result<DeletionPreparation, String> {
        // Extract base files for validation
        let base_files: Vec<CleanableFile> = files.iter().map(|f| f.base.clone()).collect();

        // Run pre-deletion validation
        let validation_result = self.validator.validate_before_deletion(&base_files).await;

        // Create recovery point
        let recovery_point = self.recovery_manager.create_recovery_point(&base_files);

        // Update file validation states
        for file in &mut self.cleanable_files {
            let path = PathBuf::from(&file.base.path);
            if let Some(state) = validation_result.file_states.get(&path) {
                file.validation_state = Some(state.clone());
            }
        }

        let files_ready = validation_result.is_safe;

        Ok(DeletionPreparation {
            validation_result,
            recovery_point_id: recovery_point.id,
            files_ready,
            total_size: base_files.iter().map(|f| f.size).sum(),
        })
    }

    /// Clean selected files with enhanced safety
    pub async fn clean_files_enhanced(
        &mut self,
        file_paths: Vec<String>,
        token: Option<&CancellationToken>,
        allow_low_safety: bool,
    ) -> Result<CleaningResult, String> {
        let mut deleted_files = Vec::new();
        let mut failed_files = Vec::new();
        let mut total_freed = 0u64;

        // Filter to get only selected enhanced files
        let files_to_clean: Vec<EnhancedCleanableFile> = self
            .cleanable_files
            .iter()
            .filter(|f| file_paths.contains(&f.base.path))
            .cloned()
            .collect();

        // Validate before deletion
        let preparation = self.validate_and_prepare_deletion(&files_to_clean).await?;
        if let Some(t) = token {
            if t.is_cancelled() {
                return Err("cancelled".into());
            }
        }

        if !preparation.files_ready {
            return Err(format!(
                "Validation failed: {} errors found",
                preparation.validation_result.errors.len()
            ));
        }

        // Proceed with deletion
        if let Some(t) = token {
            if t.is_cancelled() {
                return Err("cancelled".into());
            }
        }
        for file in files_to_clean {
            if let Some(t) = token {
                if t.is_cancelled() {
                    return Err("cancelled".into());
                }
            }
            let path = PathBuf::from(&file.base.path);
            let base_score = file.safety_metrics.base_score;

            // Double-check safety
            if base_score < 40 && !allow_low_safety {
                failed_files.push(FailedDeletion {
                    path: file.base.path.clone(),
                    reason: "Safety score too low (enable Risky Mode to override)".to_string(),
                });
                continue;
            }

            // Attempt deletion (prefer Trash). Only direct-delete when extremely safe
            let prefer_trash_only = allow_low_safety || base_score < 80;

            let deleted = if self.move_to_trash(&path).await {
                true
            } else if !prefer_trash_only && base_score >= 95 {
                // Only attempt direct deletion for extremely safe files
                fs::remove_file(&path).is_ok() || fs::remove_dir_all(&path).is_ok()
            } else {
                false
            };

            if deleted {
                deleted_files.push(file.base.path.clone());
                total_freed += file.base.size;

                // Record user action for learning
                self.auto_selector
                    .update_from_user_action(&file.base, UserAction::Selected);
            } else {
                failed_files.push(FailedDeletion {
                    path: file.base.path.clone(),
                    reason: "Failed to delete".to_string(),
                });
            }
        }

        Ok(CleaningResult {
            deleted_count: deleted_files.len(),
            failed_count: failed_files.len(),
            total_freed,
            deleted_files,
            failed_files,
            recovery_point_id: preparation.recovery_point_id,
        })
    }

    async fn move_to_trash(&self, path: &Path) -> bool {
        // Prefer Finder deletion (moves to Trash per-volume)
        if let Ok(output) = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "tell application \"Finder\" to move POSIX file \"{}\" to trash",
                path.display()
            ))
            .output()
            .await
        {
            if output.status.success() {
                return true;
            }
        }

        // Fallback: rename into ~/.Trash with unique name
        if let Some(home) = dirs::home_dir() {
            let trash = home.join(".Trash");
            if trash.exists() || std::fs::create_dir_all(&trash).is_ok() {
                if let Some(name) = path.file_name() {
                    let mut target = trash.join(name);
                    if target.exists() {
                        let stem = name.to_string_lossy().to_string();
                        let (base, ext) = Self::split_name_ext_local(&stem);
                        let ts = Local::now().format("%Y%m%d-%H%M%S").to_string();
                        let mut counter = 1u32;
                        loop {
                            let candidate = if ext.is_empty() {
                                format!("{} ({}-{})", base, ts, counter)
                            } else {
                                format!("{} ({}-{}).{}", base, ts, counter, ext)
                            };
                            target = trash.join(candidate);
                            if !target.exists() {
                                break;
                            }
                            counter += 1;
                        }
                    }
                    if std::fs::rename(path, &target).is_ok() {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Local helper to split name and extension (avoids referencing private engine helper)
    fn split_name_ext_local(name: &str) -> (String, String) {
        if let Some(idx) = name.rfind('.') {
            let (base, ext) = name.split_at(idx);
            (base.to_string(), ext.trim_start_matches('.').to_string())
        } else {
            (name.to_string(), String::new())
        }
    }

    fn generate_enhanced_report(
        &self,
        duplicate_groups: Vec<DuplicateGroup>,
    ) -> EnhancedCleaningReport {
        let mut categories_map: HashMap<String, CategorySummary> = HashMap::new();
        let mut total_size = 0u64;
        let mut auto_selected_size = 0u64;
        let mut high_risk_count = 0;
        let mut duplicate_size = 0u64;

        for file in &self.cleanable_files {
            total_size += file.base.size;

            if file.base.auto_select {
                auto_selected_size += file.base.size;
            }

            if file.safety_metrics.base_score < 50 {
                high_risk_count += 1;
            }

            let category_summary =
                categories_map
                    .entry(file.base.category.clone())
                    .or_insert(CategorySummary {
                        name: file.base.category.clone(),
                        total_size: 0,
                        file_count: 0,
                        auto_selected_size: 0,
                        auto_selected_count: 0,
                        average_safety_score: 0.0,
                    });

            category_summary.total_size += file.base.size;
            category_summary.file_count += 1;

            if file.base.auto_select {
                category_summary.auto_selected_size += file.base.size;
                category_summary.auto_selected_count += 1;
            }
        }

        // Calculate average safety scores
        for (_, summary) in categories_map.iter_mut() {
            let category_files: Vec<&EnhancedCleanableFile> = self
                .cleanable_files
                .iter()
                .filter(|f| f.base.category == summary.name)
                .collect();

            if !category_files.is_empty() {
                let total_score: u32 = category_files
                    .iter()
                    .map(|f| f.safety_metrics.base_score as u32)
                    .sum();
                summary.average_safety_score = total_score as f32 / category_files.len() as f32;
            }
        }

        // Calculate duplicate space before consuming duplicate_groups
        for group in &duplicate_groups {
            // Count all but one file in each group as duplicate space
            if group.files.len() > 1 {
                let duplicate_files_in_group = group.files.len() - 1;
                let size_per_file = group.total_size / group.files.len() as u64;
                duplicate_size += size_per_file * duplicate_files_in_group as u64;
            }
        }

        // Convert to legacy category reports for compatibility
        let categories: Vec<CategoryReport> = categories_map
            .values()
            .map(|s| CategoryReport {
                name: s.name.clone(),
                size: s.total_size,
                count: s.file_count,
            })
            .collect();

        let advanced_categories = categories_map
            .keys()
            .filter(|k| k.contains("Advanced"))
            .cloned()
            .collect();

        EnhancedCleaningReport {
            base: CleaningReport {
                total_size,
                files_count: self.cleanable_files.len(),
                categories,
                advanced_categories,
            },
            enhanced_files: self.cleanable_files.clone(),
            category_summaries: categories_map.into_iter().map(|(_, v)| v).collect(),
            safety_summary: SafetySummary {
                auto_selected_size,
                auto_selected_count: self
                    .cleanable_files
                    .iter()
                    .filter(|f| f.base.auto_select)
                    .count(),
                high_risk_count,
                average_safety_score: if !self.cleanable_files.is_empty() {
                    let total: u32 = self
                        .cleanable_files
                        .iter()
                        .map(|f| f.safety_metrics.base_score as u32)
                        .sum();
                    total as f32 / self.cleanable_files.len() as f32
                } else {
                    0.0
                },
            },
            duplicate_groups,
            duplicate_space_recoverable: duplicate_size,
        }
    }

    fn expand_path(path_template: &str) -> Option<PathBuf> {
        if path_template.starts_with("~/") {
            dirs::home_dir().map(|home| home.join(&path_template[2..]))
        } else {
            Some(PathBuf::from(path_template))
        }
    }

    /// Record user feedback for machine learning
    pub fn record_user_feedback(&mut self, file_path: &str, action: UserAction) {
        if let Some(file) = self
            .cleanable_files
            .iter()
            .find(|f| f.base.path == file_path)
        {
            self.auto_selector
                .update_from_user_action(&file.base, action.clone());
            if matches!(action, UserAction::Deselected) {
                self.telemetry.track_deselection();
            }
        }
    }

    pub fn telemetry_snapshot(&self) -> TelemetrySnapshot {
        self.telemetry.get_snapshot()
    }
}

// Enhanced data structures

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedCleanableFile {
    pub base: CleanableFile,
    pub safety_metrics: SafetyMetrics,
    pub cache_validation: Option<CacheValidation>,
    pub auto_select_score: AutoSelectScore,
    pub macos_status: Option<MacOSFileStatus>,
    pub validation_state: Option<FileValidationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacOSFileStatus {
    pub is_sip_protected: bool,
    pub spotlight_info: SpotlightInfo,
    pub time_machine_status: BackupStatus,
    pub icloud_status: CloudStatus,
    pub file_associations: Vec<FileAssociation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedCleaningReport {
    pub base: CleaningReport,
    pub enhanced_files: Vec<EnhancedCleanableFile>,
    pub category_summaries: Vec<CategorySummary>,
    pub safety_summary: SafetySummary,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub duplicate_space_recoverable: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub name: String,
    pub total_size: u64,
    pub file_count: usize,
    pub auto_selected_size: u64,
    pub auto_selected_count: usize,
    pub average_safety_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetySummary {
    pub auto_selected_size: u64,
    pub auto_selected_count: usize,
    pub high_risk_count: usize,
    pub average_safety_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionPreparation {
    pub validation_result: ValidationResult,
    pub recovery_point_id: String,
    pub files_ready: bool,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleaningResult {
    pub deleted_count: usize,
    pub failed_count: usize,
    pub total_freed: u64,
    pub deleted_files: Vec<String>,
    pub failed_files: Vec<FailedDeletion>,
    pub recovery_point_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedDeletion {
    pub path: String,
    pub reason: String,
}
