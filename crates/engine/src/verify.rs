use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use backuppo_core::config::{Config, EngineKind};
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;
use backuppo_core::traits::Destination;
use tracing::{info, instrument, warn};

use crate::manifest::{self, Manifest, ManifestEntry};
use crate::naming::extract_timestamp;
use crate::observability::ExecutionObserver;
use crate::{archive, notify, restic, runner};

/// Numero massimo di file di cui viene ricalcolato lo sha256 durante la
/// verifica (per archivi con molti file, evita di rileggerli tutti).
const SAMPLE_SIZE: usize = 5;

/// Verifica l'ultimo backup del job e invia le notifiche configurate:
/// `on_verify` se la verifica va a buon fine, `on_failure` altrimenti.
pub async fn verify_job(job_name: &str, config: &Config) -> Result<JobEvent, BackupError> {
    let mut observer = ExecutionObserver::start(config, job_name, "verify");
    observer.verification_started();
    let result = verify_job_impl(job_name, config).await;
    observer.finish_verify(config, &result);

    if let Some(job) = config.jobs.get(job_name) {
        match &result {
            Ok(event) => notify::dispatch(config, &job.notify.on_verify, event).await,
            Err(e) => {
                let event = JobEvent::Failure {
                    job: job_name.to_string(),
                    error: e.to_string(),
                };
                notify::dispatch(config, &job.notify.on_failure, &event).await;
            }
        }
    }

    result
}

/// Scarica l'ultimo backup del job, lo decifra/decomprime in una cartella
/// temporanea e ne verifica l'integrità (numero di file, dimensioni,
/// checksum su un campione) confrontando col manifest incluso nell'archivio.
#[instrument(skip(job_name, config), fields(job = job_name))]
pub(crate) async fn verify_job_impl(
    job_name: &str,
    config: &Config,
) -> Result<JobEvent, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;

    if job.engine == EngineKind::Restic {
        return restic::verify(job_name, config).await;
    }

    let dest_config = config.destinations.get(&job.destination).ok_or_else(|| {
        BackupError::Other(format!(
            "destination '{}' non trovata in config (job '{job_name}')",
            job.destination
        ))
    })?;
    let destination = backuppo_destinations::build(dest_config)?;
    let source = backuppo_sources::build(&job.source)?;

    let name = find_latest(destination.as_ref(), job_name).await?;
    check_age(job_name, &name, job.max_backup_age_hours);

    let encrypted = name.ends_with(".age");
    let base = name.strip_suffix(".age").unwrap_or(&name);
    let compressed = base.ends_with(".zst");

    let passphrase = if encrypted {
        let pass = runner::resolve_encryption(job_name, job.encryption.as_ref())?
            .ok_or_else(|| {
                BackupError::Other(format!(
                    "job '{job_name}': l'ultimo backup ('{name}') è cifrato ma manca la passphrase o chiave age in config"
                ))
            })?;
        Some(pass)
    } else {
        None
    };

    let work_dir = tempfile::tempdir()?;
    let archive_path = work_dir.path().join(&name);
    let extract_dir = work_dir.path().join("restore");
    tokio::fs::create_dir_all(&extract_dir).await?;

    info!(backup = %name, "scaricamento ultimo backup per verifica");
    destination.download(&name, &archive_path).await?;

    let extract_dir_task = extract_dir.clone();
    let archive_path_task = archive_path.clone();
    let passphrase_task = passphrase.clone();
    tokio::task::spawn_blocking(move || {
        archive::extract_archive(
            &archive_path_task,
            &extract_dir_task,
            compressed,
            passphrase_task.as_ref(),
        )
    })
    .await
    .map_err(|e| BackupError::Other(format!("task di estrazione interrotto: {e}")))?
    .map_err(|e| {
        BackupError::Other(format!(
            "job '{job_name}': impossibile ripristinare '{name}': {e}"
        ))
    })?;

    let extract_dir_task = extract_dir.clone();
    let (files, bytes, sample_checked) =
        tokio::task::spawn_blocking(move || check_restored_tree(&extract_dir_task))
            .await
            .map_err(|e| BackupError::Other(format!("task di verifica interrotto: {e}")))?
            .map_err(|e| {
                BackupError::Other(format!(
                    "job '{job_name}': verifica del restore fallita su '{name}': {e}"
                ))
            })?;

    source.verify_restore(&extract_dir).await.map_err(|e| {
        BackupError::Other(format!(
            "job '{job_name}': verifica specifica della sorgente fallita su '{name}': {e}"
        ))
    })?;

    info!(
        files,
        bytes, sample_checked, "restore verificato con successo"
    );
    Ok(JobEvent::RestoreVerified {
        job: job_name.to_string(),
        detail: format!(
            "archivio '{name}': {files} file ({bytes} byte), checksum verificato su {sample_checked} file di campione"
        ),
    })
}

