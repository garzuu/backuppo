//! Crate `engine`: esecuzione job, retention, verifica restore.

pub mod archive;
mod manifest;
mod naming;
mod notify;
pub mod retention;
mod runner;
mod verify;

use backuppo_core::config::Config;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use tracing::warn;

pub use manifest::MANIFEST_FILENAME;
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
