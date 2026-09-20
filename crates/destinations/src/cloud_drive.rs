use backuppo_core::config::RetryConfig;
use backuppo_core::error::BackupError;
use backuppo_core::secrets::resolve_env;
use opendal::services::{Dropbox, Gdrive, Onedrive};
use opendal::Operator;

use crate::opendal_backed::{ensure_http_transport_installed, to_backup_error, OpendalDestination};

pub(crate) struct OAuthConfig<'a> {
    pub root: Option<&'a str>,
    pub access_token_env: &'a str,
    pub refresh_token_env: Option<&'a str>,
    pub client_id: Option<&'a str>,
    pub client_secret_env: Option<&'a str>,
    pub retry: &'a RetryConfig,
    pub bandwidth_limit_kib_s: Option<u64>,
}

struct OAuthSecrets {
    access_token: String,
    refresh_token: Option<String>,
    client_secret: Option<String>,
}

fn resolve(config: &OAuthConfig<'_>) -> Result<OAuthSecrets, BackupError> {
    Ok(OAuthSecrets {
        access_token: resolve_env("access_token_env", config.access_token_env)?,
        refresh_token: config
            .refresh_token_env
            .map(|name| resolve_env("refresh_token_env", name))
            .transpose()?,
        client_secret: config
            .client_secret_env
            .map(|name| resolve_env("client_secret_env", name))
            .transpose()?,
    })
}

pub(crate) fn google_drive(config: OAuthConfig<'_>) -> Result<OpendalDestination, BackupError> {
    ensure_http_transport_installed();
    let secrets = resolve(&config)?;
    let mut builder = Gdrive::default()
        .root(config.root.unwrap_or("/"))
        .access_token(&secrets.access_token);
    if let Some(value) = secrets.refresh_token.as_deref() {
        builder = builder.refresh_token(value);
    }
    if let Some(value) = config.client_id {
        builder = builder.client_id(value);
    }
    if let Some(value) = secrets.client_secret.as_deref() {
        builder = builder.client_secret(value);
    }
    finish(builder, &config)
}

pub(crate) fn dropbox(config: OAuthConfig<'_>) -> Result<OpendalDestination, BackupError> {
    ensure_http_transport_installed();
    let secrets = resolve(&config)?;
    let mut builder = Dropbox::default()
        .root(config.root.unwrap_or("/"))
        .access_token(&secrets.access_token);
    if let Some(value) = secrets.refresh_token.as_deref() {
        builder = builder.refresh_token(value);
    }
    if let Some(value) = config.client_id {
        builder = builder.client_id(value);
    }
    if let Some(value) = secrets.client_secret.as_deref() {
        builder = builder.client_secret(value);
    }
    let op = Operator::new(builder).map_err(to_backup_error)?;
    Ok(OpendalDestination::new(
        op,
        config.retry,
        config.bandwidth_limit_kib_s,
    ))
}

pub(crate) fn onedrive(config: OAuthConfig<'_>) -> Result<OpendalDestination, BackupError> {
    ensure_http_transport_installed();
    let secrets = resolve(&config)?;
    let mut builder = Onedrive::default()
        .root(config.root.unwrap_or("/"))
        .access_token(&secrets.access_token);
    if let Some(value) = secrets.refresh_token.as_deref() {
        builder = builder.refresh_token(value);
    }
    if let Some(value) = config.client_id {
        builder = builder.client_id(value);
    }
    if let Some(value) = secrets.client_secret.as_deref() {
        builder = builder.client_secret(value);
    }
    let op = Operator::new(builder).map_err(to_backup_error)?;
    Ok(OpendalDestination::new(
        op,
        config.retry,
        config.bandwidth_limit_kib_s,
    ))
}

fn finish(builder: Gdrive, config: &OAuthConfig<'_>) -> Result<OpendalDestination, BackupError> {
    let op = Operator::new(builder).map_err(to_backup_error)?;
    Ok(OpendalDestination::new(
        op,
        config.retry,
        config.bandwidth_limit_kib_s,
    ))
}
