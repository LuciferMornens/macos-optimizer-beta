// src/file_cleaner/cache.rs

#[cfg(feature = "metadata-cache")]
use dashmap::DashMap;
use lazy_static::lazy_static;
use lru::LruCache;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;

pub struct DirectorySizeCache {
    cache: Arc<RwLock<LruCache<PathBuf, CachedSize>>>,
    ttl: Duration,
}

#[derive(Clone)]
struct CachedSize {
    size: u64,
    calculated_at: Instant,
    last_modified: SystemTime,
}

impl DirectorySizeCache {
    pub fn new(capacity: usize, ttl_seconds: u64) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or_else(|| NonZeroUsize::new(1000).unwrap());
        DirectorySizeCache {
            cache: Arc::new(RwLock::new(LruCache::new(cap))),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn get_or_calculate<F>(&self, path: &Path, calculator: F) -> Result<u64, String>
    where
        F: FnOnce(&Path) -> Result<u64, String>,
    {
        // Check if path metadata changed
        let metadata = fs::metadata(path).map_err(|e| format!("Failed to get metadata: {}", e))?;
        let modified = metadata
            .modified()
            .map_err(|e| format!("Failed to get modified time: {}", e))?;

        // Try to get from cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.peek(path) {
                if cached.last_modified == modified && cached.calculated_at.elapsed() < self.ttl {
                    return Ok(cached.size);
                }
            }
        }

        // Calculate new size
        let size = calculator(path)?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.put(
                path.to_path_buf(),
                CachedSize {
                    size,
                    calculated_at: Instant::now(),
                    last_modified: modified,
                },
            );
        }

        Ok(size)
    }

    pub async fn invalidate(&self, path: &Path) {
        let mut cache = self.cache.write().await;

        // Invalidate path and all children
        let keys_to_remove: Vec<_> = cache
            .iter()
            .filter(|(k, _)| k.starts_with(path))
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.pop(&key);
        }
    }
}

// Global cache instance
lazy_static! {
    pub static ref DIR_SIZE_CACHE: DirectorySizeCache = DirectorySizeCache::new(1000, 300);
}

#[cfg(feature = "metadata-cache")]
pub struct FileMetadataCache {
    cache: Arc<DashMap<PathBuf, CachedMetadata>>,
    ttl: Duration,
}

#[cfg(feature = "metadata-cache")]
#[derive(Clone)]
pub struct CachedMetadata {
    pub size: u64,
    pub modified: SystemTime,
    pub is_safe: bool,
    pub safety_score: u8,
    pub cached_at: Instant,
}

#[cfg(feature = "metadata-cache")]
impl FileMetadataCache {
    pub fn new(capacity: usize, ttl_seconds: u64) -> Self {
        FileMetadataCache {
            cache: Arc::new(DashMap::with_capacity(capacity)),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn get_or_fetch(&self, path: &Path) -> Option<CachedMetadata> {
        // Check cache first
        if let Some(entry) = self.cache.get(path) {
            if entry.cached_at.elapsed() < self.ttl {
                return Some(entry.clone());
            }
        }

        // Fetch metadata
        if let Ok(metadata) = fs::metadata(path) {
            let cached = CachedMetadata {
                size: metadata.len(),
                modified: metadata.modified().ok()?,
                is_safe: super::safety::is_safe_to_delete(path),
                safety_score: super::safety::calculate_safety_score(path, "", None, true).0,
                cached_at: Instant::now(),
            };

            self.cache.insert(path.to_path_buf(), cached.clone());
            Some(cached)
        } else {
            None
        }
    }

    pub async fn invalidate(&self, path: &Path) {
        self.cache.remove(path);
    }
}

// Global file metadata cache
#[cfg(feature = "metadata-cache")]
lazy_static! {
    pub static ref FILE_METADATA_CACHE: FileMetadataCache = FileMetadataCache::new(5000, 300);
}

// Background cache refresh
#[cfg(feature = "cache-refresh")]
use tokio::time::interval;

#[cfg(feature = "cache-refresh")]
pub struct CacheRefresher {
    dir_cache: Arc<DirectorySizeCache>,
    file_cache: Arc<FileMetadataCache>,
    paths_to_monitor: Arc<RwLock<Vec<PathBuf>>>,
}

#[cfg(feature = "cache-refresh")]
impl CacheRefresher {
    pub fn new() -> Self {
        CacheRefresher {
            dir_cache: Arc::new(DirectorySizeCache::new(1000, 300)),
            file_cache: Arc::new(FileMetadataCache::new(5000, 300)),
            paths_to_monitor: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_monitored_path(&self, path: PathBuf) {
        let mut paths = self.paths_to_monitor.write().await;
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    pub async fn start_background_refresh(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                // Only refresh during idle time
                if self.is_system_idle().await {
                    self.refresh_hot_paths().await;
                }
            }
        });
    }

    async fn refresh_hot_paths(&self) {
        let paths = self.paths_to_monitor.read().await;

        // Refresh most frequently accessed paths
        for path in paths.iter().take(10) {
            if path.exists() {
                // Refresh directory size cache
                if path.is_dir() {
                    let _ = self
                        .dir_cache
                        .get_or_calculate(path, |p| self.calculate_directory_size(p))
                        .await;
                }

                // Small delay to avoid CPU spike
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    async fn is_system_idle(&self) -> bool {
        // Simple check - could be enhanced with actual CPU usage monitoring
        true // For now, always consider system idle
    }

    fn calculate_directory_size(&self, path: &Path) -> Result<u64, String> {
        let mut total = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total += metadata.len();
                    } else if metadata.is_dir() {
                        // Recursively calculate subdirectory size
                        if let Ok(subdir_size) = self.calculate_directory_size(&entry.path()) {
                            total += subdir_size;
                        }
                    }
                }
            }
        }
        Ok(total)
    }
}
