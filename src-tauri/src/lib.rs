#[cfg(feature = "app")]
mod config;
mod file_cleaner;
#[cfg(feature = "app")]
mod memory_optimizer;
#[cfg(feature = "app")]
mod metrics;
mod ops;
#[cfg(feature = "app")]
mod system_info;

pub use file_cleaner::{
    CleanableFile as StorageCleanableFile, CleaningReport as StorageCleaningReport,
    EnhancedCleaningReport, EnhancedDeletionProgress, EnhancedFileCleaner,
    FileCleaner as StorageFileCleaner, UserAction as StorageUserAction,
};

#[cfg(feature = "app")]
mod app;

#[cfg(feature = "app")]
pub use app::run;
