use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use sysinfo::System;

/// Multi-layer safety analysis system
pub struct SafetyAnalyzer {
    pattern_detector: PatternBasedDetector,
    usage_analyzer: FileUsageAnalyzer,
    content_inspector: ContentInspector,
    system_checker: SystemIntegrationChecker,
    ml_predictor: Option<SafetyMLModel>,
}

impl SafetyAnalyzer {
    pub fn new() -> Self {
        Self {
            pattern_detector: PatternBasedDetector::new(),
            usage_analyzer: FileUsageAnalyzer::new(),
            content_inspector: ContentInspector::new(),
            system_checker: SystemIntegrationChecker::new(),
            ml_predictor: None, // ML model can be loaded later
        }
    }

    pub async fn analyze(&self, path: &Path, category: &str) -> SafetyMetrics {
        let mut base_score = 50u8; // Start neutral
        let mut confidence = 0.5f32;
        let mut risk_factors = Vec::new();
        let mut safety_flags = SafetyFlags::default();

        // Layer 1: Pattern-based analysis
        let pattern_result = self.pattern_detector.analyze(path);
        base_score = adjust_score(base_score, pattern_result.score_adjustment);
        confidence = (confidence + pattern_result.confidence) / 2.0;
        risk_factors.extend(pattern_result.risk_factors);
        safety_flags.merge(&pattern_result.flags);

        // Layer 2: Usage analysis
        let usage_result = self.usage_analyzer.analyze(path).await;
        base_score = adjust_score(base_score, usage_result.score_adjustment);
        confidence = (confidence + usage_result.confidence) / 2.0;
        risk_factors.extend(usage_result.risk_factors);
        safety_flags.merge(&usage_result.flags);

        // Layer 3: Content inspection
        if should_inspect_content(path, category) {
            let content_result = self.content_inspector.inspect(path).await;
            base_score = adjust_score(base_score, content_result.score_adjustment);
            confidence = (confidence + content_result.confidence) / 2.0;
            risk_factors.extend(content_result.risk_factors);
            safety_flags.merge(&content_result.flags);
        }

        // Layer 4: System integration check
        let system_result = self.system_checker.check(path).await;
        base_score = adjust_score(base_score, system_result.score_adjustment);
        confidence = (confidence + system_result.confidence) / 2.0;
        risk_factors.extend(system_result.risk_factors);
        safety_flags.merge(&system_result.flags);

        // Layer 5: ML prediction (if available)
        if let Some(ref ml_model) = self.ml_predictor {
            let ml_result = ml_model.predict(path, &risk_factors).await;
            base_score = adjust_score(base_score, ml_result.score_adjustment);
            confidence = (confidence + ml_result.confidence) / 2.0;
        }

        // Determine recommendation based on final score
        let recommendation = match base_score {
            95..=100 => SafetyRecommendation::SafeToAutoDelete,
            80..=94 => SafetyRecommendation::SafeWithUserConfirmation,
            60..=79 => SafetyRecommendation::ReviewRecommended,
            40..=59 => SafetyRecommendation::CautionAdvised,
            _ => SafetyRecommendation::DoNotDelete,
        };

        SafetyMetrics {
            base_score,
            confidence,
            risk_factors,
            safety_flags,
            recommendation,
        }
    }
}

/// Pattern-based detection for known safe/unsafe patterns
pub struct PatternBasedDetector {
    safe_patterns: Vec<String>,
    unsafe_patterns: Vec<String>,
    protected_directories: Vec<String>,
}

impl PatternBasedDetector {
    pub fn new() -> Self {
        Self {
            safe_patterns: vec![
                "/library/caches/".to_string(),
                "/tmp/".to_string(),
                "/.trash/".to_string(),
                "/deriveddata/".to_string(),
                "/.cache/".to_string(),
                "/temporaryitems/".to_string(),
            ],
            unsafe_patterns: vec![
                ".ssh".to_string(),
                ".gnupg".to_string(),
                ".keychain".to_string(),
                "passwords".to_string(),
                "credentials".to_string(),
                ".env".to_string(),
                ".pem".to_string(),
                ".key".to_string(),
                ".cert".to_string(),
                "vault".to_string(),
                "wallet".to_string(),
                "backup".to_string(),
                "important".to_string(),
                "personal".to_string(),
                "private".to_string(),
                "secret".to_string(),
            ],
            protected_directories: vec![
                "/System".to_string(),
                "/Library/Preferences".to_string(),
                "/Library/Keychains".to_string(),
                "/Users/*/Documents".to_string(),
                "/Users/*/Desktop".to_string(),
                "/Users/*/Pictures".to_string(),
            ],
        }
    }

