use backuppo_core::config::RetryConfig;
use backuppo_core::error::BackupError;
use backuppo_core::secrets::resolve_env;
use opendal::services::Webdav;
use opendal::Operator;

use crate::opendal_backed::{ensure_http_transport_installed, to_backup_error, OpendalDestination};

pub fn build(
    url: &str,
    user: Option<&str>,
    password_env: Option<&str>,
    retry: &RetryConfig,
    bandwidth_limit_kib_s: Option<u64>,
) -> Result<OpendalDestination, BackupError> {
    ensure_http_transport_installed();

    let mut builder = Webdav::default().endpoint(url);

    if let Some(user) = user {
        builder = builder.username(user);
    }
    if let Some(password_env) = password_env {
        let password = resolve_env("password_env", password_env)?;
        builder = builder.password(&password);
    }

    let op = Operator::new(builder).map_err(to_backup_error)?;
    Ok(OpendalDestination::new(op, retry, bandwidth_limit_kib_s))
}
