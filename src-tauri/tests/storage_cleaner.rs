use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lazy_static::lazy_static;
use macos_optimizer_lib::StorageFileCleaner as FileCleaner;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

lazy_static! {
    static ref TEST_ENV_GUARD: Mutex<()> = Mutex::new(());
}

const TEST_RULES_JSON: &str = r#"{
    "categories": [
        {
            "name": "Test Downloads",
            "paths": ["~/Downloads"],
            "safe": true,
            "max_depth": 2,
            "min_size_kb": 2,
            "extensions": ["crdownload"]
        }
    ]
}"#;

struct StorageTestEnv {
    temp_home: TempDir,
    prev_home: Option<String>,
    prev_rules_override: Option<String>,
    prev_disable_osa: Option<String>,
}

impl StorageTestEnv {
    fn new() -> Self {
        let temp_home = TempDir::new().expect("temp home dir");
        let rules_path = temp_home.path().join("rules.json");
        fs::write(&rules_path, TEST_RULES_JSON).expect("write rules override");
        fs::create_dir_all(temp_home.path().join(".Trash")).expect("create trash");
        fs::create_dir_all(temp_home.path().join("Downloads")).expect("create downloads");

        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_home.path());

        let prev_rules_override = std::env::var("MACOS_OPTIMIZER_RULES_OVERRIDE").ok();
        std::env::set_var("MACOS_OPTIMIZER_RULES_OVERRIDE", &rules_path);

        let prev_disable_osa = std::env::var("MACOS_OPTIMIZER_DISABLE_OSA").ok();
        std::env::set_var("MACOS_OPTIMIZER_DISABLE_OSA", "1");

        StorageTestEnv {
            temp_home,
            prev_home,
            prev_rules_override,
            prev_disable_osa,
        }
    }

    fn home(&self) -> &Path {
        self.temp_home.path()
    }

    fn trash_dir(&self) -> PathBuf {
        self.home().join(".Trash")
    }

    fn create_file(&self, relative: &str, size: usize) -> PathBuf {
        let path = self.home().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut file = fs::File::create(&path).expect("create file");
        if size > 0 {
            let content = vec![0u8; size];
            file.write_all(&content).expect("write file");
        }
        drop(file);

        path
    }
}

impl Drop for StorageTestEnv {
    fn drop(&mut self) {
        if let Some(prev) = self.prev_home.take() {
            std::env::set_var("HOME", prev);
        } else {
            std::env::remove_var("HOME");
        }

        if let Some(prev) = self.prev_rules_override.take() {
            std::env::set_var("MACOS_OPTIMIZER_RULES_OVERRIDE", prev);
        } else {
            std::env::remove_var("MACOS_OPTIMIZER_RULES_OVERRIDE");
        }

        if let Some(prev) = self.prev_disable_osa.take() {
            std::env::set_var("MACOS_OPTIMIZER_DISABLE_OSA", prev);
        } else {
            std::env::remove_var("MACOS_OPTIMIZER_DISABLE_OSA");
        }
    }
}

fn acquire_env_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_ENV_GUARD.lock().expect("env guard")
}

#[tokio::test]
async fn scan_detects_only_stale_downloads() {
    let _guard = acquire_env_guard();
    let env = StorageTestEnv::new();
    let large = env.create_file("Downloads/large.crdownload", 4096);
    env.create_file("Downloads/small.crdownload", 1024);

    let mut cleaner = FileCleaner::new();
    let token = CancellationToken::new();

    let _report = cleaner
        .scan_system_with_cancel(&token)
        .await
        .expect("scan should succeed");

    let items = cleaner.get_cleanable_files().clone();
    let downloads_dir = env.home().join("Downloads").to_string_lossy().to_string();
    assert!(
        !items.iter().any(|entry| entry.path == downloads_dir),
        "Downloads directory should never be surfaced as a cleanable item: {:?}",
        items
    );
    let matching: Vec<_> = items
        .iter()
        .filter(|entry| entry.path.ends_with(".crdownload"))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "only large .crdownload should be reported: {:?}",
        items
    );
    let entry = matching[0];
    assert_eq!(entry.category, "Test Downloads");
    assert!(entry.safe_to_delete);
    assert!(!entry.auto_select);
    assert_eq!(entry.path, large.to_string_lossy());
}

