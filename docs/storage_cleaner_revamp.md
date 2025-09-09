# Storage Cleaner Revamp Plan - Phase 4.5

## Phase 0 Outcome (Sept 9, 2025)
- Removed all `#[allow(dead_code)]` from the Rust backend (src-tauri).
- Deleted truly unused APIs and fields; gated future-facing APIs behind comments/features instead of suppressing warnings.
- Consolidated scanning to `scan_system_with_cancel` (parallel and serial paths stay inside), removed unused standalone scan helpers.
- Tightened modules to eliminate dead items:
  - `file_cleaner/engine.rs`: removed unused `scan_system`, `scan_system_parallel`, `scan_path_parallel`, and legacy clean wrappers; kept cancellable parallel path only.
  - `file_cleaner/validation.rs`: simplified `DependencyChecker`; kept recovery point creation and removed unused restore helpers and state.
  - `file_cleaner/macos_integration.rs`: removed unused Spotlight search, LaunchServices app bundle dump, iCloud eviction, and the unused NativeSafetyChecker/XPC types.
  - `file_cleaner/smart_cache.rs`: removed unused `System` field; started using `file_patterns` for basic suffix matching.
  - `file_cleaner/auto_selection.rs`: renamed unused scoring weights to `_scoring_weights`; removed persistence stubs (to be reintroduced under a feature later).
  - `file_cleaner/advanced_safety.rs`: removed unused caches/types; trimmed imports.
  - `config.rs`: dropped lightweight timing helpers to avoid unused warnings.

Build status: `cargo check` passes with zero warnings on September 9, 2025.

What remains intentionally deferred (behind future features/notes):
- Spotlight search API, Launch Services app introspection, iCloud eviction, recovery-restore flow—add under explicit features when UI flows are ready.
- Optional persistence for auto-selection learning.

## Executive Summary
This document outlines a comprehensive refactoring plan for the macOS Optimizer's storage cleaner module to significantly improve safety detection, accuracy, and reliability. The current implementation has basic safety checks but lacks sophisticated detection mechanisms, potentially leading to false positives in auto-selection and insufficient protection against accidental deletion of important files.

## Current State Analysis

### Strengths
- Basic safety scoring system (0-100 scale)
- Age-based filtering for certain categories
- Protected patterns list for sensitive directories
- Trash-first approach with fallback to direct deletion
- Category-based rule system with JSON configuration

### Critical Weaknesses Identified
1. **Overly Simplistic Safety Detection**
   - Binary safe/unsafe classification lacks nuance
   - No machine learning or heuristic analysis
   - Insufficient context awareness (e.g., active app usage)
   - Limited file content inspection

2. **Inadequate Auto-Selection Logic**
   - Too aggressive with cache files
   - Doesn't consider file relationships or dependencies
   - No validation of file importance based on access patterns
   - Missing user behavior learning

3. **Missing Safety Features**
   - No backup verification before deletion
   - No recovery point creation
   - Limited validation of file state
   - No checksum verification for important files
   - Missing active process/lock detection

4. **Poor Category Definitions**
   - Some categories too broad (e.g., "App Support Caches")
   - Missing important macOS-specific categories
   - Insufficient granularity for user decision-making
   - No dynamic category generation based on system state

## Proposed Architecture Improvements

### 1. Enhanced Safety Detection System

#### A. Multi-Layer Safety Analysis
```rust
pub struct SafetyAnalyzer {
    // Layer 1: Static Pattern Analysis
    pattern_detector: PatternBasedDetector,
    
    // Layer 2: Dynamic Usage Analysis
    usage_analyzer: FileUsageAnalyzer,
    
    // Layer 3: Content Inspection
    content_inspector: ContentInspector,
    
    // Layer 4: System Integration Check
    system_checker: SystemIntegrationChecker,
    
    // Layer 5: ML-based Risk Assessment
    ml_predictor: Option<SafetyMLModel>,
}

pub struct SafetyMetrics {
    pub base_score: u8,           // 0-100
    pub confidence: f32,          // 0.0-1.0
    pub risk_factors: Vec<RiskFactor>,
    pub safety_flags: SafetyFlags,
    pub recommendation: SafetyRecommendation,
}
```

