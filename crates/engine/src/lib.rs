//! Crate `engine`: esecuzione job, retention, verifica restore.

pub mod archive;
pub mod history;
mod manifest;
mod naming;
mod notify;
mod observability;
pub mod recovery;
pub mod report;
mod restic;
pub mod retention;
mod runner;
pub mod status;
mod verify;

use backuppo_core::config::{Config, EngineKind};
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use tracing::warn;

pub use manifest::MANIFEST_FILENAME;
pub use recovery::{browse, restore, snapshots};
pub use runner::run_job;
pub use verify::verify_job;

/// Esegue il job e, solo se ha successo, applica la retention configurata.
/// Se il job fallisce, la retention viene saltata: non deve mai cancellare
/// backup precedenti quando il tentativo più recente non è andato a buon
/// fine. Un eventuale errore della retention stessa viene solo loggato, non
/// fa fallire la chiamata (il backup è comunque riuscito).
pub async fn run_and_retain(job_name: &str, config: &Config) -> Result<Artifact, BackupError> {
    let artifact = run_job(job_name, config).await?;

    if let Err(e) = retention::apply(job_name, config).await {
        warn!(job = job_name, error = %e, "applicazione della retention fallita");
    }

    Ok(artifact)
}

/// Esegue la manutenzione distruttiva Restic con l'identita amministrativa.
/// E' intenzionalmente separata dal daemon e dai normali backup.
pub async fn maintain(job_name: &str, config: &Config) -> Result<(), BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato")))?;
    if job.engine != EngineKind::Restic {
        return Err(BackupError::Other(
            "la manutenzione separata e' disponibile solo per job Restic".into(),
        ));
    }
    restic::maintain(job_name, config, &job.retention).await
}