#[tokio::test]
async fn clean_files_moves_items_to_trash() {
    let _guard = acquire_env_guard();
    let env = StorageTestEnv::new();
    let target = env.create_file("Downloads/remove.crdownload", 4096);

    let mut cleaner = FileCleaner::new();
    let token = CancellationToken::new();
    cleaner
        .scan_system_with_cancel(&token)
        .await
        .expect("scan should succeed");

    let original_size = fs::metadata(&target).expect("metadata").len();
    let (freed, removed) = cleaner
        .clean_files_with_cancel(vec![target.to_string_lossy().into_owned()], &token)
        .await
        .expect("clean should succeed");

    assert_eq!(removed, 1);
    assert_eq!(freed, original_size);
    assert!(fs::read_dir(env.trash_dir()).expect("read trash").count() >= 1);
    assert!(!target.exists(), "file should be moved to trash");
}

#[tokio::test]
async fn empty_trash_removes_items() {
    let _guard = acquire_env_guard();
    let env = StorageTestEnv::new();
    let target = env.create_file("Downloads/to-trash.crdownload", 4096);

    let mut cleaner = FileCleaner::new();
    let token = CancellationToken::new();
    cleaner
        .scan_system_with_cancel(&token)
        .await
        .expect("scan should succeed");
    cleaner
        .clean_files_with_cancel(vec![target.to_string_lossy().into_owned()], &token)
        .await
        .expect("cleanup should succeed");

    let trash_before = fs::read_dir(env.trash_dir()).expect("read trash").count();
    assert!(trash_before >= 1);

    let trash_dir = env.trash_dir();
    let reported_size_before = cleaner
        .get_directory_size_async(&trash_dir)
        .await
        .expect("size before");
    assert!(
        reported_size_before >= 4096,
        "trash should report at least original file size"
    );

    let (freed, removed) = cleaner
        .empty_trash_with_cancel(&token)
        .await
        .expect("empty trash should succeed");

    assert_eq!(
        freed, reported_size_before,
        "freed bytes should match trash size"
    );
    assert_eq!(
        removed, trash_before,
        "removed count should match trash entries"
    );

    let reported_size_after = cleaner
        .get_directory_size_async(&trash_dir)
        .await
        .expect("size after");
    assert_eq!(
        reported_size_after, 0,
        "trash directory should report empty size"
    );

    let trash_after = fs::read_dir(&trash_dir).expect("read trash").count();
    assert_eq!(trash_after, 0, "trash should be empty");
}

#[tokio::test]
async fn empty_trash_invalidation_refreshes_home_aggregate_size() {
    let _guard = acquire_env_guard();
    let env = StorageTestEnv::new();
    let target = env.create_file("Downloads/cache-me.crdownload", 4096);

    let mut cleaner = FileCleaner::new();
    let token = CancellationToken::new();
    cleaner
        .scan_system_with_cancel(&token)
        .await
        .expect("scan should succeed");
    cleaner
        .clean_files_with_cancel(vec![target.to_string_lossy().into_owned()], &token)
        .await
        .expect("cleanup should succeed");

    // Prime cache for an ancestor directory entry.
    let home_path = env.home().to_path_buf();
    let home_size_before = cleaner
        .get_directory_size_async(&home_path)
        .await
        .expect("home size before");

    let (freed, removed) = cleaner
        .empty_trash_with_cancel(&token)
        .await
        .expect("empty trash should succeed");
    assert_eq!(removed, 1);
    assert!(freed > 0, "trash cleanup should reclaim bytes");

    // Parent size must be refreshed after trash cleanup (not served stale from cache).
    let home_size_after = cleaner
        .get_directory_size_async(&home_path)
        .await
        .expect("home size after");
    assert!(
        home_size_after < home_size_before,
        "home aggregate should shrink after emptying trash (before={}, after={})",
        home_size_before,
        home_size_after
    );
}
