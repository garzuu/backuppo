//! Crate `sources`: implementazioni del trait `Source` di `core`.

mod folder;

pub use folder::FolderSource;

use backuppo_core::config::SourceConfig;
use backuppo_core::error::BackupError;
use backuppo_core::traits::Source;

/// Costruisce l'implementazione di `Source` corrispondente alla config.
pub fn build(config: &SourceConfig) -> Result<Box<dyn Source>, BackupError> {
    match config {
        SourceConfig::Folder { path, exclude } => Ok(Box::new(FolderSource::new(path, exclude)?)),
    }
}