    pub fn analyze(&self, path: &Path) -> PatternAnalysisResult {
        let path_str = path.to_string_lossy().to_lowercase();
        let mut score_adjustment = 0i8;
        let mut confidence = 0.7f32;
        let mut risk_factors = Vec::new();
        let mut flags = SafetyFlags::default();

        // Check safe patterns
        for pattern in &self.safe_patterns {
            if path_str.contains(pattern) {
                score_adjustment += 20;
                confidence += 0.1;
                flags.is_known_safe_location = true;
                break;
            }
        }

        // Check unsafe patterns
        for pattern in &self.unsafe_patterns {
            if path_str.contains(pattern) {
                score_adjustment -= 40;
                confidence += 0.15;
                risk_factors.push(RiskFactor::ContainsSensitivePattern(pattern.clone()));
                flags.contains_sensitive_data = true;
                break;
            }
        }

        // Check protected directories
        for protected in &self.protected_directories {
            if path_str.starts_with(&protected.to_lowercase()) {
                score_adjustment -= 30;
                confidence += 0.1;
                risk_factors.push(RiskFactor::InProtectedDirectory(protected.clone()));
                flags.in_protected_location = true;
                break;
            }
        }

        PatternAnalysisResult {
            score_adjustment,
            confidence: confidence.min(1.0),
            risk_factors,
            flags,
        }
    }
}

/// Analyzes file usage patterns and access history
pub struct FileUsageAnalyzer {
    access_threshold_days: i64,
}

impl FileUsageAnalyzer {
    pub fn new() -> Self {
        Self {
            access_threshold_days: 7,
        }
    }

    pub async fn analyze(&self, path: &Path) -> UsageAnalysisResult {
        let mut score_adjustment = 0i8;
        let mut confidence = 0.6f32;
        let mut risk_factors = Vec::new();
        let mut flags = SafetyFlags::default();

        // Check last access time
        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(accessed) = metadata.accessed() {
                let accessed_time = DateTime::<Utc>::from(accessed);
                let days_since_access = Utc::now().signed_duration_since(accessed_time).num_days();

                if days_since_access < self.access_threshold_days {
                    score_adjustment -= 20;
                    confidence += 0.1;
                    risk_factors.push(RiskFactor::RecentlyAccessed(days_since_access));
                    flags.recently_accessed = true;
                } else if days_since_access > 90 {
                    score_adjustment += 10;
                    confidence += 0.05;
                    flags.stale_file = true;
                }
            }

            // Check if file is currently open by any process
            if self.is_file_in_use(path).await {
                score_adjustment -= 50;
                confidence += 0.2;
                risk_factors.push(RiskFactor::CurrentlyInUse);
                flags.currently_in_use = true;
            }
        }

        UsageAnalysisResult {
            score_adjustment,
            confidence: confidence.min(1.0),
            risk_factors,
            flags,
        }
    }

    async fn is_file_in_use(&self, path: &Path) -> bool {
        let mut system = System::new_all();
        system.refresh_all();

        let path_str = path.to_string_lossy();

        for process in system.processes_by_name(&path_str) {
            // Check if process has the file open
            // This is a simplified check - in production, you'd use lsof or similar
            for arg in process.cmd() {
                if arg.contains(&*path_str) {
                    return true;
                }
            }
        }

        false
    }
}

/// Inspects file content for sensitive data
pub struct ContentInspector {
    sensitive_patterns: Vec<regex::Regex>,
    binary_signatures: HashMap<Vec<u8>, String>,
}

