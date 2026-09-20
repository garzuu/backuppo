use std::path::Path;

use async_trait::async_trait;

use crate::error::BackupError;
use crate::model::{Artifact, JobEvent};

/// Produce un `Artifact` a partire da una sorgente di dati (cartella, dump di
/// un database, ecc.), scrivendo in una directory di staging.
#[async_trait]
pub trait Source: Send + Sync {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError>;
    async fn cleanup(&self, artifact: &Artifact) -> Result<(), BackupError>;
}

/// Trasferisce un `Artifact` verso/da uno storage (locale, sftp, s3, webdav).
#[async_trait]
pub trait Destination: Send + Sync {
    async fn upload(&self, artifact: &Artifact) -> Result<(), BackupError>;
    async fn download(&self, name: &str, dest: &Path) -> Result<Artifact, BackupError>;
    async fn list(&self) -> Result<Vec<String>, BackupError>;
    async fn delete(&self, name: &str) -> Result<(), BackupError>;
}

/// Invia un `JobEvent` a un canale esterno (SMTP, Telegram, webhook, ...).
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError>;
}