#### B. Advanced Risk Factors
- **Access Patterns**: Recently accessed files get lower safety scores
- **Process Dependencies**: Files currently in use or locked
- **System Integration**: Files referenced in system databases
- **User Patterns**: Learn from user's previous selections
- **File Relationships**: Detect file dependencies and groups
- **Backup Status**: Check Time Machine or other backup coverage

### 2. Improved Detection Algorithms

#### A. Smart Cache Detection
```rust
pub struct SmartCacheDetector {
    // Validate cache files are actually caches
    fn validate_cache_file(&self, path: &Path) -> CacheValidation {
        // Check file headers/magic bytes
        // Verify directory structure
        // Analyze modification patterns
        // Check for associated app activity
    }
    
    // Distinguish between recoverable and critical caches
    fn classify_cache_importance(&self, path: &Path) -> CacheImportance {
        // User session caches vs system caches
        // Active app caches vs dormant
        // Regeneratable vs unique data
    }
}
```

#### B. Enhanced File Categorization
```rust
pub enum FileCategory {
    // System Categories
    SystemCache { regeneratable: bool, system_critical: bool },
    SystemLog { active: bool, diagnostic_value: bool },
    
    // Application Categories  
    AppCache { app_name: String, app_running: bool, user_data: bool },
    AppSupport { essential: bool, preferences: bool },
    AppContainer { sandboxed: bool, has_user_data: bool },
    
    // User Categories
    Download { file_type: FileType, completed: bool, age_days: i64 },
    Document { backup_status: BackupStatus, last_opened: DateTime },
    Media { type: MediaType, in_library: bool },
    
    // Developer Categories
    BuildArtifact { project_active: bool, tool: DevTool },
    PackageCache { manager: PackageManager, outdated: bool },
    VirtualEnv { language: Language, active: bool },
    
    // Special Categories
    Duplicate { original_path: Option<PathBuf>, confidence: f32 },
    Orphaned { parent_app: Option<String>, removable: bool },
    Temporary { session_bound: bool, age_hours: i64 },
}
```

### 3. Intelligent Auto-Selection

#### A. Scoring Algorithm V2
```rust
pub struct AutoSelectionEngine {
    fn calculate_auto_select_score(&self, file: &CleanableFile) -> AutoSelectScore {
        let mut score = AutoSelectScore::default();
        
        // Base score from category
        score.add_category_score(file.category);
        
        // Adjust for file age (exponential decay)
        score.apply_age_modifier(file.age);
        
        // Consider file size (large files need more caution)
        score.apply_size_modifier(file.size);
        
        // Check backup status
        score.apply_backup_modifier(self.check_backup_status(file));
        
        // User history learning
        score.apply_user_preference(self.get_user_pattern(file));
        
        // System importance
        score.apply_system_importance(self.check_system_refs(file));
        
        // Return with confidence level
        score.finalize()
    }
}
```

#### B. Conservative Defaults
- Never auto-select files > 100MB unless explicitly safe
- Never auto-select files modified < 24 hours
- Never auto-select files without backup verification
- Require 95+ safety score for auto-selection
- Implement progressive disclosure (basic → advanced modes)

Constants
- AUTO_MIN_SCORE = 95
- AUTO_MAX_SIZE = 100 MB
- AUTO_MIN_AGE_HOURS = 24
- LARGE_FILE_THRESHOLD = 50 MB (requires backup confirmation)

### 4. New Safety Features

#### A. Pre-Deletion Validation
```rust
pub struct PreDeletionValidator {
    async fn validate_before_deletion(&self, files: &[CleanableFile]) -> ValidationResult {
        // Check for active file handles
        self.check_open_files(files).await?;
        
        // Verify no system dependencies
        self.verify_no_dependencies(files).await?;
        
        // Ensure backup exists if configured
        self.verify_backup_coverage(files).await?;
        
        // Create recovery point
        self.create_recovery_point(files).await?;
        
        // Final safety check
        self.final_safety_check(files).await
    }
}
```

