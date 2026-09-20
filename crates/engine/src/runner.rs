use std::time::{SystemTime, UNIX_EPOCH};

use backuppo_core::config::{Compression, Config, EncryptionConfig, VerifyRestore};
use backuppo_core::error::BackupError;
use backuppo_core::model::{Artifact, JobEvent};
use backuppo_core::secrets::resolve_env;
use tokio::process::Command;
use tracing::{info, instrument, warn};

use crate::{archive, manifest, notify, verify};

/// Esegue il job `job_name` e invia le notifiche configurate: `on_success`
/// se il backup (ed eventuale verifica) va a buon fine, `on_failure`
/// altrimenti.
#[instrument(skip(job_name, config), fields(job = job_name))]
pub async fn run_job(job_name: &str, config: &Config) -> Result<Artifact, BackupError> {
    let result = run_job_impl(job_name, config).await;

    if let Some(job) = config.jobs.get(job_name) {
        let (event, names) = match &result {
            Ok(artifact) => (
                JobEvent::Success {
                    job: job_name.to_string(),
                    artifact: artifact.clone(),
                },
                &job.notify.on_success,
            ),
            Err(e) => (
                JobEvent::Failure {
                    job: job_name.to_string(),
                    error: e.to_string(),
                },
                &job.notify.on_failure,
            ),
        };
        notify::dispatch(config, names, &event).await;
    }

    result
}

/// Prepara la sorgente, costruisce l'archivio (tar [+ zstd] [+ age]) e lo
/// carica sulla destination configurata. Ritorna l'`Artifact` finale caricato.
async fn run_job_impl(job_name: &str, config: &Config) -> Result<Artifact, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;

    let dest_config = config.destinations.get(&job.destination).ok_or_else(|| {
        BackupError::Other(format!(
            "destination '{}' non trovata in config (job '{job_name}')",
            job.destination
        ))
    })?;

    let source = backuppo_sources::build(&job.source)?;
    let destination = backuppo_destinations::build(dest_config)?;

    run_hooks("pre", &job.pre).await?;

    let staging_root = tempfile::tempdir()?;
    let staging_data = staging_root.path().join("data");

    info!("avvio preparazione sorgente");
    let raw = source.prepare(&staging_data).await?;
    info!(files = raw.files, bytes = raw.bytes, "sorgente preparata");

    let manifest_dir = raw.path.clone();
    tokio::task::spawn_blocking(move || {
        let manifest = manifest::build(&manifest_dir)?;
        manifest::write(&manifest_dir, &manifest)
    })
    .await
    .map_err(|e| BackupError::Other(format!("task di manifest interrotto: {e}")))??;

    let compress = !matches!(job.compression, Some(Compression::None));
    let passphrase = resolve_passphrase(job_name, job.encryption.as_ref())?;

    let mut filename = format!("{job_name}-{}.tar", unix_timestamp());
    if compress {
        filename.push_str(".zst");
    }
    if passphrase.is_some() {
        filename.push_str(".age");
    }

    let archive_path = staging_root.path().join(&filename);
    let raw_path = raw.path.clone();
    let archive_path_for_task = archive_path.clone();
    let passphrase_for_task = passphrase.clone();
    tokio::task::spawn_blocking(move || {
        archive::build_archive(
            &raw_path,
            &archive_path_for_task,
            compress,
            passphrase_for_task.as_deref(),
        )
    })
    .await
    .map_err(|e| BackupError::Other(format!("task di archiviazione interrotto: {e}")))??;

    let checksum_path = archive_path.clone();
    let checksum = tokio::task::spawn_blocking(move || archive::sha256_file(&checksum_path))
        .await
        .map_err(|e| BackupError::Other(format!("task di checksum interrotto: {e}")))??;
    let bytes = tokio::fs::metadata(&archive_path).await?.len();

    let final_artifact = Artifact {
        path: archive_path,
        bytes,
        files: raw.files,
        checksum,
    };

    info!(
        bytes = final_artifact.bytes,
        checksum = %final_artifact.checksum,
        "archivio pronto, avvio upload"
    );
    destination.upload(&final_artifact).await?;
    source.cleanup(&raw).await?;
    info!("upload completato");

    // "Un backup si considera riuscito solo dopo che l'upload è confermato
    // e (se attivo) il restore è verificato": con `verify_restore: every` la
    // verifica fa parte del criterio di successo del job.
    if job.verify_restore == Some(VerifyRestore::Every) {
        info!("verify_restore=every: avvio verifica restore");
        let verify_result = verify::verify_job_impl(job_name, config).await;
        if let Ok(event) = &verify_result {
            notify::dispatch(config, &job.notify.on_verify, event).await;
        }
        verify_result?;
    }

    if let Err(error) = run_hooks("post", &job.post).await {
        warn!(%error, "hook post fallito dopo il completamento del backup");
    }

    info!("job completato con successo");
    Ok(final_artifact)
}

async fn run_hooks(kind: &str, hooks: &[String]) -> Result<(), BackupError> {
    for (index, hook) in hooks.iter().enumerate() {
        info!(hook_kind = kind, hook_number = index + 1, "esecuzione hook");
        #[cfg(windows)]
        let output = Command::new("cmd").args(["/C", hook]).output().await;
        #[cfg(not(windows))]
        let output = Command::new("sh").args(["-c", hook]).output().await;

        let output = output.map_err(|e| {
            BackupError::Other(format!(
                "hook {kind} #{}: impossibile avviare la shell: {e}",
                index + 1
            ))
        })?;
        if !output.status.success() {
            return Err(BackupError::Other(format!(
                "hook {kind} #{} fallito con {}: {}",
                index + 1,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
    }
    Ok(())
}

/// Risolve la passphrase di cifratura dalla config del job, se presente.
/// MVP: supporta solo `age` con `passphrase_env` (le chiavi asimmetriche
/// sono fuori scope per questa fase).
pub(crate) fn resolve_passphrase(
    job_name: &str,
    encryption: Option<&EncryptionConfig>,
) -> Result<Option<String>, BackupError> {
    match encryption {
        None | Some(EncryptionConfig::None) => Ok(None),
        Some(EncryptionConfig::Age { passphrase_env: Some(var), .. }) => {
            Ok(Some(resolve_env("encryption.passphrase_env", var)?))
        }
        Some(EncryptionConfig::Age { key_env: Some(_), .. }) => Err(BackupError::Other(format!(
            "job '{job_name}': cifratura age con 'key_env' non ancora supportata (solo 'passphrase_env' in questa fase)"
        ))),
        Some(EncryptionConfig::Age { .. }) => Err(BackupError::Other(format!(
            "job '{job_name}': encryption 'age' richiede 'passphrase_env'"
        ))),
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
