mod advanced_safety;
mod auto_selection;
mod cache;
mod dependency_checker;
pub mod duplicate_detector;
mod engine;
mod engine_utils;
pub mod enhanced_engine;
pub mod enhanced_rules;
mod macos_integration;
pub mod process_snapshot;
mod safety;
pub mod smart_cache;
pub mod telemetry;
pub mod types;
mod validation;

#[cfg(test)]
mod tests;

// Legacy exports for backward compatibility
pub use engine::FileCleaner;
pub use types::{CleanableFile, CleaningReport};

// Enhanced engine with all safety features - used by lib.rs
pub use auto_selection::UserAction;
pub use enhanced_engine::{EnhancedCleaningReport, EnhancedDeletionProgress, EnhancedFileCleaner};
pub use enhanced_rules::{DryRunReport, DynamicRuleEngine, RuleConflict, RuleValidator};
pub use types::load_rules_result;