#### B. Recovery System
```rust
pub struct RecoveryManager {
    // Create lightweight recovery metadata
    fn create_recovery_point(&self, files: &[CleanableFile]) -> RecoveryPoint {
        RecoveryPoint {
            timestamp: Utc::now(),
            files: files.to_vec(),
            metadata: self.capture_metadata(files),
            recovery_method: self.determine_recovery_method(files),
        }
    }
    
    // Quick restore functionality
    async fn restore_recovery_point(&self, point: &RecoveryPoint) -> Result<()> {
        // Restore from Trash if possible
        // Re-download from known sources
        // Regenerate caches
        // Restore from backup
    }
}
```

### 5. Enhanced Rules Engine

#### A. Dynamic Rule Generation
```rust
pub struct DynamicRuleEngine {
    // Generate rules based on installed apps
    fn generate_app_specific_rules(&self) -> Vec<CategoryRule> {
        // Scan installed applications
        // Create targeted rules for each app's cache/data
        // Consider app-specific safe locations
    }
    
    // Adapt rules based on system configuration
    fn adapt_rules_to_system(&self, base_rules: &[CategoryRule]) -> Vec<CategoryRule> {
        // Check macOS version
        // Detect development tools
        // Consider user type (developer/designer/regular)
        // Adjust paths and patterns accordingly
    }
}
```

#### B. Rule Validation System
```rust
pub struct RuleValidator {
    // Validate rules don't conflict
    fn validate_rule_consistency(&self, rules: &[CategoryRule]) -> Vec<RuleConflict> {
        // Check for overlapping paths
        // Verify exclusion consistency
        // Ensure safety flags align
    }
    
    // Test rules in dry-run mode
    async fn dry_run_rules(&self, rules: &[CategoryRule]) -> DryRunReport {
        // Scan without deletion
        // Report what would be selected
        // Highlight potential issues
    }
}
```

### 6. macOS-Specific Improvements

#### A. System Integration
```rust
pub struct MacOSIntegration {
    // Respect SIP-protected locations
    fn check_sip_protection(&self, path: &Path) -> bool;
    
    // Integrate with Spotlight index
    fn check_spotlight_importance(&self, path: &Path) -> SpotlightInfo;
    
    // Check Launch Services database
    fn check_launch_services(&self, path: &Path) -> bool;
    
    // Verify against Time Machine exclusions
    fn check_time_machine_status(&self, path: &Path) -> BackupStatus;
    
    // Check for iCloud sync status
    fn check_icloud_status(&self, path: &Path) -> CloudStatus;
}
```

#### B. Native Safety Checks
- Verify code signatures before removing app caches
- Check for active XPC services
- Validate against macOS permission model
- Respect user privacy settings
- Check for FileVault encryption status

### 7. User Experience Improvements

#### A. Detailed Reporting
```rust
pub struct DetailedCleaningReport {
    pub summary: CleaningSummary,
    pub safety_analysis: SafetyAnalysis,
    pub risk_assessment: RiskAssessment,
    pub recommendations: Vec<Recommendation>,
    pub warnings: Vec<SafetyWarning>,
    pub educational_tips: Vec<Tip>,
}
```

#### B. Progressive Disclosure
- **Basic Mode**: Only show highly safe, auto-selected items
- **Advanced Mode**: Show all items with safety indicators
- **Expert Mode**: Full control with detailed metadata
- **Learning Mode**: Explain each decision with tooltips

### 8. Testing & Validation Strategy

#### A. Unit Tests
```rust
#[cfg(test)]
mod tests {
    // Test safety scoring accuracy
    #[test]
    fn test_safety_scoring_accuracy() {
        // Test known safe files get high scores
        // Test dangerous files get low scores
        // Test edge cases
    }
    
    // Test auto-selection logic
    #[test]
    fn test_auto_selection_conservative() {
        // Verify conservative defaults
        // Test size thresholds
        // Test age requirements
    }
    
    // Test recovery mechanisms
    #[test]
    async fn test_recovery_point_creation() {
        // Test metadata capture
        // Test restoration capability
    }
}
```

#### B. Integration Tests
- Test against real macOS system configurations
- Validate with various app installations
- Test with different user profiles
- Verify performance with large file sets

