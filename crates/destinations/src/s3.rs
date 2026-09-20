use backupper_core::config::RetryConfig;
use backupper_core::error::BackupError;
use backupper_core::secrets::resolve_env;
use opendal::services::S3;
use opendal::Operator;

use crate::opendal_backed::{ensure_http_transport_installed, to_backup_error, OpendalDestination};

#[allow(clippy::too_many_arguments)]
pub fn build(
    bucket: &str,
    region: Option<&str>,
    endpoint: Option<&str>,
    access_key_id_env: &str,
    secret_access_key_env: &str,
    root: Option<&str>,
    virtual_host_style: bool,
    retry: &RetryConfig,
    bandwidth_limit_kib_s: Option<u64>,
) -> Result<OpendalDestination, BackupError> {
    ensure_http_transport_installed();

    let access_key_id = resolve_env("access_key_id_env", access_key_id_env)?;
    let secret_access_key = resolve_env("secret_access_key_env", secret_access_key_env)?;

    let mut builder = S3::default()
        .bucket(bucket)
        .access_key_id(&access_key_id)
        .secret_access_key(&secret_access_key)
        .root(root.unwrap_or("/"));

    if let Some(region) = region {
        builder = builder.region(region);
    }
    if let Some(endpoint) = endpoint {
        builder = builder.endpoint(endpoint);
    }
    if virtual_host_style {
        builder = builder.enable_virtual_host_style();
    }

    let op = Operator::new(builder).map_err(to_backup_error)?;
    Ok(OpendalDestination::new(op, retry, bandwidth_limit_kib_s))
}
