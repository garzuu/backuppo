//! Crate `destinations`: implementazioni del trait `Destination` di `core`.

mod local;

pub use local::LocalDestination;

use backupper_core::config::DestinationConfig;
use backupper_core::error::BackupError;
use backupper_core::traits::Destination;

/// Costruisce l'implementazione di `Destination` corrispondente alla config.
pub fn build(config: &DestinationConfig) -> Result<Box<dyn Destination>, BackupError> {
    match config {
        DestinationConfig::Fs { root } => Ok(Box::new(LocalDestination::new(root)?)),
        DestinationConfig::Sftp { .. } => Err(BackupError::Other(
            "destination 'sftp' non ancora implementata (prevista in Fase 6)".to_string(),
        )),
        DestinationConfig::S3 { .. } => Err(BackupError::Other(
            "destination 's3' non ancora implementata (prevista in Fase 6)".to_string(),
        )),
        DestinationConfig::Webdav { .. } => Err(BackupError::Other(
            "destination 'webdav' non ancora implementata (prevista in Fase 6)".to_string(),
        )),
    }
}
