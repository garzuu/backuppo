//! Crate `destinations`: implementazioni del trait `Destination` di `core`.

mod cloud_drive;
mod local;
mod opendal_backed;
mod s3;
mod sftp;
mod webdav;

pub use opendal_backed::OpendalDestination;
pub use sftp::SftpDestination;

use backuppo_core::config::DestinationConfig;
use backuppo_core::error::BackupError;
use backuppo_core::traits::Destination;

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
        DestinationConfig::GoogleDrive {
            root,
            access_token_env,
            refresh_token_env,
            client_id,
            client_secret_env,
            retry,
            bandwidth_limit_kib_s,
        } => Ok(Box::new(cloud_drive::google_drive(oauth_config(
            root.as_deref(),
            access_token_env,
            refresh_token_env.as_deref(),
            client_id.as_deref(),
            client_secret_env.as_deref(),
            retry,
            *bandwidth_limit_kib_s,
        ))?)),
        DestinationConfig::Dropbox {
            root,
            access_token_env,
            refresh_token_env,
            client_id,
            client_secret_env,
            retry,
            bandwidth_limit_kib_s,
        } => Ok(Box::new(cloud_drive::dropbox(oauth_config(
            root.as_deref(),
            access_token_env,
            refresh_token_env.as_deref(),
            client_id.as_deref(),
            client_secret_env.as_deref(),
            retry,
            *bandwidth_limit_kib_s,
        ))?)),
        DestinationConfig::OneDrive {
            root,
            access_token_env,
            refresh_token_env,
            client_id,
            client_secret_env,
            retry,
            bandwidth_limit_kib_s,
        } => Ok(Box::new(cloud_drive::onedrive(oauth_config(
            root.as_deref(),
            access_token_env,
            refresh_token_env.as_deref(),
            client_id.as_deref(),
            client_secret_env.as_deref(),
            retry,
            *bandwidth_limit_kib_s,
        ))?)),
        DestinationConfig::Restic { .. } => Err(BackupError::Other(
            "una destination restic può essere usata solo da un job con engine: restic".to_string(),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn oauth_config<'a>(
    root: Option<&'a str>,
    access_token_env: &'a str,
    refresh_token_env: Option<&'a str>,
    client_id: Option<&'a str>,
    client_secret_env: Option<&'a str>,
    retry: &'a backuppo_core::config::RetryConfig,
    bandwidth_limit_kib_s: Option<u64>,
) -> cloud_drive::OAuthConfig<'a> {
    cloud_drive::OAuthConfig {
        root,
        access_token_env,
        refresh_token_env,
        client_id,
        client_secret_env,
        retry,
        bandwidth_limit_kib_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::config::RetryConfig;

    fn common() -> (String, RetryConfig) {
        let variable = format!("BACKUPPO_CLOUD_TOKEN_TEST_{}", std::process::id());
        std::env::set_var(&variable, "test-access-token");
        (variable, RetryConfig::default())
    }

    #[test]
    fn builds_all_cloud_drive_backends() {
        let (access_token_env, retry) = common();
        let google = DestinationConfig::GoogleDrive {
            root: Some("/backups".into()),
            access_token_env: access_token_env.clone(),
            refresh_token_env: None,
            client_id: None,
            client_secret_env: None,
            retry: retry.clone(),
            bandwidth_limit_kib_s: None,
        };
        let dropbox = DestinationConfig::Dropbox {
            root: Some("/backups".into()),
            access_token_env: access_token_env.clone(),
            refresh_token_env: None,
            client_id: None,
            client_secret_env: None,
            retry: retry.clone(),
            bandwidth_limit_kib_s: None,
        };
        let onedrive = DestinationConfig::OneDrive {
            root: Some("/backups".into()),
            access_token_env,
            refresh_token_env: None,
            client_id: None,
            client_secret_env: None,
            retry,
            bandwidth_limit_kib_s: None,
        };

        assert!(build(&google).is_ok());
        assert!(build(&dropbox).is_ok());
        assert!(build(&onedrive).is_ok());
    }
}
