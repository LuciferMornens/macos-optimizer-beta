use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs as async_fs;
use tokio::task;
use tokio_util::sync::CancellationToken;

const QUICK_FINGERPRINT_BYTES: usize = 64 * 1024;
const DEFAULT_DUPLICATE_TIME_BUDGET: Duration = Duration::from_secs(12);

#[derive(Debug, Default)]
pub struct DuplicateScanResult {
    pub groups: Vec<DuplicateGroup>,
    pub analyzed_files: usize,
    pub skipped_files: usize,
    pub truncated: bool,
}

pub struct DuplicateDetector {
    hash_cache: HashMap<PathBuf, String>,
    quick_cache: HashMap<PathBuf, (u64, String)>,
}

impl DuplicateDetector {
    pub fn new() -> Self {
        Self {
            hash_cache: HashMap::new(),
            quick_cache: HashMap::new(),
        }
    }

    pub async fn find_duplicates(
        &mut self,
        paths: &[PathBuf],
        token: &CancellationToken,
    ) -> Result<DuplicateScanResult, String> {
        let mut result = DuplicateScanResult::default();
        if paths.is_empty() {
            return Ok(result);
        }

        let mut size_buckets: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if token.is_cancelled() {
                return Err("cancelled".into());
            }
            match async_fs::metadata(path).await {
                Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {
                    size_buckets
                        .entry(metadata.len())
                        .or_insert_with(Vec::new)
                        .push(path.clone());
                }
                Ok(_) => {
                    result.skipped_files += 1;
                }
                Err(err) => {
                    result.skipped_files += 1;
                    log::debug!("Skipping duplicate candidate {}: {}", path.display(), err);
                }
            }
        }

        let mut buckets: Vec<_> = size_buckets.into_iter().collect();
        buckets.sort_by(|a, b| b.0.cmp(&a.0));

        let start = Instant::now();

        'outer: for (size, files) in buckets {
            if files.len() < 2 {
                continue;
            }

            if token.is_cancelled() {
                return Err("cancelled".into());
            }
            if start.elapsed() >= DEFAULT_DUPLICATE_TIME_BUDGET {
                result.truncated = true;
                break;
            }

            let mut fingerprint_map: HashMap<String, Vec<PathBuf>> = HashMap::new();

            for path in files {
                if token.is_cancelled() {
                    return Err("cancelled".into());
                }
                if start.elapsed() >= DEFAULT_DUPLICATE_TIME_BUDGET {
                    result.truncated = true;
                    break 'outer;
                }

                match self.quick_fingerprint(&path, size).await {
                    Ok(Some(fingerprint)) => {
                        result.analyzed_files += 1;
                        fingerprint_map
                            .entry(fingerprint)
                            .or_insert_with(Vec::new)
                            .push(path);
                    }
                    Ok(None) => {
                        result.skipped_files += 1;
                    }
                    Err(err) => {
                        result.skipped_files += 1;
                        log::debug!(
                            "Unable to fingerprint {} during duplicate detection: {}",
                            path.display(),
                            err
                        );
                    }
                }
            }

            for (_, candidates) in fingerprint_map.into_iter() {
                if candidates.len() < 2 {
                    continue;
                }

                if token.is_cancelled() {
                    return Err("cancelled".into());
                }
                if start.elapsed() >= DEFAULT_DUPLICATE_TIME_BUDGET {
                    result.truncated = true;
                    break 'outer;
                }

                let mut hash_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();

                for path in candidates {
                    if token.is_cancelled() {
                        return Err("cancelled".into());
                    }
                    if start.elapsed() >= DEFAULT_DUPLICATE_TIME_BUDGET {
                        result.truncated = true;
                        break 'outer;
                    }

                    match self.calculate_file_signature(&path).await {
                        Ok(signature) => {
                            hash_groups
                                .entry(signature)
                                .or_insert_with(Vec::new)
                                .push(path);
                        }
                        Err(err) => {
                            result.skipped_files += 1;
                            log::debug!(
                                "Unable to hash {} during duplicate detection: {}",
                                path.display(),
                                err
                            );
                        }
                    }
                }

                for (signature, group_files) in hash_groups.into_iter() {
                    if group_files.len() > 1 {
                        let total_size = self.calculate_total_size(&group_files);
                        let recommended_to_keep = self.determine_original(&group_files);
                        result.groups.push(DuplicateGroup {
                            hash: signature,
                            files: group_files,
                            total_size,
                            recommended_to_keep,
                        });
                    }
                }
            }

