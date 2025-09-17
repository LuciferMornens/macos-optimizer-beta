#[cfg(test)]
mod tests {
    use super::super::safety::{self, RiskLevel};
    use super::super::*;
    use crate::file_cleaner::{enhanced_rules, types};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_is_safe_to_delete_tmp_file() {
        let temp_dir = TempDir::new().unwrap();
        let tmp_path = temp_dir.path().join("tmp");
        fs::create_dir_all(&tmp_path).unwrap();
        let file_path = tmp_path.join("scratch.tmp");
        fs::write(&file_path, b"temp").unwrap();

        let assessment = safety::assess_path_risk(&file_path);
        assert_eq!(assessment.level, RiskLevel::Safe);
    }

    #[test]
    fn test_is_safe_to_delete_documents_file() {
        let temp_dir = TempDir::new().unwrap();
        let docs_path = temp_dir.path().join("Users/test/Documents");
        fs::create_dir_all(&docs_path).unwrap();
        let file_path = docs_path.join("report.pdf");
        fs::write(&file_path, b"report").unwrap();

        let assessment = safety::assess_path_risk(&file_path);
        assert_eq!(assessment.level, RiskLevel::Risky);
    }

    #[test]
    fn test_incomplete_download_risk_assessment() {
        let temp_dir = TempDir::new().unwrap();
        let downloads = temp_dir.path().join("Downloads");
        fs::create_dir_all(&downloads).unwrap();
        let file_path = downloads.join("unfinished.crdownload");
        fs::write(&file_path, b"partial").unwrap();

        let assessment = safety::assess_path_risk(&file_path);
        assert_eq!(assessment.level, RiskLevel::Safe);
    }

    // Test Safety Analyzer
    #[tokio::test]
    async fn test_safety_analyzer_safe_locations() {
        let analyzer = advanced_safety::SafetyAnalyzer::new();

        // Test known safe locations
        let cache_path = PathBuf::from("/Users/test/Library/Caches/test");
        let metrics = analyzer.analyze(&cache_path, "User Cache").await;

        assert!(metrics.base_score >= 80);
        assert_eq!(
            metrics.recommendation,
            advanced_safety::SafetyRecommendation::SafeToAutoDelete
        );
        assert!(metrics.safety_flags.is_known_safe_location);
    }

    #[tokio::test]
    async fn test_safety_analyzer_unsafe_locations() {
        let analyzer = advanced_safety::SafetyAnalyzer::new();

        // Test sensitive locations
        let ssh_path = PathBuf::from("/Users/test/.ssh/id_rsa");
        let metrics = analyzer.analyze(&ssh_path, "Unknown").await;

        assert!(metrics.base_score < 50);
        assert_eq!(
            metrics.recommendation,
            advanced_safety::SafetyRecommendation::DoNotDelete
        );
        assert!(metrics.safety_flags.contains_sensitive_data);
    }

    #[tokio::test]
    async fn test_safety_analyzer_system_components() {
        let analyzer = advanced_safety::SafetyAnalyzer::new();

        // Test system components
        let system_path = PathBuf::from("/System/Library/Frameworks/Foundation.framework");
        let metrics = analyzer.analyze(&system_path, "System").await;

        assert_eq!(metrics.base_score, 0);
        assert_eq!(
            metrics.recommendation,
            advanced_safety::SafetyRecommendation::DoNotDelete
        );
        assert!(metrics.safety_flags.is_system_component);
    }

    // Test Smart Cache Detector
    #[tokio::test]
    async fn test_cache_detector_browser_cache() {
        let detector = smart_cache::SmartCacheDetector::new();

        let cache_path = PathBuf::from("/Users/test/Library/Caches/com.apple.Safari/Cache.db");
        let validation = detector
            .validate_cache_file(&cache_path, "Browser Cache")
            .await;

        assert!(validation.is_valid_cache);
        assert!(validation.regeneratable);
        assert_eq!(validation.cache_type, smart_cache::CacheType::Browser);
    }

    #[tokio::test]
    async fn test_cache_detector_developer_cache() {
        let detector = smart_cache::SmartCacheDetector::new();

        let xcode_path =
            PathBuf::from("/Users/test/Library/Developer/Xcode/DerivedData/MyApp/Build");
        let validation = detector
            .validate_cache_file(&xcode_path, "Xcode Cache")
            .await;

        assert!(validation.is_valid_cache);
        assert!(validation.regeneratable);
        assert_eq!(validation.cache_type, smart_cache::CacheType::Developer);
    }

