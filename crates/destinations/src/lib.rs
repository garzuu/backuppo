//! Crate `destinations`: implementazioni del trait `Destination` di `core`.

mod local;
mod opendal_backed;
mod s3;
mod sftp;
mod webdav;

pub use opendal_backed::OpendalDestination;
pub use sftp::SftpDestination;

use backupper_core::config::DestinationConfig;
use backupper_core::error::BackupError;
use backupper_core::traits::Destination;

/// Costruisce l'implementazione di `Destination` corrispondente alla config.
pub fn build(config: &DestinationConfig) -> Result<Box<dyn Destination>, BackupError> {
    match config {
        DestinationConfig::Fs { root } => Ok(Box::new(local::build(root)?)),
        DestinationConfig::Sftp {
            host,
            port,
            user,
            password_env,
            key_path,
            key_passphrase_env,
            root,
            host_key_fingerprint,
            retry,
            bandwidth_limit_kib_s: _,
        } => Ok(Box::new(SftpDestination::new(
            host,
            *port,
            user,
            password_env.as_deref(),
            key_path.as_deref(),
            key_passphrase_env.as_deref(),
            root,
            host_key_fingerprint.as_deref(),
            retry.clone(),
        )?)),
        DestinationConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key_id_env,
            secret_access_key_env,
            root,
            virtual_host_style,
            retry,
            bandwidth_limit_kib_s,
        } => Ok(Box::new(s3::build(
            bucket,
            region.as_deref(),
            endpoint.as_deref(),
            access_key_id_env,
            secret_access_key_env,
            root.as_deref(),
            *virtual_host_style,
            retry,
            *bandwidth_limit_kib_s,
        )?)),
        DestinationConfig::Webdav {
            url,
            user,
            password_env,
            retry,
            bandwidth_limit_kib_s,
        } => Ok(Box::new(webdav::build(
            url,
            user.as_deref(),
            password_env.as_deref(),
            retry,
            *bandwidth_limit_kib_s,
        )?)),
    }
}