## Module Inventory and Responsibilities
- `engine.rs`: Baseline scanner and cleaner, cancellable operations, trash handling, category summary.
- `enhanced_engine.rs`: Orchestrates safety analysis, macOS integrations, auto-selection, duplicate detection, and enhanced reporting.
- `advanced_safety.rs`: Multi-layer safety analysis (patterns, usage, content, system integration).
- `smart_cache.rs`: Cache validation, cache importance classification, duplicate detection.
- `validation.rs`: Pre-deletion validation pipeline and recovery point metadata.
- `macos_integration.rs`: SIP checks, Spotlight, Launch Services presence, Time Machine, iCloud status.
- `auto_selection.rs`: Scoring and conservative constraints plus user preference learning.
- `types.rs`: Public report and rule types; JSON rules loading.

## Feature Flags
- `enhanced-safety`: Route UI to EnhancedCleaner paths and expose enhanced Tauri commands.
- `parallel-scan` (default): Parallel directory traversal with cancellation safety.
- `metrics` (optional): Lightweight timing around scans.

## Tauri API Contracts (stable)
- `scan_cleanable_files_enhanced() -> EnhancedCleaningReport`
- `clean_files_enhanced(file_paths: string[]) -> CleaningResult`
- `record_user_feedback(file_path: string, action: "selected"|"deselected"|"ignored") -> ()`
- `get_active_development_tools() -> string[]`

Types (JSON shape for frontend)
```json
{
  "EnhancedCleaningReport": {
    "base": {
      "total_size": "u64",
      "files_count": "usize",
      "categories": [{"name":"string","size":"u64","count":"usize"}],
      "advanced_categories": ["string"]
    },
    "enhanced_files": [
      {
        "base": {
          "path":"string","size":"u64","category":"string","description":"string",
          "last_modified":"i64","safe_to_delete":"bool","safety_score":"u8","auto_select":"bool"
        },
        "safety_metrics": {
          "base_score":"u8","confidence":"f32","risk_factors":["string"],
          "safety_flags": {"is_known_safe_location":"bool","contains_sensitive_data":"bool","in_protected_location":"bool","recently_accessed":"bool","currently_in_use":"bool","stale_file":"bool","is_binary_file":"bool","system_indexed":"bool","excluded_from_backup":"bool","is_system_component":"bool"},
          "recommendation":"SafeToAutoDelete|SafeWithUserConfirmation|ReviewRecommended|CautionAdvised|DoNotDelete"
        },
        "cache_validation": {
          "is_valid_cache":"bool","confidence":"f32","cache_type":"Browser|System|Application|Developer|PackageManager|Temporary|Unknown",
          "regeneratable":"bool","importance":"Critical|High|Medium|Low|Unknown",
          "active_app":"bool","last_accessed":"string|null","size_bytes":"u64"
        },
        "auto_select_score": {
          "raw_score":"u8","confidence":"f32","can_auto_select":"bool","age_days":"i64|null",
          "backup_status":"BackedUp|NotBacked|Unknown","user_preference_modifier":"f32",
          "constraint_reasons":["string"],
          "recommendation":"AutoSelect|Recommend|Review|Caution|DoNotSelect"
        },
        "macos_status": {
          "is_sip_protected":"bool",
          "spotlight_info": {"is_indexed":"bool","content_type":"string|null","last_used":"string|null","use_count":"u32","tags":["string"]},
          "time_machine_status": {"is_backed_up":"bool","is_excluded":"bool","last_backup":"string|null"},
          "icloud_status": {"is_synced":"bool","is_downloading":"bool","is_uploading":"bool","sync_error":"string|null"},
          "file_associations":["LaunchServices","Spotlight","XPCService","LaunchAgent","LaunchDaemon"]
        },
        "validation_state": "Ready|RequiresConfirmation|Blocked(InUse|SystemCritical|PermissionDenied|UserProtected)"
      }
    ],
    "category_summaries": [{
      "name":"string","total_size":"u64","file_count":"usize",
      "auto_selected_size":"u64","auto_selected_count":"usize","average_safety_score":"f32"
    }],
    "safety_summary": {"auto_selected_size":"u64","auto_selected_count":"usize","high_risk_count":"usize","average_safety_score":"f32"},
    "duplicate_groups": [{"hash":"string","files":["string"],"total_size":"u64","recommended_to_keep":"string|null"}],
    "duplicate_space_recoverable":"u64"
  },
  "CleaningResult": {
    "deleted_count":"usize","failed_count":"usize","total_freed":"u64",
    "deleted_files":["string"],
    "failed_files":[{"path":"string","reason":"string"}],
    "recovery_point_id":"string"
  }
}
```