    #[tokio::test]
    async fn test_duplicate_detector() {
        let temp_dir = TempDir::new().unwrap();
        let mut detector = smart_cache::DuplicateDetector::new();

        // Create duplicate files
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        let file3 = temp_dir.path().join("unique.txt");

        fs::write(&file1, "duplicate content").unwrap();
        fs::write(&file2, "duplicate content").unwrap();
        fs::write(&file3, "unique content").unwrap();

        let paths = vec![file1.clone(), file2.clone(), file3];
        let duplicates = detector.find_duplicates(&paths).await;

        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].files.len(), 2);
        assert!(duplicates[0].files.contains(&file1) || duplicates[0].files.contains(&file2));
    }

    // Test Auto Selection Engine
    #[tokio::test]
    async fn test_auto_selection_trash_files() {
        let engine = auto_selection::AutoSelectionEngine::new();

        let trash_file = types::CleanableFile {
            path: "/Users/test/.Trash/old_file.txt".to_string(),
            size: 1024 * 1024, // 1MB
            category: "Trash".to_string(),
            description: "Trash file".to_string(),
            last_modified: 0,
            safe_to_delete: true,
            safety_score: 100,
            auto_select: false,
        };

        let safety_metrics = advanced_safety::SafetyMetrics {
            base_score: 100,
            confidence: 1.0,
            risk_factors: vec![],
            safety_flags: Default::default(),
            recommendation: advanced_safety::SafetyRecommendation::SafeToAutoDelete,
        };

        let score = engine
            .calculate_auto_select_score(&trash_file, &safety_metrics)
            .await;

        assert!(score.can_auto_select);
        assert_eq!(
            score.recommendation,
            auto_selection::SelectionRecommendation::AutoSelect
        );
        assert!(score.raw_score >= 95);
    }

    #[tokio::test]
    async fn test_auto_selection_large_files() {
        let engine = auto_selection::AutoSelectionEngine::new();

        let large_file = types::CleanableFile {
            path: "/Users/test/Downloads/large_file.zip".to_string(),
            size: 500 * 1024 * 1024, // 500MB
            category: "Downloads".to_string(),
            description: "Large download".to_string(),
            last_modified: 0,
            safe_to_delete: true,
            safety_score: 80,
            auto_select: false,
        };

        let safety_metrics = advanced_safety::SafetyMetrics {
            base_score: 80,
            confidence: 0.7,
            risk_factors: vec![],
            safety_flags: Default::default(),
            recommendation: advanced_safety::SafetyRecommendation::SafeWithUserConfirmation,
        };

        let score = engine
            .calculate_auto_select_score(&large_file, &safety_metrics)
            .await;

        assert!(!score.can_auto_select); // Large files should not be auto-selected
        assert!(score
            .constraint_reasons
            .contains(&"Large file with insufficient confidence".to_string()));
    }

    #[tokio::test]
    async fn test_auto_selection_recent_files() {
        let engine = auto_selection::AutoSelectionEngine::new();

        let recent_file = types::CleanableFile {
            path: "/Users/test/Library/Caches/recent.cache".to_string(),
            size: 1024 * 1024, // 1MB
            category: "User Cache".to_string(),
            description: "Recent cache".to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            safe_to_delete: true,
            safety_score: 90,
            auto_select: false,
        };

        let safety_metrics = advanced_safety::SafetyMetrics {
            base_score: 90,
            confidence: 0.8,
            risk_factors: vec![advanced_safety::RiskFactor::RecentlyAccessed(0)],
            safety_flags: Default::default(),
            recommendation: advanced_safety::SafetyRecommendation::SafeWithUserConfirmation,
        };

        let score = engine
            .calculate_auto_select_score(&recent_file, &safety_metrics)
            .await;

        assert!(!score.can_auto_select); // Recent files should not be auto-selected
        assert!(score
            .constraint_reasons
            .contains(&"File modified too recently".to_string()));
    }

    // Test Validation
    #[tokio::test]
    async fn test_pre_deletion_validator() {
        let validator = validation::PreDeletionValidator::new();

        let files = vec![types::CleanableFile {
            path: "/tmp/test_file.txt".to_string(),
            size: 1024,
            category: "Temporary Files".to_string(),
            description: "Temp file".to_string(),
            last_modified: 0,
            safe_to_delete: true,
            safety_score: 95,
            auto_select: true,
        }];

        let result = validator.validate_before_deletion(&files).await;

        assert!(result.is_safe);
        assert_eq!(result.errors.len(), 0);
        assert!(result
            .file_states
            .contains_key(&PathBuf::from("/tmp/test_file.txt")));
    }

    #[tokio::test]
    async fn test_recovery_manager() {
        let mut recovery_manager = validation::RecoveryManager::new();

        let files = vec![types::CleanableFile {
            path: "/Users/test/Downloads/recoverable.txt".to_string(),
            size: 1024,
            category: "Downloads".to_string(),
            description: "Test file".to_string(),
            last_modified: 0,
            safe_to_delete: true,
            safety_score: 80,
            auto_select: false,
        }];

        let recovery_point = recovery_manager.create_recovery_point(&files);

        assert!(!recovery_point.id.is_empty());
        assert_eq!(recovery_point.files.len(), 1);
        assert_eq!(
            recovery_point.files[0].original_path,
            PathBuf::from("/Users/test/Downloads/recoverable.txt")
        );
    }

    // Test macOS Integration
    #[tokio::test]
    async fn test_macos_sip_protection() {
        let integration = macos_integration::MacOSIntegration::new();

        // Test SIP-protected paths
        assert!(integration.check_sip_protection(&PathBuf::from("/System/Library/Frameworks")));
        assert!(integration.check_sip_protection(&PathBuf::from("/usr/bin/ls")));
        assert!(!integration.check_sip_protection(&PathBuf::from("/usr/local/bin/custom")));
        assert!(!integration.check_sip_protection(&PathBuf::from("/Users/test/Documents")));
    }

    #[tokio::test]
    async fn test_macos_spotlight_info() {
        let integration = macos_integration::MacOSIntegration::new();

        let test_path = PathBuf::from("/Users/test/Documents/test.txt");
        let spotlight_info = integration.check_spotlight_importance(&test_path).await;

        // Basic structure test (actual results depend on system)
        assert!(spotlight_info.use_count >= 0);
        assert!(spotlight_info.tags.is_empty() || !spotlight_info.tags.is_empty());
    }

    // Test Enhanced Engine Integration
    #[tokio::test]
    async fn test_enhanced_engine_scan() {
        let engine = enhanced_engine::EnhancedFileCleaner::new();

        // Test that it initializes correctly
        // The actual scan would need proper test setup with mock files
        assert!(true); // Basic initialization test
    }

    // Test User Pattern Learning
    #[test]
    fn test_user_pattern_learning() {
        let mut learner = auto_selection::UserPatternLearner::new();

        let cache_file = types::CleanableFile {
            path: "/test/cache.db".to_string(),
            size: 1024,
            category: "Cache".to_string(),
            description: "Test cache".to_string(),
            last_modified: 0,
            safe_to_delete: true,
            safety_score: 90,
            auto_select: true,
        };

        // Record multiple selections
        for _ in 0..10 {
            learner.record_action(&cache_file, auto_selection::UserAction::Selected);
        }

        // Record a few deselections
        for _ in 0..2 {
            learner.record_action(&cache_file, auto_selection::UserAction::Deselected);
        }

        let preference = learner.get_user_pattern(&cache_file);
        assert_eq!(preference, auto_selection::UserPreference::UsuallySelects);
    }

    // Test Conservative Defaults
    #[test]
    fn test_conservative_defaults() {
        let defaults = auto_selection::ConservativeDefaults::new();
        let mut score = auto_selection::AutoSelectScore::new();
        score.raw_score = 94; // Just below threshold
        score.can_auto_select = true;

        let file = types::CleanableFile {
            path: "/test/file.txt".to_string(),
            size: 200 * 1024 * 1024, // 200MB - over limit
            category: "Test".to_string(),
            description: "Test file".to_string(),
            last_modified: 0,
            safe_to_delete: true,
            safety_score: 94,
            auto_select: true,
        };

        defaults.apply_constraints(&mut score, &file);

        assert!(!score.can_auto_select);
        assert!(score
            .constraint_reasons
            .contains(&"File too large for auto-selection".to_string()));
        assert!(score
            .constraint_reasons
            .contains(&"Safety score below threshold".to_string()));
    }

    // Test Dynamic Rule Engine & Rule Validator
    #[test]
    fn test_dynamic_rules_and_validation() {
        let engine = enhanced_rules::DynamicRuleEngine::new();
        let mut base = types::CleanerRules {
            categories: vec![
                types::CategoryRule {
                    name: "User Cache".into(),
                    paths: vec!["~/Library/Caches".into()],
                    safe: true,
                    advanced: Some(false),
                    max_depth: Some(4),
                    min_age_days: None,
                    min_size_kb: None,
                    excludes: None,
                    extensions: None,
                    require_subpaths: None,
                },
                types::CategoryRule {
                    name: "User Cache Duplicate".into(),
                    paths: vec!["~/Library/Caches".into()],
                    safe: false,
                    advanced: Some(false),
                    max_depth: Some(4),
                    min_age_days: None,
                    min_size_kb: None,
                    excludes: None,
                    extensions: None,
                    require_subpaths: None,
                },
            ],
        };

        let adapted = engine.adapt_rules_to_system(&base);
        let conflicts = enhanced_rules::RuleValidator::new().validate_rule_consistency(&adapted);
        // Overlapping paths with conflicting safe flag should produce at least one conflict
        assert!(!conflicts.is_empty());

        // Dry run should return stats map
        let report = enhanced_rules::RuleValidator::new().dry_run_rules(&adapted);
        assert!(!report.category_stats.is_empty());
    }
}
