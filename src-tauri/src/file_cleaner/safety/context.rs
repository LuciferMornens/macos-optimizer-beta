use chrono::{DateTime, Utc};
use std::fs::{self, Metadata};
use std::path::Path;

#[derive(Debug)]
pub(super) struct PathContext<'a> {
    pub(super) path: &'a Path,
    lower: String,
    segments_lower: Vec<String>,
    segments_original: Vec<String>,
    metadata: Option<Metadata>,
}

impl<'a> PathContext<'a> {
    pub(super) fn new(path: &'a Path) -> Self {
        let lower = path.to_string_lossy().to_lowercase();
        let segments_original: Vec<String> = path
            .iter()
            .filter_map(|component| component.to_str())
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();
        let segments_lower = segments_original
            .iter()
            .map(|segment| segment.to_lowercase())
            .collect();
        let metadata = fs::metadata(path).ok();

        Self {
            path,
            lower,
            segments_lower,
            segments_original,
            metadata,
        }
    }

    pub(super) fn lower(&self) -> &str {
        &self.lower
    }

    pub(super) fn size(&self) -> Option<u64> {
        self.metadata.as_ref().map(|m| m.len())
    }

    pub(super) fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
    }

    pub(super) fn segment_contains_any(&self, keywords: &[&str]) -> bool {
        self.segments_lower
            .iter()
            .any(|segment| keywords.iter().any(|keyword| segment.contains(keyword)))
    }

    pub(super) fn contains_keyword(&self, keyword: &str) -> bool {
        self.lower.contains(keyword)
    }

    pub(super) fn contains_sequence(&self, sequence: &[&str]) -> bool {
        if sequence.is_empty() {
            return true;
        }
        let target_len = sequence.len();
        if target_len > self.segments_lower.len() {
            return false;
        }

        self.segments_lower
            .windows(target_len)
            .any(|window| Self::sequence_matches(window, sequence))
    }

    pub(super) fn ends_with_sequence(&self, sequence: &[&str]) -> bool {
        if sequence.is_empty() {
            return true;
        }
        let target_len = sequence.len();
        if target_len > self.segments_lower.len() {
            return false;
        }

        let start = self.segments_lower.len() - target_len;
        Self::sequence_matches(&self.segments_lower[start..], sequence)
    }

    pub(super) fn original_segment(&self, index: usize) -> Option<&str> {
        self.segments_original.get(index).map(|s| s.as_str())
    }

    pub(super) fn original_segments(&self) -> &[String] {
        &self.segments_original
    }

    pub(super) fn age_days_modified(&self) -> Option<i64> {
        self.metadata
            .as_ref()
            .and_then(|md| md.modified().ok())
            .map(|modified| {
                let modified_time = DateTime::<Utc>::from(modified);
                Utc::now().signed_duration_since(modified_time).num_days()
            })
    }

    pub(super) fn age_days_created(&self) -> Option<i64> {
        self.metadata
            .as_ref()
            .and_then(|md| md.created().ok())
            .map(|created| {
                let created_time = DateTime::<Utc>::from(created);
                Utc::now().signed_duration_since(created_time).num_days()
            })
    }

    pub(super) fn segments_lower(&self) -> &[String] {
        &self.segments_lower
    }

    fn sequence_matches(window: &[String], sequence: &[&str]) -> bool {
        window
            .iter()
            .zip(sequence.iter())
            .all(|(segment, pattern)| *pattern == "*" || segment == pattern)
    }
}
