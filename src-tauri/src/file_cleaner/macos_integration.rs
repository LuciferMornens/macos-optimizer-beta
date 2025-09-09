use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
use tokio::process::Command as TokioCommand;

/// macOS-specific system integration
pub struct MacOSIntegration {
    sip_checker: SIPChecker,
    spotlight_integration: SpotlightIntegration,
    launch_services: LaunchServicesChecker,
    time_machine: TimeMachineIntegration,
    icloud_checker: ICloudChecker,
}

impl MacOSIntegration {
    pub fn new() -> Self {
        Self {
            sip_checker: SIPChecker::new(),
            spotlight_integration: SpotlightIntegration::new(),
            launch_services: LaunchServicesChecker::new(),
            time_machine: TimeMachineIntegration::new(),
            icloud_checker: ICloudChecker::new(),
        }
    }

    pub fn check_sip_protection(&self, path: &Path) -> bool {
        self.sip_checker.is_protected(path)
    }

    pub async fn check_spotlight_importance(&self, path: &Path) -> SpotlightInfo {
        self.spotlight_integration.get_file_info(path).await
    }

    pub async fn check_launch_services(&self, path: &Path) -> bool {
        self.launch_services.is_registered(path).await
    }

    pub async fn check_time_machine_status(&self, path: &Path) -> BackupStatus {
        self.time_machine.get_backup_status(path).await
    }

    pub async fn check_icloud_status(&self, path: &Path) -> CloudStatus {
        self.icloud_checker.get_sync_status(path).await
    }

    pub async fn get_file_associations(&self, path: &Path) -> Vec<FileAssociation> {
        let mut associations = Vec::new();
        
        // Check Launch Services
        if self.check_launch_services(path).await {
            associations.push(FileAssociation::LaunchServices);
        }
        
        // Check Spotlight
        let spotlight_info = self.check_spotlight_importance(path).await;
        if spotlight_info.is_indexed {
            associations.push(FileAssociation::Spotlight);
        }
        
        // Check for XPC services
        if self.is_xpc_service(path).await {
            associations.push(FileAssociation::XPCService);
        }
        
        associations
    }

    async fn is_xpc_service(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.contains(".xpc") || path_str.contains("XPCServices")
    }
}

/// System Integrity Protection checker
pub struct SIPChecker {
    protected_paths: Vec<PathBuf>,
}

impl SIPChecker {
    pub fn new() -> Self {
        Self {
            protected_paths: vec![
                PathBuf::from("/System"),
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
                PathBuf::from("/var"),
            ],
        }
    }

    pub fn is_protected(&self, path: &Path) -> bool {
        // Check if path is under SIP protection
        for protected in &self.protected_paths {
            if path.starts_with(protected) {
                // Some paths under /usr/local are not protected
                if path.starts_with("/usr/local") {
                    return false;
                }
                return true;
            }
        }
        
        // Check using csrutil if available
        if let Ok(output) = Command::new("csrutil")
            .arg("status")
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("enabled") {
                // SIP is enabled, check if path is protected
                return self.is_sip_protected_path(path);
            }
        }
        
        false
    }

    fn is_sip_protected_path(&self, path: &Path) -> bool {
        // Additional SIP-specific checks
        let path_str = path.to_string_lossy();
        
        // Check for system frameworks and libraries
        if path_str.contains("/System/Library/Frameworks/")
            || path_str.contains("/System/Library/PrivateFrameworks/")
            || path_str.contains("/System/Library/CoreServices/")
        {
            return true;
        }
        
        false
    }
}

/// Spotlight metadata integration
pub struct SpotlightIntegration;

impl SpotlightIntegration {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_file_info(&self, path: &Path) -> SpotlightInfo {
        let mut info = SpotlightInfo {
            is_indexed: false,
            content_type: None,
            last_used: None,
            use_count: 0,
            tags: Vec::new(),
        };
        
        // Use mdls to get Spotlight metadata
        if let Ok(output) = TokioCommand::new("mdls")
            .arg("-plist")
            .arg("-")
            .arg(path)
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Parse metadata (simplified - in production would use plist parser)
            if stdout.contains("kMDItemContentType") {
                info.is_indexed = true;
                
                // Extract content type
                if let Some(content_type) = self.extract_value(&stdout, "kMDItemContentType") {
                    info.content_type = Some(content_type);
                }
                
                // Extract use count
                if let Some(use_count_str) = self.extract_value(&stdout, "kMDItemUseCount") {
                    if let Ok(count) = use_count_str.parse::<u32>() {
                        info.use_count = count;
                    }
                }
                
                // Extract tags
                if let Some(tags_str) = self.extract_value(&stdout, "kMDItemUserTags") {
                    info.tags = tags_str.split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
            }
        }
        
