use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Local;
use serde::{Deserialize, Serialize};

use super::advanced_safety::{
    RiskFactor, SafetyAnalyzer, SafetyFlags, SafetyMetrics, SafetyRecommendation,
};
use super::auto_selection::{AutoSelectScore, AutoSelectionEngine, UserAction};
use super::duplicate_detector::{DuplicateDetector, DuplicateGroup};
use super::engine::FileCleaner;
use super::enhanced_rules::DynamicRuleEngine;
use super::macos_integration::{
    BackupStatus, CloudStatus, FileAssociation, MacOSIntegration, SpotlightInfo,
};
use super::process_snapshot::ProcessSnapshot;
use super::safety::policy_for_category;
use super::smart_cache::{CacheValidation, SmartCacheDetector};
use super::telemetry::{SafetyMetricsCollector, TelemetrySnapshot};
use super::types::{CategoryReport, CleanableFile, CleanerRules, CleaningReport};
use super::validation::{
    BlockReason, FileValidationState, PreDeletionValidator, RecoveryManager, ValidationResult,
};
use crate::ops::ThroughputTracker;
use dirs;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct EnhancedDeletionProgress {
    pub progress: f32,
    pub message: String,
    pub stage: &'static str,
    pub eta_ms: Option<u32>,
    pub files_per_s: Option<f32>,
    pub mb_per_s: Option<f32>,
}

/// Enhanced file cleaner with all safety features
pub struct EnhancedFileCleaner {
    cleanable_files: Vec<EnhancedCleanableFile>,
    seen_paths: HashSet<String>,
    seen_dir_prefixes: Vec<String>,
    base_cleaner: FileCleaner,

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
            base_cleaner: FileCleaner::new(),

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

