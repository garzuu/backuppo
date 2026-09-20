use std::collections::HashMap;
use std::path::{Path, PathBuf};

use backuppo_core::config::{Config, DestinationConfig, Retention};
use backuppo_core::error::BackupError;
use backuppo_core::model::{Artifact, JobEvent};
use backuppo_core::secrets::resolve_env;
use serde_json::Value;
use tokio::process::Command;

use crate::manifest;

struct Settings<'a> {
    repository: &'a str,
    password_env: &'a str,
    environment: &'a HashMap<String, String>,
    binary: &'a str,
    initialize: bool,
}

fn settings<'a>(job_name: &str, config: &'a Config) -> Result<Settings<'a>, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;
    match config.destinations.get(&job.destination) {
        Some(DestinationConfig::Restic {
            repository,
            password_env,
            environment,
            binary,
            initialize,
        }) => Ok(Settings {
            repository,
            password_env,
            environment,
            binary,
            initialize: *initialize,
        }),
        _ => Err(BackupError::Other(format!(
            "job '{job_name}': destination Restic non valida"
        ))),
    }
}

fn command(settings: &Settings<'_>) -> Result<Command, BackupError> {
    let mut command = Command::new(settings.binary);
    command.env("RESTIC_REPOSITORY", settings.repository).env(
        "RESTIC_PASSWORD",
        resolve_env("destination.password_env", settings.password_env)?,
    );
    for (target, source) in settings.environment {
        command.env(target, resolve_env("destination.environment", source)?);
    }
    Ok(command)
}

async fn output(
    settings: &Settings<'_>,
    arguments: &[&str],
    current_dir: Option<&Path>,
) -> Result<Vec<u8>, BackupError> {
    let mut command = command(settings)?;
    command.args(arguments);
    if let Some(path) = current_dir {
        command.current_dir(path);
    }
    let output = command.output().await.map_err(|error| {
        BackupError::Other(format!(
            "impossibile avviare Restic ('{}'): {error}",
            settings.binary
        ))
    })?;
    if !output.status.success() {
        return Err(BackupError::Other(format!(
            "restic {} fallito: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

async fn ensure_repository(settings: &Settings<'_>) -> Result<(), BackupError> {
    if output(settings, &["snapshots", "--json"], None)
        .await
        .is_ok()
    {
        return Ok(());
    }
    if !settings.initialize {
        return Err(BackupError::Other(
            "repository Restic non disponibile e initialize è false".to_string(),
        ));
    }
    output(settings, &["init"], None).await.map(|_| ())
}

pub(crate) async fn backup(
    job_name: &str,
    config: &Config,
    staging: &Path,
    files: u64,
    bytes: u64,
) -> Result<Artifact, BackupError> {
    let settings = settings(job_name, config)?;
    ensure_repository(&settings).await?;
    let tag = format!("backuppo-{job_name}");
    output(
        &settings,
        &["backup", ".", "--json", "--tag", &tag],
        Some(staging),
    )
    .await?;
    let snapshots = output(
        &settings,
        &["snapshots", "--json", "--latest", "1", "--tag", &tag],
        None,
    )
    .await?;
    let values: Vec<Value> = serde_json::from_slice(&snapshots).map_err(|error| {
        BackupError::Other(format!("risposta JSON di Restic non valida: {error}"))
    })?;
    let snapshot = values
        .first()
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| BackupError::Other("Restic non ha restituito lo snapshot creato".into()))?;
    Ok(Artifact {
        path: PathBuf::from(format!("restic-{snapshot}")),
        bytes,
        files,
        checksum: snapshot.to_string(),
    })
}

pub(crate) async fn verify(job_name: &str, config: &Config) -> Result<JobEvent, BackupError> {
    let settings = settings(job_name, config)?;
    let tag = format!("backuppo-{job_name}");
    let restore = tempfile::tempdir()?;
    let target = restore.path().to_string_lossy().into_owned();
    output(
        &settings,
        &["restore", "latest", "--tag", &tag, "--target", &target],
        None,
    )
    .await?;
    let restored_root = find_manifest_root(restore.path())?;
    let restored_for_task = restored_root.clone();
    let (files, bytes, sample) =
        tokio::task::spawn_blocking(move || crate::verify::check_restored_tree(&restored_for_task))
            .await
            .map_err(|error| {
                BackupError::Other(format!("task di verifica interrotto: {error}"))
            })??;
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato")))?;
    backuppo_sources::build(&job.source)?
        .verify_restore(&restored_root)
        .await?;
    Ok(JobEvent::RestoreVerified {
        job: job_name.to_string(),
        detail: format!(
            "snapshot Restic: {files} file ({bytes} byte), checksum verificato su {sample} file"
        ),
    })
}

fn find_manifest_root(root: &Path) -> Result<PathBuf, BackupError> {
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|error| BackupError::Other(error.to_string()))?;
        if entry.file_name() == manifest::MANIFEST_FILENAME {
            return entry
                .path()
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| BackupError::Other("manifest Restic senza directory padre".into()));
        }
    }
    Err(BackupError::Other(
        "manifest Backuppo non trovato nello snapshot Restic".into(),
    ))
}

pub(crate) async fn forget(
    job_name: &str,
    config: &Config,
    retention: &Retention,
) -> Result<(), BackupError> {
    let settings = settings(job_name, config)?;
    let tag = format!("backuppo-{job_name}");
    let mut owned = vec![
        "forget".to_string(),
        "--prune".to_string(),
        "--tag".to_string(),
        tag,
    ];
    if let Some(value) = retention.daily {
        owned.extend(["--keep-daily".into(), value.to_string()]);
    }
    if let Some(value) = retention.weekly {
        owned.extend(["--keep-weekly".into(), value.to_string()]);
    }
    if let Some(value) = retention.monthly {
        owned.extend(["--keep-monthly".into(), value.to_string()]);
    }
    let arguments: Vec<&str> = owned.iter().map(String::as_str).collect();
    output(&settings, &arguments, None).await.map(|_| ())
}