        info
    }

    fn extract_value(&self, plist: &str, key: &str) -> Option<String> {
        // Simplified extraction - in production would use proper plist parsing
        if let Some(key_pos) = plist.find(key) {
            let after_key = &plist[key_pos + key.len()..];
            if let Some(value_start) = after_key.find("<string>") {
                let value_section = &after_key[value_start + 8..];
                if let Some(value_end) = value_section.find("</string>") {
                    return Some(value_section[..value_end].to_string());
                }
            }
        }
        None
    }

    // Additional Spotlight search API can be added behind a feature flag when needed.
}

/// Launch Services database checker
pub struct LaunchServicesChecker;

impl LaunchServicesChecker {
    pub fn new() -> Self {
        Self
    }

    pub async fn is_registered(&self, path: &Path) -> bool {
        // Check if file is registered with Launch Services
        if path.extension().map_or(false, |ext| ext == "app") {
            // Check if app is registered
            if let Ok(output) = TokioCommand::new("lsregister")
                .arg("-dump")
                .output()
                .await
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let path_str = path.to_string_lossy();
                return stdout.contains(&*path_str);
            }
        }
        
        // Check for launch agents/daemons
        let path_str = path.to_string_lossy();
        if path_str.contains("LaunchAgents") || path_str.contains("LaunchDaemons") {
            return true;
        }
        
        false
    }

    // Future App bundle APIs can be introduced with explicit features if required.
}

/// Time Machine integration
pub struct TimeMachineIntegration;

impl TimeMachineIntegration {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_backup_status(&self, path: &Path) -> BackupStatus {
        // Check if Time Machine is enabled
        if !self.is_enabled().await {
            return BackupStatus {
                is_backed_up: false,
                is_excluded: false,
                last_backup: None,
            };
        }
        
        // Check if path is excluded
        let is_excluded = self.is_excluded(path).await;
        
        BackupStatus {
            is_backed_up: !is_excluded,
            is_excluded,
            last_backup: self.get_last_backup_time().await,
        }
    }

    async fn is_enabled(&self) -> bool {
        if let Ok(output) = TokioCommand::new("tmutil")
            .arg("status")
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout.contains("BackupPhase");
        }
        false
    }

    async fn is_excluded(&self, path: &Path) -> bool {
        if let Ok(output) = TokioCommand::new("tmutil")
            .arg("isexcluded")
            .arg(path)
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout.contains("[Excluded]");
        }
        false
    }

    async fn get_last_backup_time(&self) -> Option<String> {
        if let Ok(output) = TokioCommand::new("tmutil")
            .arg("latestbackup")
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                return Some(stdout);
            }
        }
        None
    }

    // Additional Time Machine APIs (add/remove exclusions) can be added behind a feature flag.
}

/// iCloud sync status checker
pub struct ICloudChecker;

impl ICloudChecker {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_sync_status(&self, path: &Path) -> CloudStatus {
        let mut status = CloudStatus {
            is_synced: false,
            is_downloading: false,
            is_uploading: false,
            sync_error: None,
        };
        
        // Check if path is in iCloud Drive
        let path_str = path.to_string_lossy();
        if !path_str.contains("Library/Mobile Documents") && !path_str.contains("iCloud Drive") {
            return status;
        }
        
        // Use brctl to check iCloud status
        if let Ok(output) = TokioCommand::new("brctl")
            .arg("status")
            .arg(path)
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            status.is_synced = stdout.contains("synced");
            status.is_downloading = stdout.contains("downloading");
            status.is_uploading = stdout.contains("uploading");
            
            if stdout.contains("error") {
                status.sync_error = Some("Sync error detected".to_string());
            }
        }
        
        status
    }

    // Future iCloud storage management APIs can be added behind a feature flag.
}

// Data structures

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotlightInfo {
    pub is_indexed: bool,
    pub content_type: Option<String>,
    pub last_used: Option<String>,
    pub use_count: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatus {
    pub is_backed_up: bool,
    pub is_excluded: bool,
    pub last_backup: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudStatus {
    pub is_synced: bool,
    pub is_downloading: bool,
    pub is_uploading: bool,
    pub sync_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileAssociation {
    LaunchServices,
    Spotlight,
    XPCService,
    LaunchAgent,
    LaunchDaemon,
}

// AppBundle and XPC-specific types removed until needed; avoids unused code warnings.