    fn add_enhanced_placeholder(&mut self, base: CleanableFile) {
        let path_lower = base.path.to_lowercase();
        if self.seen_paths.contains(&path_lower) {
            return;
        }

        if let Ok(metadata) = fs::metadata(&base.path) {
            if metadata.is_dir() {
                let mut prefix = path_lower.clone();
                if !prefix.ends_with('/') {
                    prefix.push('/');
                }
                self.seen_dir_prefixes.push(prefix);
            }
        }

        self.seen_paths.insert(path_lower);

        let enhanced = EnhancedCleanableFile {
            base,
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

        self.cleanable_files.push(enhanced);
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

        if token.is_cancelled() {
            return Err("cancelled".into());
        }
        if let Some(cb) = progress {
            cb(8.0, "Scanning baseline categories", "discovery");
        }

        self.base_cleaner.scan_system_with_cancel(token).await?;

        let baseline_files: Vec<CleanableFile> = self
            .base_cleaner
            .get_cleanable_files()
            .iter()
            .cloned()
            .collect();

        for base in baseline_files {
            if token.is_cancelled() {
                return Err("cancelled".into());
            }
            self.add_enhanced_placeholder(base);
        }

        // Dynamic app-aware rules (developer caches, etc.)
        let dynamic_engine = DynamicRuleEngine::new();
        let generated_dynamic = dynamic_engine.generate_app_specific_rules();
        if !generated_dynamic.is_empty() {
            if let Some(cb) = progress {
                cb(18.0, "Scanning developer caches", "discovery");
            }

            let dynamic_rules = CleanerRules {
                categories: generated_dynamic,
            };
            let adapted = dynamic_engine.adapt_rules_to_system(&dynamic_rules);

            for rule in adapted.categories.iter() {
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
                    match self
                        .base_cleaner
                        .collect_rule_matches_for_path(&path, rule, token)
                    {
                        Ok(matches) => {
                            for file in matches {
                                if token.is_cancelled() {
                                    return Err("cancelled".into());
                                }
                                self.add_enhanced_placeholder(file);
                            }
                        }
                        Err(err) => {
                            log::warn!("Dynamic rule scan error for {}: {}", path.display(), err);
                        }
                    }
                }

                tokio::task::yield_now().await;
            }
        }

        if token.is_cancelled() {
            return Err("cancelled".into());
        }
        if let Some(cb) = progress {
            cb(40.0, "Enhancing metadata", "analysis");
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
        let duplicate_scan = self
            .duplicate_detector
            .find_duplicates(&all_paths, token)
            .await?;
        let duplicate_group_count = duplicate_scan.groups.len();
        let analyzed_for_duplicates = duplicate_scan.analyzed_files;
        let skipped_for_duplicates = duplicate_scan.skipped_files;
        if duplicate_scan.truncated {
            log::warn!(
                "Duplicate detection truncated: processed {} files, skipped {}",
                analyzed_for_duplicates,
                skipped_for_duplicates
            );
        } else {
            log::debug!(
                "Duplicate detection completed: {} groups from {} files ({} skipped)",
                duplicate_group_count,
                analyzed_for_duplicates,
                skipped_for_duplicates
            );
        }
        if let Some(cb) = progress {
            let message = if duplicate_scan.truncated {
                format!(
                    "Duplicate scan partial ({} files, {} skipped)",
                    analyzed_for_duplicates, skipped_for_duplicates
                )
            } else {
                format!(
                    "Detected {} duplicate sets across {} files",
                    duplicate_group_count, analyzed_for_duplicates
                )
            };
            let progress_value = if duplicate_scan.truncated { 52.0 } else { 55.0 };
            cb(progress_value, &message, "duplicates");
        }
        let duplicate_groups = duplicate_scan.groups;

        if token.is_cancelled() {
            return Err("cancelled".into());
        }

        let process_snapshot = ProcessSnapshot::capture().await;

        if token.is_cancelled() {
            return Err("cancelled".into());
        }

        // Phase 3: Safety analysis for each file
        if token.is_cancelled() {
            return Err("cancelled".into());
        }

        let total_files = self.cleanable_files.len();
        if let Some(cb) = progress {
            cb(65.0, "Analyzing safety", "safety");
        }

        if total_files > 0 {
            const SAFETY_ANALYSIS_BUDGET: Duration = Duration::from_secs(12);
            let safety_start = Instant::now();
            let mut processed = 0usize;
            let mut truncated = false;

            for file in &mut self.cleanable_files {
                if token.is_cancelled() {
                    return Err("cancelled".into());
                }
                if safety_start.elapsed() >= SAFETY_ANALYSIS_BUDGET {
                    truncated = true;
                    break;
                }

                let path = PathBuf::from(&file.base.path);

                // Multi-layer safety analysis
                file.safety_metrics = self
                    .safety_analyzer
                    .analyze_with_snapshot(&path, &file.base.category, &process_snapshot)
                    .await;

                // Cache validation if applicable
                if file.base.category.to_lowercase().contains("cache") {
                    file.cache_validation = Some(
                        self.cache_detector
                            .validate_cache_file(&path, &file.base.category, &process_snapshot)
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
                policy.enforce(&mut file.base);

                processed += 1;

                if let Some(cb) = progress {
                    if processed % 75 == 0 || processed == total_files {
                        let fraction = processed as f32 / total_files as f32;
                        let progress_value = 65.0 + fraction * 20.0;
                        cb(
                            progress_value.min(88.0),
                            &format!("Analyzing safety ({} / {} files)", processed, total_files),
                            "safety",
                        );
                    }
                }
            }

            if truncated {
                for file in self.cleanable_files.iter_mut().skip(processed) {
                    Self::apply_deferred_safety(file);
                }
                if let Some(cb) = progress {
                    cb(
                        88.0,
                        &format!(
                            "Safety analysis partial ({} of {} files)",
                            processed, total_files
                        ),
                        "safety",
                    );
                }
                log::warn!(
                    "Safety analysis truncated after {} files ({} total)",
                    processed,
                    total_files
                );
            } else if let Some(cb) = progress {
                cb(
                    90.0,
                    &format!("Safety analysis complete ({} files)", processed),
                    "safety",
                );
            }
        } else if let Some(cb) = progress {
            cb(90.0, "Safety analysis complete (0 files)", "safety");
        }

        if let Some(cb) = progress {
            cb(90.0, "Scoring and summarizing", "scoring");
        }
        // Generate enhanced report
        let report = self.generate_enhanced_report(duplicate_groups);
        self.telemetry.finish_scan();
        Ok(report)
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
        progress: Option<&(dyn Fn(EnhancedDeletionProgress) + Send + Sync)>,
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

        let total_candidates = files_to_clean.len() as u64;

        if let Some(cb) = progress {
            cb(EnhancedDeletionProgress {
                progress: if total_candidates == 0 { 0.0 } else { 5.0 },
                message: "Validating selections and preparing recovery point".to_string(),
                stage: "validation",
                eta_ms: None,
                files_per_s: None,
                mb_per_s: None,
            });
        }

        // Validate before deletion
        let preparation = self.validate_and_prepare_deletion(&files_to_clean).await?;
        if let Some(t) = token {
            if t.is_cancelled() {
                return Err("cancelled".into());
            }
        }

        let describe_block_reason = |reason: &BlockReason| -> &'static str {
            match reason {
                BlockReason::InUse => "File is currently in use",
                BlockReason::SystemCritical => "System-critical item blocked",
                BlockReason::PermissionDenied => "Insufficient permissions",
                BlockReason::UserProtected => "Item protected by user policy",
            }
        };

        let mut error_lookup: HashMap<PathBuf, String> = HashMap::new();
        for err in &preparation.validation_result.errors {
            error_lookup.insert(err.file_path.clone(), err.message.clone());
        }

        let mut eligible_files = Vec::new();
        for file in files_to_clean.into_iter() {
            let path_buf = PathBuf::from(&file.base.path);
            match preparation.validation_result.file_states.get(&path_buf) {
                Some(FileValidationState::Blocked(reason)) => {
                    let reason_msg = error_lookup
                        .get(&path_buf)
                        .cloned()
                        .unwrap_or_else(|| describe_block_reason(reason).to_string());
                    failed_files.push(FailedDeletion {
                        path: file.base.path.clone(),
                        reason: reason_msg,
                    });
                }
                Some(FileValidationState::RequiresConfirmation) => {
                    if allow_low_safety {
                        eligible_files.push(file);
                    } else {
                        failed_files.push(FailedDeletion {
                            path: file.base.path.clone(),
                            reason:
                                "Requires confirmation. Enable Risky Mode to include this item."
                                    .into(),
                        });
                    }
                }
                _ => eligible_files.push(file),
            }
        }

        let total_files = eligible_files.len() as u64;

        if let Some(cb) = progress {
            cb(EnhancedDeletionProgress {
                progress: if total_files == 0 { 20.0 } else { 20.0 },
                message: if total_files > 0 {
                    format!(
                        "Validation complete. {} file(s) cleared for deletion",
                        total_files
                    )
                } else {
                    "Validation complete. No files cleared for deletion".to_string()
                },
                stage: "validation",
                eta_ms: None,
                files_per_s: None,
                mb_per_s: None,
            });
        }

        if eligible_files.is_empty() {
            let detail = failed_files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.reason))
                .take(3)
                .collect::<Vec<_>>()
                .join(", ");
            let message = if detail.is_empty() {
                "No files eligible for deletion after validation".to_string()
            } else {
                format!(
                    "No files eligible for deletion after validation: {}",
                    detail
                )
            };
            return Err(message);
        }

        // Proceed with deletion
        if let Some(t) = token {
            if t.is_cancelled() {
                return Err("cancelled".into());
            }
        }

        let mut tracker = ThroughputTracker::default();
        let mut processed_files = 0u64;
        let mut processed_bytes = 0u64;
        let progress_base = 20.0;
        let progress_scale = 80.0;

        if let Some(cb) = progress {
            cb(EnhancedDeletionProgress {
                progress: progress_base,
                message: format!(
                    "Starting enhanced cleaning ({} file{})",
                    total_files,
                    if total_files == 1 { "" } else { "s" }
                ),
                stage: "deleting",
                eta_ms: None,
                files_per_s: None,
                mb_per_s: None,
            });
        }

        for file in eligible_files {
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

            processed_files += 1;
            processed_bytes = processed_bytes.saturating_add(file.base.size);
            let progress_factor = if total_files > 0 {
                processed_files as f32 / total_files as f32
            } else {
                1.0
            };
            let (eta_ms, fps, mbs) = tracker.tick(processed_files, processed_bytes, total_files);
            if let Some(cb) = progress {
                cb(EnhancedDeletionProgress {
                    progress: (progress_base + progress_scale * progress_factor).min(100.0),
                    message: format!(
                        "Cleaning files… {}/{} processed",
                        processed_files, total_files
                    ),
                    stage: "deleting",
                    eta_ms,
                    files_per_s: fps,
                    mb_per_s: mbs,
                });
            }
        }

        if let Some(cb) = progress {
            cb(EnhancedDeletionProgress {
                progress: 100.0,
                message: "Finalizing enhanced cleaning".to_string(),
                stage: "finalizing",
                eta_ms: Some(0),
                files_per_s: None,
                mb_per_s: None,
            });
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
        if !Self::is_osascript_disabled() {
            match tokio::process::Command::new("osascript")
                .arg("-e")
                .arg(format!(
                    "tell application \"Finder\" to move POSIX file \"{}\" to trash",
                    path.display()
                ))
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    return true;
                }
                Ok(output) => {
                    log::warn!(
                        "Finder trash command (enhanced) failed (status {:?}): {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                Err(err) => {
                    log::warn!("Enhanced cleaner AppleScript move failed: {}", err);
                }
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

    fn is_osascript_disabled() -> bool {
        env::var("MACOS_OPTIMIZER_DISABLE_OSA")
            .map(|value| {
                let lowercase = value.trim().to_ascii_lowercase();
                lowercase == "1" || lowercase == "true" || lowercase == "yes"
            })
            .unwrap_or(false)
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

    fn apply_deferred_safety(file: &mut EnhancedCleanableFile) {
        file.safety_metrics = SafetyMetrics {
            base_score: 45,
            confidence: 0.2,
            risk_factors: vec![RiskFactor::SafetyAnalysisDeferred],
            safety_flags: SafetyFlags::default(),
            recommendation: SafetyRecommendation::CautionAdvised,
        };
        file.auto_select_score = AutoSelectScore::new();
        file.base.safe_to_delete = false;
        file.base.safety_score = file.safety_metrics.base_score;
        file.base.auto_select = false;

        let policy = policy_for_category(&file.base.category);
        policy.enforce(&mut file.base);
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