impl ContentInspector {
    pub fn new() -> Self {
        let mut sensitive_patterns = Vec::new();

        // Common patterns for sensitive data
        if let Ok(re) = regex::Regex::new(r"(?i)(api[_-]?key|secret|password|token|credential)") {
            sensitive_patterns.push(re);
        }
        if let Ok(re) = regex::Regex::new(r"[A-Za-z0-9+/]{40,}={0,2}") {
            // Base64 encoded data
            sensitive_patterns.push(re);
        }
        if let Ok(re) = regex::Regex::new(r"-----BEGIN (RSA |EC |)PRIVATE KEY-----") {
            sensitive_patterns.push(re);
        }

        let mut binary_signatures = HashMap::new();
        // Common file signatures (magic bytes)
        binary_signatures.insert(vec![0x50, 0x4B, 0x03, 0x04], "ZIP".to_string());
        binary_signatures.insert(vec![0x89, 0x50, 0x4E, 0x47], "PNG".to_string());
        binary_signatures.insert(vec![0xFF, 0xD8, 0xFF], "JPEG".to_string());
        binary_signatures.insert(vec![0x25, 0x50, 0x44, 0x46], "PDF".to_string());

        Self {
            sensitive_patterns,
            binary_signatures,
        }
    }

    pub async fn inspect(&self, path: &Path) -> ContentInspectionResult {
        let mut score_adjustment = 0i8;
        let mut confidence = 0.5f32;
        let mut risk_factors = Vec::new();
        let mut flags = SafetyFlags::default();

        // Skip inspection for very large files
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.len() > 100 * 1024 * 1024 {
                // 100MB
                return ContentInspectionResult {
                    score_adjustment: 0,
                    confidence: 0.3,
                    risk_factors: vec![],
                    flags,
                };
            }
        }

        // Try to read file header for binary detection
        if let Ok(mut file) = fs::File::open(path) {
            use std::io::Read;
            let mut header = vec![0u8; 16];
            if file.read(&mut header).is_ok() {
                // Check for binary signatures
                for (sig, file_type) in &self.binary_signatures {
                    if header.starts_with(sig) {
                        flags.is_binary_file = true;
                        if file_type == "ZIP" || file_type == "PDF" {
                            // These might contain important data
                            score_adjustment -= 10;
                            risk_factors
                                .push(RiskFactor::PotentiallyImportantFileType(file_type.clone()));
                        }
                        break;
                    }
                }
            }
        }

        // For text files, scan for sensitive patterns
        if !flags.is_binary_file {
            if let Ok(content) = fs::read_to_string(path) {
                for pattern in &self.sensitive_patterns {
                    if pattern.is_match(&content) {
                        score_adjustment -= 30;
                        confidence += 0.15;
                        risk_factors.push(RiskFactor::ContainsSensitiveContent);
                        flags.contains_sensitive_data = true;
                        break;
                    }
                }
            }
        }

        ContentInspectionResult {
            score_adjustment,
            confidence: confidence.min(1.0),
            risk_factors,
            flags,
        }
    }
}

/// Checks system integration and dependencies
pub struct SystemIntegrationChecker {}

impl SystemIntegrationChecker {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn check(&self, path: &Path) -> SystemIntegrationResult {
        let mut score_adjustment = 0i8;
        let mut confidence = 0.6f32;
        let mut risk_factors = Vec::new();
        let mut flags = SafetyFlags::default();

        // Check if file is referenced in system databases
        if self.check_spotlight_index(path).await {
            score_adjustment -= 15;
            confidence += 0.1;
            risk_factors.push(RiskFactor::IndexedBySpotlight);
            flags.system_indexed = true;
        }

        // Check for Time Machine exclusions
        if self.check_time_machine_status(path).await {
            score_adjustment += 10; // If excluded from backup, likely safe to delete
            confidence += 0.05;
            flags.excluded_from_backup = true;
        }

        // Check if it's a system framework or library
        let path_str = path.to_string_lossy().to_lowercase();
        if path_str.contains("/system/library/") || path_str.contains("/library/frameworks/") {
            score_adjustment -= 50;
            confidence += 0.2;
            risk_factors.push(RiskFactor::SystemComponent);
            flags.is_system_component = true;
        }

        SystemIntegrationResult {
            score_adjustment,
            confidence: confidence.min(1.0),
            risk_factors,
            flags,
        }
    }

