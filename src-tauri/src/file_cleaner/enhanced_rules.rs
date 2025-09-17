use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::smart_cache::AppActivityChecker;
use super::types::{CategoryRule, CleanerRules};

/// Generates additional rules based on installed/active tools and adapts base rules to the system.
pub struct DynamicRuleEngine {
    app_checker: AppActivityChecker,
}

impl DynamicRuleEngine {
    pub fn new() -> Self {
        Self {
            app_checker: AppActivityChecker::new(),
        }
    }

    /// Generate app-specific rules opportunistically. These are conservative and marked safe.
    pub fn generate_app_specific_rules(&self) -> Vec<CategoryRule> {
        let mut rules = Vec::new();
        let active = self.app_checker.get_active_development_tools();
        let lower: HashSet<String> = active.into_iter().map(|s| s.to_lowercase()).collect();

        // Xcode
        if lower.contains("xcode") || lower.contains("xcodebuild") {
            rules.push(CategoryRule {
                name: "Xcode DerivedData (Active)".to_string(),
                paths: vec!["~/Library/Developer/Xcode/DerivedData".to_string()],
                safe: true,
                advanced: Some(false),
                max_depth: Some(4),
                min_age_days: Some(2),
                min_size_kb: None,
                excludes: None,
                extensions: None,
                require_subpaths: None,
            });
        }

        // Node / npm / yarn
        if lower.contains("node")
            || lower.contains("npm")
            || lower.contains("yarn")
            || lower.contains("pnpm")
        {
            rules.push(CategoryRule {
                name: "Node Package Cache".to_string(),
                paths: vec!["~/.npm".to_string()],
                safe: true,
                advanced: Some(false),
                max_depth: Some(5),
                min_age_days: Some(7),
                min_size_kb: None,
                excludes: None,
                extensions: None,
                require_subpaths: None,
            });
        }

        // Python / pip
        if lower.contains("python")
            || lower.contains("python3")
            || lower.contains("pip")
            || lower.contains("pip3")
        {
            rules.push(CategoryRule {
                name: "Pip Cache".to_string(),
                paths: vec!["~/Library/Caches/pip".to_string()],
                safe: true,
                advanced: Some(false),
                max_depth: Some(5),
                min_age_days: Some(7),
                min_size_kb: None,
                excludes: None,
                extensions: None,
                require_subpaths: None,
            });
        }

        // Rust / cargo
        if lower.contains("cargo") || lower.contains("rustc") {
            rules.push(CategoryRule {
                name: "Cargo Registry Cache".to_string(),
                paths: vec!["~/.cargo/registry/cache".to_string()],
                safe: true,
                advanced: Some(true),
                max_depth: Some(6),
                min_age_days: Some(14),
                min_size_kb: None,
                excludes: None,
                extensions: Some(vec![
                    "crate".to_string(),
                    "tar.gz".to_string(),
                    "tgz".to_string(),
                ]),
                require_subpaths: None,
            });
        }

        // Go
        if lower.contains("go") || lower.contains("golang") {
            rules.push(CategoryRule {
                name: "Go Module Cache".to_string(),
                paths: vec!["~/go/pkg/mod/cache".to_string()],
                safe: true,
                advanced: Some(true),
                max_depth: Some(6),
                min_age_days: Some(14),
                min_size_kb: None,
                excludes: None,
                extensions: None,
                require_subpaths: None,
            });
        }

        rules
    }

    /// Adapt rules to the system by removing non-existent roots and de-duplicating.
    pub fn adapt_rules_to_system(&self, base: &CleanerRules) -> CleanerRules {
        let mut seen = HashSet::<(String, String)>::new();
        let mut categories = Vec::new();
        for rule in &base.categories {
            // Keep paths that exist or are under the home dir (expand at scan time)
            let filtered_paths: Vec<String> = rule
                .paths
                .iter()
                .filter(|p| {
                    p.starts_with("~/") || std::path::Path::new(&Self::expand_home(p)).exists()
                })
                .cloned()
                .collect();

            if filtered_paths.is_empty() {
                continue;
            }

            let key = (rule.name.clone(), filtered_paths.join("|"));
            if seen.insert(key) {
                let cloned = CategoryRule {
                    name: rule.name.clone(),
                    paths: filtered_paths,
                    safe: rule.safe,
                    advanced: rule.advanced,
                    max_depth: rule.max_depth,
                    min_age_days: rule.min_age_days,
                    min_size_kb: rule.min_size_kb,
                    excludes: rule.excludes.clone(),
                    extensions: rule.extensions.clone(),
                    require_subpaths: rule.require_subpaths.clone(),
                };
                categories.push(cloned);
            }
        }
        CleanerRules { categories }
    }

    fn expand_home(path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest).to_string_lossy().to_string();
            }
        }
        path.to_string()
    }
}

/// Validates rule consistency and provides a dry-run preview report.
#[cfg_attr(not(feature = "app"), allow(dead_code))]
pub struct RuleValidator;

#[cfg_attr(not(feature = "app"), allow(dead_code))]
impl RuleValidator {
    pub fn new() -> Self {
        Self
    }

    /// Check for overlapping path coverage and contradictory flags.
    pub fn validate_rule_consistency(&self, rules: &CleanerRules) -> Vec<RuleConflict> {
        let mut conflicts = Vec::new();
        for (i, a) in rules.categories.iter().enumerate() {
            for (_j, b) in rules.categories.iter().enumerate().skip(i + 1) {
                // Overlap heuristic: one path is a prefix of another OR names equal
                let overlap = a.paths.iter().any(|pa| {
                    b.paths
                        .iter()
                        .any(|pb| pa == pb || pa.starts_with(pb) || pb.starts_with(pa))
                });
                if overlap && a.safe != b.safe {
                    conflicts.push(RuleConflict::OverlappingPaths {
                        rule_a: a.name.clone(),
                        rule_b: b.name.clone(),
                        message: "Safe flag differs for overlapping paths".to_string(),
                    });
                }
            }
        }
        conflicts
    }

    /// Simulate a scan by listing root paths and basic filters for a quick preview.
    pub fn dry_run_rules(&self, rules: &CleanerRules) -> DryRunReport {
        let mut category_stats: HashMap<String, usize> = HashMap::new();
        for rule in &rules.categories {
            // Count how many roots exist as a proxy for activation
            let mut active_roots = 0usize;
            for raw in &rule.paths {
                let p = DynamicRuleEngine::expand_home(raw);
                if std::path::Path::new(&p).exists() {
                    active_roots += 1;
                }
            }
            category_stats.insert(rule.name.clone(), active_roots);
        }

        DryRunReport { category_stats }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleConflict {
    OverlappingPaths {
        rule_a: String,
        rule_b: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunReport {
    pub category_stats: HashMap<String, usize>,
}