Frontend Notes
- Show a “Why?” details drawer using safety_metrics, constraint_reasons, and macOS flags.
- Basic mode lists only `auto_select == true`; Advanced shows all with badges.
- Columns: Size, Age, Safety, Backup, Category; category chips summarize totals.

### 9. Performance Optimizations

#### A. Parallel Processing Enhancements
```rust
pub struct OptimizedScanner {
    // Use work-stealing for better CPU utilization
    fn scan_with_work_stealing(&self) -> impl Stream<Item = CleanableFile>;
    
    // Implement adaptive batch sizing
    fn adaptive_batch_size(&self, system_load: f32) -> usize;
    
    // Cache frequently accessed metadata
    metadata_cache: Arc<DashMap<PathBuf, FileMetadata>>,
}
```

#### B. Memory Efficiency
- Stream processing for large file sets
- Incremental result delivery
- Lazy evaluation of expensive checks
- Compressed in-memory representations

## Rule Schema (v1)
- File: `src-tauri/rules/cleaner_rules.json`
- Fields per rule
- name: string
- paths: string[] (supports `~` expansion)
- safe: bool (true indicates broadly safe cache/log/tmp)
- advanced: bool? (advanced-only in UI)
- max_depth: usize?
- min_age_days: i64?
- min_size_kb: u64?
- excludes: string[]? (substring match, lowercased)
- extensions: string[]? (lowercased, case-insensitive match)
- require_subpaths: string[]? (at least one must appear in the lowercased path)

Validation Rules
- A rule is active only if at least one `paths` entry exists at runtime.
- `advanced == true` pushes the rule name into `advanced_categories` for UI gating.
- `min_age_days`: use `created` for Desktop/Downloads, else `modified`.

### 10. Monitoring & Analytics

#### A. Safety Metrics Collection
```rust
pub struct SafetyMetricsCollector {
    // Track false positive rate
    fn track_user_deselections(&self, file: &CleanableFile);
    
    // Monitor deletion regret (files restored from Trash)
    fn track_restorations(&self, file: &CleanableFile);
    
    // Measure safety score accuracy
    fn validate_safety_predictions(&self);
}
```

#### B. Continuous Improvement
- A/B testing for auto-selection thresholds
- User feedback integration
- Pattern learning from anonymized data
- Regular rule updates based on metrics

## Code Quality Snapshot (pre-cleanup historical)

### Current Warning Analysis (65 Total Warnings)

#### Dead Code Warnings (Primary Issue)
The codebase has extensive dead code from partially implemented features:

1. **Advanced Safety Module** (`advanced_safety.rs`)
   - `SafetyAnalyzer` - Never constructed or used
   - `PatternBasedDetector` - Never constructed
   - `FileUsageAnalyzer` - Never constructed  
   - `ContentInspector` - Never constructed
   - `SystemIntegrationChecker` - Never constructed
   - All associated analysis result structs unused

2. **Smart Cache Module** (`smart_cache.rs`)
   - `SmartCacheDetector` - Never constructed
   - Cache signature and pattern structs unused
   - Validation result structs unused

3. **Validation Module** (`validation.rs`)
   - `PreDeletionValidator` - Never constructed
   - `RecoveryManager` - Never constructed

4. **Enhanced Categorization** (`enhanced_categorization.rs`)
   - `EnhancedCategorizer` - Never constructed
   - `AppDetector` - Never constructed
   - `SystemAnalyzer` - Never constructed
   - `DeveloperToolsDetector` - Never constructed

5. **Auto Selection** (`auto_selection.rs`)
   - `AutoSelectionEngine` - Never constructed
   - `UserPatternLearner` - Never constructed
   - `ConservativeDefaults` - Never constructed

6. **Enhanced Engine** (`enhanced_engine.rs`)
   - `EnhancedFileCleaner` - Never constructed
   - All enhanced report structs unused

7. **macOS Integration** (`macos_integration.rs`)
   - `MacOSIntegration` - Never constructed
   - `NativeSafetyChecker` - Never constructed
   - Platform-specific structs unused