            task::yield_now().await;
        }

        Ok(result)
    }

    async fn quick_fingerprint(
        &mut self,
        path: &Path,
        size: u64,
    ) -> Result<Option<String>, String> {
        if size == 0 {
            return Ok(None);
        }

        if let Some((cached_size, fingerprint)) = self.quick_cache.get(path) {
            if *cached_size == size {
                return Ok(Some(fingerprint.clone()));
            }
        }

        let path_clone = path.to_path_buf();
        let chunk = cmp::min(QUICK_FINGERPRINT_BYTES as u64, size.max(1)) as usize;

        let fingerprint = task::spawn_blocking(move || -> Result<Option<String>, io::Error> {
            use sha2::{Digest, Sha256};

            let mut file = fs::File::open(&path_clone)?;
            let mut buffer = vec![0u8; chunk];
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                return Ok(None);
            }

            let mut hasher = Sha256::new();
            hasher.update(&buffer[..bytes_read]);
            Ok(Some(format!("{:x}", hasher.finalize())))
        })
        .await
        .map_err(|err| err.to_string())
        .and_then(|res| res.map_err(|err| err.to_string()))?;

        if let Some(ref fingerprint) = fingerprint {
            self.quick_cache
                .insert(path.to_path_buf(), (size, fingerprint.clone()));
        }

        Ok(fingerprint)
    }

    async fn calculate_file_signature(&mut self, path: &Path) -> Result<String, String> {
        if let Some(hash) = self.hash_cache.get(path) {
            return Ok(hash.clone());
        }

        let path_clone = path.to_path_buf();
        let hash = task::spawn_blocking(move || -> Result<String, io::Error> {
            use sha2::{Digest, Sha256};

            let mut file = fs::File::open(&path_clone)?;
            let metadata = file.metadata()?;
            let file_len = metadata.len();

            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; QUICK_FINGERPRINT_BYTES];

            let bytes_read = file.read(&mut buffer)?;
            if bytes_read > 0 {
                hasher.update(&buffer[..bytes_read]);
            }

            if file_len > (QUICK_FINGERPRINT_BYTES as u64 * 2) {
                let mid_offset = file_len / 2;
                let seek_offset = mid_offset.saturating_sub((QUICK_FINGERPRINT_BYTES / 2) as u64);
                file.seek(SeekFrom::Start(seek_offset))?;
                let bytes_read = file.read(&mut buffer)?;
                if bytes_read > 0 {
                    hasher.update(&buffer[..bytes_read]);
                }
            }

            if file_len > QUICK_FINGERPRINT_BYTES as u64 {
                let end_offset = file_len.saturating_sub(QUICK_FINGERPRINT_BYTES as u64);
                file.seek(SeekFrom::Start(end_offset))?;
                let bytes_read = file.read(&mut buffer)?;
                if bytes_read > 0 {
                    hasher.update(&buffer[..bytes_read]);
                }
            }

            hasher.update(file_len.to_le_bytes());
            Ok(format!("{:x}", hasher.finalize()))
        })
        .await
        .map_err(|err| err.to_string())
        .and_then(|res| res.map_err(|err| err.to_string()))?;

        self.hash_cache.insert(path.to_path_buf(), hash.clone());
        Ok(hash)
    }

    fn calculate_total_size(&self, files: &[PathBuf]) -> u64 {
        files
            .iter()
            .filter_map(|f| fs::metadata(f).ok())
            .map(|m| m.len())
            .sum()
    }

    fn determine_original(&self, files: &[PathBuf]) -> Option<PathBuf> {
        let mut candidates: Vec<(PathBuf, i32)> = Vec::new();

        for file in files {
            let path_str = file.to_string_lossy().to_lowercase();
            let mut score = 0;

            if path_str.contains("/applications/") {
                score += 10;
            }
            if path_str.contains("/documents/") {
                score += 8;
            }
            if !path_str.contains("/downloads/") {
                score += 5;
            }
            if !path_str.contains("/cache") && !path_str.contains("/tmp") {
                score += 5;
            }

            if let Ok(metadata) = fs::metadata(file) {
                if let Ok(created) = metadata.created() {
                    let age = DateTime::<Utc>::from(created);
                    let days_old = Utc::now().signed_duration_since(age).num_days();
                    score += (days_old / 30) as i32;
                }
            }

            candidates.push((file.clone(), score));
        }

        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates.first().map(|(path, _)| path.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub files: Vec<PathBuf>,
    pub total_size: u64,
    pub recommended_to_keep: Option<PathBuf>,
}
