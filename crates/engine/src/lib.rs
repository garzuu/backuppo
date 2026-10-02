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
use backuppo_core::model::{Artifact, RepositoryKey};
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

fn require_restic_engine(job_name: &str, config: &Config) -> Result<(), BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato")))?;
    if job.engine != EngineKind::Restic {
        return Err(BackupError::Other(
            "la rotazione delle chiavi e' disponibile solo per job Restic; per l'engine \
             'archive' vedi la guida alla sicurezza (nuova identita' age, nessuna \
             ricifratura degli archivi esistenti)"
                .into(),
        ));
    }
    Ok(())
}

/// Elenca le chiavi del repository Restic di un job (`restic key list`).
pub async fn list_restic_keys(
    job_name: &str,
    config: &Config,
) -> Result<Vec<RepositoryKey>, BackupError> {
    require_restic_engine(job_name, config)?;
    restic::key_list(job_name, config).await
}

/// Aggiunge una nuova password al repository Restic di un job, autenticando
/// con quella corrente. Non ricifra i dati: la vecchia password resta
/// valida finche' non viene rimossa esplicitamente con
/// [`remove_restic_key`].
pub async fn rotate_restic_key(
    job_name: &str,
    config: &Config,
    new_password: &str,
) -> Result<RepositoryKey, BackupError> {
    require_restic_engine(job_name, config)?;
    restic::key_add(job_name, config, new_password).await
}

/// Rimuove una chiave dal repository Restic di un job per id. Restic
/// rifiuta di rimuovere l'ultima chiave rimasta.
pub async fn remove_restic_key(
    job_name: &str,
    config: &Config,
    key_id: &str,
) -> Result<(), BackupError> {
    require_restic_engine(job_name, config)?;
    restic::key_remove(job_name, config, key_id).await
}