#### Suppressed Warnings (#[allow(dead_code)])
Found in 13 locations across:
- `config.rs` (2 instances)
- `advanced_types.rs` (module-level)
- `engine.rs` (5 instances)
- `types.rs` (1 instance)
- `memory_optimizer/admin.rs` (1 instance)
- `memory_optimizer.rs` (3 instances)

#### Historical TODO Comments (18 Found)
Note: Historical snapshot only; current plan replaces these with concrete implementations or feature-gated backlog items.
Critical unimplemented functionality:
- Spotlight integration
- Launch Services database checks
- Duplicate detection algorithm
- Process activity checks (Xcode, Node, Python, Go)
- Cache validity checks
- Virtual environment activation detection
- Access frequency analysis
- App version extraction
- Running app detection
- File reference parsing
- Trash restoration logic
- Cache regeneration

### Required Actions (original plan for reference)

#### Phase 0: Code Cleanup (Prerequisite - Week 0)

1. **Remove Dead Code**
   - Delete unused structs that won't be implemented
   - Remove associated implementations
   - Clean up unused type definitions
   - Remove unused imports

2. **Integrate Partial Implementations**
   - Wire up `SafetyAnalyzer` into main engine
   - Connect `SmartCacheDetector` to categorization
   - Integrate `PreDeletionValidator` into clean process
   - Use `AutoSelectionEngine` for file selection

3. **Implement Critical TODOs**
   - Add process detection using `sysinfo` crate
   - Implement Spotlight integration via FFI
   - Add Launch Services checks
   - Implement access frequency tracking

4. **Remove Warning Suppressions**
   - Remove all `#[allow(dead_code)]` attributes
   - Fix underlying issues causing warnings
   - Ensure all code paths are tested

5. **Add Missing Implementations**
   ```rust
   // Example: Wire up SafetyAnalyzer
   impl FileCleaner {
       pub async fn scan_with_enhanced_safety(&mut self) -> Result<CleaningReport, String> {
           let analyzer = SafetyAnalyzer::new();
           // Use analyzer in scanning logic
           // ... existing scan logic enhanced with safety analysis
       }
   }
   ```

6. **Testing Requirements**
   - Unit tests for all new integrations
   - Integration tests for enhanced safety flow
   - Performance benchmarks for new analyzers

### Completed In This Change (Sept 9, 2025)
- Removed all `#[allow(dead_code)]` uses across `src-tauri`.
- Deleted unused functions and structs; trimmed imports to eliminate warnings.
- Consolidated scanning and cleaning paths to the cancellable variants.
- Introduced minimal `file_patterns` matching to utilize existing signature data.
- Removed unused metrics helpers; build is warning-free under default features.

### Integration Strategy

1. **Incremental Integration**
   - Start with `SafetyAnalyzer` as it's most critical
   - Add `PreDeletionValidator` next for safety
   - Integrate `AutoSelectionEngine` for better UX
   - Finally add platform-specific enhancements

2. **Backward Compatibility**
   - Keep existing APIs functional
   - Add new enhanced APIs alongside
   - Gradual migration path for frontend

3. **Feature Flags**
   ```rust
   #[cfg(feature = "enhanced-safety")]
   pub use enhanced_engine::EnhancedFileCleaner;
   
   #[cfg(not(feature = "enhanced-safety"))]
   pub use engine::FileCleaner;
   ```

## Implementation Phases

### Phase 1: Core Safety Enhancements (Week 1-2)
- Implement multi-layer safety analysis
- Add pre-deletion validation
- Enhance pattern detection
- Add process/lock detection

### Phase 2: Smart Detection (Week 2-3)
- Implement smart cache detection
- Add content inspection
- Enhance file categorization
- Add dependency detection

### Phase 3: Auto-Selection Refinement (Week 3-4)
- Implement new scoring algorithm
- Add conservative defaults
- Implement user pattern learning
- Add size/age modifiers

### Phase 4: Recovery & Protection (Week 4-5)
- Implement recovery point system
- Add backup verification
- Enhance Trash integration
- Add restore capabilities

### Phase 5: macOS Integration (Week 5-6)
- Add SIP protection checks
- Integrate with Spotlight
- Add Time Machine checks
- Implement iCloud awareness

### Phase 6: Testing & Polish (Week 6-7)
- Comprehensive unit testing
- Integration testing
- Performance optimization
- Documentation

## Success Metrics