/// Trova il nome dell'ultimo backup caricato per `job_name`: i nomi
/// incorporano un timestamp unix a larghezza fissa, quindi l'ordinamento
/// lessicografico corrisponde all'ordine cronologico.
async fn find_latest(destination: &dyn Destination, job_name: &str) -> Result<String, BackupError> {
    let prefix = format!("{job_name}-");
    let mut names: Vec<String> = destination
        .list()
        .await?
        .into_iter()
        .filter(|n| n.starts_with(&prefix))
        .collect();
    names.sort();
    names
        .pop()
        .ok_or_else(|| BackupError::Other(format!("nessun backup trovato per il job '{job_name}'")))
}

/// Confronta l'albero ripristinato in `extract_dir` col manifest incluso
/// nell'archivio: numero di file, dimensioni di ognuno, checksum su un
/// campione. Ritorna (file, byte totali, file di campione controllati).
pub(crate) fn check_restored_tree(extract_dir: &Path) -> Result<(u64, u64, usize), BackupError> {
    let manifest: Manifest = manifest::read(extract_dir)?;

    let mut total_bytes = 0u64;
    for entry in &manifest.files {
        total_bytes += entry.size;
        let actual_size = std::fs::metadata(extract_dir.join(&entry.path))
            .map_err(|e| {
                BackupError::Other(format!("file mancante nel restore '{}': {e}", entry.path))
            })?
            .len();
        if actual_size != entry.size {
            return Err(BackupError::Other(format!(
                "dimensione diversa per '{}': attesi {} byte, trovati {actual_size}",
                entry.path, entry.size
            )));
        }
    }

    let sample = pick_sample(&manifest.files);
    for entry in &sample {
        let actual = archive::sha256_file(&extract_dir.join(&entry.path))?;
        if actual != entry.sha256 {
            return Err(BackupError::Other(format!(
                "checksum non corrispondente per '{}': il backup risulta corrotto",
                entry.path
            )));
        }
    }

    Ok((manifest.files.len() as u64, total_bytes, sample.len()))
}

fn pick_sample(files: &[ManifestEntry]) -> Vec<&ManifestEntry> {
    if files.len() <= SAMPLE_SIZE {
        return files.iter().collect();
    }
    let step = (files.len() / SAMPLE_SIZE).max(1);
    files.iter().step_by(step).take(SAMPLE_SIZE).collect()
}

/// Se `max_hours` è configurato e il backup più recente lo supera, logga un
/// allarme (non bloccante: la verifica prosegue comunque).
fn check_age(job_name: &str, name: &str, max_hours: Option<u64>) {
    let Some(max_hours) = max_hours else {
        return;
    };
    let Some(created_at) = extract_timestamp(name) else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let age_hours = now.saturating_sub(created_at) / 3600;
    if age_hours > max_hours {
        warn!(
            job = job_name,
            backup = name,
            age_hours,
            max_hours,
            "il backup più recente supera la soglia di età configurata"
        );
    }
}
