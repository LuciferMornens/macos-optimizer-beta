use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::System;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use super::dependency_checker::DependencyChecker;
use super::types::CleanableFile;

/// Pre-deletion validation system
pub struct PreDeletionValidator {
    file_lock_checker: FileLockChecker,
    dependency_checker: DependencyChecker,
    backup_verifier: BackupVerifier,
}

impl PreDeletionValidator {
    pub fn new() -> Self {
        Self {
            file_lock_checker: FileLockChecker::new(),
            dependency_checker: DependencyChecker::new(),
            backup_verifier: BackupVerifier::new(),
        }
    }

    pub async fn validate_before_deletion(&self, files: &[CleanableFile]) -> ValidationResult {
        let mut validation_result = ValidationResult {
            is_safe: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            file_states: HashMap::new(),
        };

        // Check for active file handles
        let open_files = self.file_lock_checker.check_open_files(files).await;
        if !open_files.is_empty() {
            validation_result.is_safe = false;
            for file in open_files {
                validation_result.errors.push(ValidationError {
                    file_path: file.clone(),
                    error_type: ErrorType::FileInUse,
                    message: format!("File is currently in use: {}", file.display()),
                });
                validation_result
                    .file_states
                    .insert(file, FileValidationState::Blocked(BlockReason::InUse));
            }
        }

        // Verify no system dependencies
        let dependencies = self.dependency_checker.verify_no_dependencies(files).await;
        for (file, deps) in dependencies {
            if !deps.is_empty() {
                validation_result.warnings.push(ValidationWarning {
                    file_path: file.clone(),
                    warning_type: WarningType::HasDependencies,
                    message: format!("File has {} dependencies", deps.len()),
                    dependencies: Some(deps),
                });
                validation_result
                    .file_states
                    .insert(file, FileValidationState::RequiresConfirmation);
            }
        }

        // Ensure backup exists if configured
        let backup_status = self.backup_verifier.verify_backup_coverage(files).await;
        for (file, has_backup) in backup_status {
            if !has_backup {
                validation_result.warnings.push(ValidationWarning {
                    file_path: file.clone(),
                    warning_type: WarningType::NoBackup,
                    message: format!("File not backed up: {}", file.display()),
                    dependencies: None,
                });
            }

            let current_state = validation_result
                .file_states
                .entry(file.clone())
                .or_insert(FileValidationState::Ready);

            if !has_backup && matches!(current_state, FileValidationState::Ready) {
                validation_result
                    .file_states
                    .insert(file, FileValidationState::RequiresConfirmation);
            }
        }

        // Final safety check
        for file in files {
            let path = PathBuf::from(&file.path);
            if !validation_result.file_states.contains_key(&path) {
                // Additional safety checks
                if self.is_critical_file(&path).await {
                    validation_result.is_safe = false;
                    validation_result.errors.push(ValidationError {
                        file_path: path.clone(),
                        error_type: ErrorType::CriticalFile,
                        message: format!("Critical system file: {}", path.display()),
                    });
                    validation_result.file_states.insert(
                        path,
                        FileValidationState::Blocked(BlockReason::SystemCritical),
                    );
                } else {
                    validation_result
                        .file_states
                        .insert(path, FileValidationState::Ready);
                }
            }
        }

        validation_result
    }

    async fn is_critical_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        // Check for critical system files
        let critical_patterns = vec![
            "/system/library/corefoundation",
            "/system/library/frameworks",
            "/usr/lib/",
            "/usr/bin/",
            "/sbin/",
            "/bin/",
            ".dylib",
            ".framework",
            ".kext",
        ];

        for pattern in critical_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }

        false
    }
}

/// Checks for file locks and open handles
pub struct FileLockChecker {
    lsof_available: bool,
}

impl FileLockChecker {
    pub fn new() -> Self {
        // Check if lsof is available
        let lsof_available = std::process::Command::new("which")
            .arg("lsof")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        Self { lsof_available }
    }

    pub async fn check_open_files(&self, files: &[CleanableFile]) -> Vec<PathBuf> {
        let mut open_files = Vec::new();

        if self.lsof_available {
            for file in files {
                let path = PathBuf::from(&file.path);
                if let Ok(metadata) = fs::metadata(&path) {
                    if metadata.is_dir() {
                        continue;
                    }
                }
                if self.is_file_open(&path).await {
                    open_files.push(path);
                }
            }
        } else {
            // Fallback: check using system info
            let mut system = System::new_all();
            system.refresh_all();

            for file in files {
                let path = PathBuf::from(&file.path);
                if self.is_file_in_use_fallback(&path, &system) {
                    open_files.push(path);
                }
            }
        }

        open_files
    }

    async fn is_file_open(&self, path: &Path) -> bool {
        if !self.lsof_available {
            return false;
        }

        let mut command = Command::new("lsof");
        command.arg(path);
        command.kill_on_drop(true);

        match timeout(Duration::from_secs(5), command.output()).await {
            Ok(Ok(output)) => !output.stdout.is_empty(),
            Ok(Err(err)) => {
                log::warn!("lsof check failed for {}: {}", path.display(), err);
                false
            }
            Err(_) => {
                log::warn!("lsof timed out for {}", path.display());
                false
            }
        }
    }