    async fn check_spotlight_index(&self, _path: &Path) -> bool {
        // In production, would use mdfind or MDQuery APIs
        false
    }

    async fn check_time_machine_status(&self, _path: &Path) -> bool {
        // In production, would check tmutil exclusions
        false
    }
}

/// Placeholder for ML-based safety prediction
pub struct SafetyMLModel;

impl SafetyMLModel {
    pub async fn predict(&self, _path: &Path, _risk_factors: &[RiskFactor]) -> MLPredictionResult {
        // Placeholder for ML model integration
        MLPredictionResult {
            score_adjustment: 0,
            confidence: 0.5,
        }
    }
}

// Result types for each analysis layer

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyMetrics {
    pub base_score: u8,
    pub confidence: f32,
    pub risk_factors: Vec<RiskFactor>,
    pub safety_flags: SafetyFlags,
    pub recommendation: SafetyRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskFactor {
    ContainsSensitivePattern(String),
    InProtectedDirectory(String),
    RecentlyAccessed(i64),
    CurrentlyInUse,
    ContainsSensitiveContent,
    PotentiallyImportantFileType(String),
    IndexedBySpotlight,
    SystemComponent,
    HasActiveDependencies,
    LargeFileSize(u64),
    UserCreatedContent,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SafetyFlags {
    pub is_known_safe_location: bool,
    pub contains_sensitive_data: bool,
    pub in_protected_location: bool,
    pub recently_accessed: bool,
    pub currently_in_use: bool,
    pub stale_file: bool,
    pub is_binary_file: bool,
    pub system_indexed: bool,
    pub excluded_from_backup: bool,
    pub is_system_component: bool,
}

impl SafetyFlags {
    pub fn merge(&mut self, other: &SafetyFlags) {
        self.is_known_safe_location |= other.is_known_safe_location;
        self.contains_sensitive_data |= other.contains_sensitive_data;
        self.in_protected_location |= other.in_protected_location;
        self.recently_accessed |= other.recently_accessed;
        self.currently_in_use |= other.currently_in_use;
        self.stale_file |= other.stale_file;
        self.is_binary_file |= other.is_binary_file;
        self.system_indexed |= other.system_indexed;
        self.excluded_from_backup |= other.excluded_from_backup;
        self.is_system_component |= other.is_system_component;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SafetyRecommendation {
    SafeToAutoDelete,
    SafeWithUserConfirmation,
    ReviewRecommended,
    CautionAdvised,
    DoNotDelete,
}

pub(crate) struct PatternAnalysisResult {
    pub score_adjustment: i8,
    pub confidence: f32,
    pub risk_factors: Vec<RiskFactor>,
    pub flags: SafetyFlags,
}

pub(crate) struct UsageAnalysisResult {
    pub score_adjustment: i8,
    pub confidence: f32,
    pub risk_factors: Vec<RiskFactor>,
    pub flags: SafetyFlags,
}

pub(crate) struct ContentInspectionResult {
    pub score_adjustment: i8,
    pub confidence: f32,
    pub risk_factors: Vec<RiskFactor>,
    pub flags: SafetyFlags,
}

pub(crate) struct SystemIntegrationResult {
    pub score_adjustment: i8,
    pub confidence: f32,
    pub risk_factors: Vec<RiskFactor>,
    pub flags: SafetyFlags,
}

pub(crate) struct MLPredictionResult {
    pub score_adjustment: i8,
    pub confidence: f32,
}

// Helper functions

pub(crate) fn adjust_score(current: u8, adjustment: i8) -> u8 {
    if adjustment >= 0 {
        current.saturating_add(adjustment as u8).min(100)
    } else {
        current.saturating_sub(adjustment.unsigned_abs() as u8)
    }
}

pub(crate) fn should_inspect_content(path: &Path, category: &str) -> bool {
    // Skip content inspection for known safe categories
    let safe_categories = [
        "System Cache",
        "User Cache",
        "Browser Cache",
        "Temporary Files",
        "Trash",
    ];

    if safe_categories.contains(&category) {
        return false;
    }

    // Skip for very large files
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.len() > 100 * 1024 * 1024 {
            // 100MB
            return false;
        }
    }

    true
}