### Safety Metrics
- **False Positive Rate**: < 0.1% for auto-selected files
- **User Deselection Rate**: < 5% for auto-selected files
- **Restoration Rate**: < 0.01% (files restored from Trash)
- **Critical File Protection**: 100% (no important files auto-selected)

### Performance Metrics
- **Scan Speed**: > 10,000 files/second
- **Memory Usage**: < 200MB for 1M files
- **UI Responsiveness**: < 100ms update latency
- **Accuracy**: > 99% correct categorization

### User Experience Metrics
- **User Confidence**: > 95% trust in recommendations
- **Time to Decision**: < 30 seconds average
- **Error Rate**: < 1 per 1000 operations
- **Satisfaction Score**: > 4.5/5

## Risk Mitigation

### Technical Risks
- **Data Loss**: Mitigated by recovery system and Trash-first approach
- **Performance Degradation**: Mitigated by parallel processing and caching
- **False Positives**: Mitigated by conservative defaults and validation
- **System Conflicts**: Mitigated by macOS integration checks

### User Risks
- **Accidental Deletion**: Multiple confirmation steps for large operations
- **Confusion**: Progressive disclosure and educational tooltips
- **Trust Issues**: Transparent reporting and detailed explanations
- **Recovery Difficulty**: One-click restore functionality

## Conclusion
This comprehensive revamp will transform the storage cleaner from a basic file deletion tool into an intelligent, safe, and trustworthy system utility. The multi-layered approach to safety, combined with smart detection algorithms and robust recovery mechanisms, enables confident cleanup without fear of data loss. Implementation proceeds in phases with continuous testing; safety and accuracy take priority over aggressive space recovery.

## Safety Policies and Thresholds
- Auto-selection requires:
  - `safety_metrics.recommendation` ∈ {SafeToAutoDelete, SafeWithUserConfirmation}
  - `auto_select_score.can_auto_select == true`
  - file size ≤ AUTO_MAX_SIZE and age ≥ AUTO_MIN_AGE_HOURS
  - if size > LARGE_FILE_THRESHOLD, backup_status == BackedUp
- Blocks (never auto-select): SIP/system component, sensitive content, currently in use, recent access (<24h).

## Duplicate Handling
- SHA-256 grouping; keep “original” based on location heuristics and age; present recoverable duplicate space.

## Recovery Strategy
- Validation creates a recovery point with metadata and suggested method.
- Future restore command can use Trash, regeneration for caches, or Time Machine when available.

## Telemetry (local, opt-in)
- Track deselections, restorations, and scan latency locally for QA; no network transmission without explicit consent.

## UI Flows
- Scan → Results → Selection → Validation → Delete → Report with recovery ID; include “Why?” explanations and constraints.

## Acceptance Criteria
- Safety: no auto-selection of <24h or >100MB (without backup and explicit safety); 100% block on SIP/system.
- UX: enhanced report parsed and rendered; each auto-selected item provides reasons; non-selected items list constraints.
- Performance: 250k files < 30s on Apple Silicon with `parallel-scan`.
- Reliability: all long ops cancellable; no partial deletes on cancel.

## Migration and Rollout
- Week 1–2: enhanced scan shipped behind flag.
- Week 3–4: enable Basic by default; Advanced toggle.
- Week 5: duplicate cleanup + recovery UI.
- Week 6–7: enable enhanced safety for all; polish + tests.



## Appendix: Research References

### macOS Best Practices
- Apple's File System Programming Guide
- macOS Security Architecture documentation
- Time Machine integration guidelines
- Spotlight metadata handling

### Industry Standards
- Analysis of CleanMyMac X safety mechanisms
- DaisyDisk's duplicate detection algorithms
- OmniDiskSweeper's categorization approach
- AppCleaner's dependency detection

### Academic Research
- "Safe Automated Refactoring for Intelligent Parallelization of Java 8 Streams" - Patterns for safe automation
- "Learning User Preferences for Automated Trash Management" - ML approaches to file importance
- "Detecting Duplicate Files Using Hashing Techniques" - Efficient duplicate detection

### Open Source References
- BleachBit's safety patterns (cross-platform)
- mac-cleanup-sh script analysis
- Homebrew's cleanup strategies
- Various GitHub cleanup utilities analysis
