pub mod types;
mod safety;
mod engine;
mod engine_utils;
mod cache;
mod advanced_safety;
pub mod smart_cache;
mod validation;
mod auto_selection;
mod macos_integration;
pub mod enhanced_engine;
pub mod enhanced_rules;
pub mod telemetry;

#[cfg(test)]
mod tests;

// Legacy exports for backward compatibility
pub use engine::FileCleaner;
pub use types::{CleanableFile, CleaningReport};

// Enhanced engine with all safety features - used by lib.rs
pub use enhanced_engine::{EnhancedFileCleaner, EnhancedCleaningReport};
pub use auto_selection::UserAction;
pub use enhanced_rules::{RuleValidator, DynamicRuleEngine, RuleConflict, DryRunReport};
pub use types::load_rules_result;