    fn is_file_in_use_fallback(&self, _path: &Path, _system: &System) -> bool {
        // Simplified check - in production would need more sophisticated checking
        // Using lsof is more reliable
        false
    }
}

/// Verifies backup status
pub struct BackupVerifier {
    time_machine_enabled: bool,
}

impl BackupVerifier {
    pub fn new() -> Self {
        // Check if Time Machine is enabled
        let time_machine_enabled = Self::check_time_machine_status();

        Self {
            time_machine_enabled,
        }
    }

    fn check_time_machine_status() -> bool {
        std::process::Command::new("tmutil")
            .arg("status")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub async fn verify_backup_coverage(&self, files: &[CleanableFile]) -> HashMap<PathBuf, bool> {
        let mut backup_status = HashMap::new();

        for file in files {
            let path = PathBuf::from(&file.path);
            let has_backup = self.is_backed_up(&path).await;
            backup_status.insert(path, has_backup);
        }

        backup_status
    }

    async fn is_backed_up(&self, path: &Path) -> bool {
        if !self.time_machine_enabled {
            return false;
        }

        // Check if file is excluded from Time Machine
        if let Ok(output) = Command::new("tmutil")
            .arg("isexcluded")
            .arg(path)
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // If not excluded, it's backed up
            return !stdout.contains("Excluded");
        }

        false
    }
}

/// Recovery manager for deleted files
pub struct RecoveryManager {
    recovery_points: Vec<RecoveryPoint>,
}

impl RecoveryManager {
    pub fn new() -> Self {
        Self {
            recovery_points: Vec::new(),
        }
    }

    pub fn create_recovery_point(&mut self, files: &[CleanableFile]) -> RecoveryPoint {
        let recovery_point = RecoveryPoint {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            files: files
                .iter()
                .map(|f| RecoveryFile {
                    original_path: PathBuf::from(&f.path),
                    size: f.size,
                    category: f.category.clone(),
                    metadata: self.capture_metadata(&PathBuf::from(&f.path)),
                })
                .collect(),
            recovery_method: self.determine_recovery_method(files),
        };

        self.recovery_points.push(recovery_point.clone());
        recovery_point
    }

    fn capture_metadata(&self, path: &Path) -> FileMetadata {
        let mut metadata = FileMetadata {
            permissions: None,
            owner: None,
            group: None,
            created: None,
            modified: None,
            file_type: FileType::Unknown,
        };

        if let Ok(meta) = fs::metadata(path) {
            if let Ok(created) = meta.created() {
                metadata.created = Some(DateTime::<Utc>::from(created));
            }
            if let Ok(modified) = meta.modified() {
                metadata.modified = Some(DateTime::<Utc>::from(modified));
            }

            if meta.is_file() {
                metadata.file_type = FileType::Regular;
            } else if meta.is_dir() {
                metadata.file_type = FileType::Directory;
            } else if meta.is_symlink() {
                metadata.file_type = FileType::Symlink;
            }
        }

        metadata
    }

    fn determine_recovery_method(&self, files: &[CleanableFile]) -> RecoveryMethod {
        // Check if all files can be restored from trash
        let all_trash_eligible = files.iter().all(|f| {
            let path = PathBuf::from(&f.path);
            !path.starts_with("/System") && !path.starts_with("/Library")
        });

        if all_trash_eligible {
            RecoveryMethod::TrashRestore
        } else {
            RecoveryMethod::Regenerate
        }
    }

    // Recovery restore API can be introduced as a Tauri command when UI flow is ready.

    // Placeholder helpers for future recovery modes are intentionally omitted to keep the crate warning-free.
}

// Data structures for validation and recovery

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_safe: bool,
    pub warnings: Vec<ValidationWarning>,
    pub errors: Vec<ValidationError>,
    pub file_states: HashMap<PathBuf, FileValidationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub file_path: PathBuf,
    pub warning_type: WarningType,
    pub message: String,
    pub dependencies: Option<Vec<PathBuf>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub file_path: PathBuf,
    pub error_type: ErrorType,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WarningType {
    HasDependencies,
    NoBackup,
    LargeFile,
    RecentlyModified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    FileInUse,
    CriticalFile,
    PermissionDenied,
    SystemProtected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileValidationState {
    Ready,
    RequiresConfirmation,
    Blocked(BlockReason),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockReason {
    InUse,
    SystemCritical,
    PermissionDenied,
    UserProtected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPoint {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub files: Vec<RecoveryFile>,
    pub recovery_method: RecoveryMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryFile {
    pub original_path: PathBuf,
    pub size: u64,
    pub category: String,
    pub metadata: FileMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub permissions: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub created: Option<DateTime<Utc>>,
    pub modified: Option<DateTime<Utc>>,
    pub file_type: FileType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileType {
    Regular,
    Directory,
    Symlink,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryMethod {
    TrashRestore,
    Regenerate,
    Redownload,
    Backup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct RestoreResult {
    pub restored_count: usize,
    pub failed_count: usize,
    pub restored_files: Vec<PathBuf>,
    pub failed_files: Vec<PathBuf>,
}
