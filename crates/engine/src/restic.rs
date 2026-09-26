use std::collections::HashMap;
use std::path::{Path, PathBuf};

use backuppo_core::config::{Config, DestinationConfig, Retention};
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;
use backuppo_core::model::{
    Artifact, BackupEntry, BackupRef, OverwritePolicy, RestoreRequest, RestoreResult,
};
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
    append_only: bool,
    maintenance_environment: &'a HashMap<String, String>,
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
            append_only,
            maintenance_environment,
            object_lock: _,
        }) => Ok(Settings {
            repository,
            password_env,
            environment,
            binary,
            initialize: *initialize,
            append_only: *append_only,
            maintenance_environment,
        }),
        _ => Err(BackupError::Other(format!(
            "job '{job_name}': destination Restic non valida"
        ))),
    }
}

fn command_with_environment(
    settings: &Settings<'_>,
    environment: &HashMap<String, String>,
) -> Result<Command, BackupError> {
    let binary = bundled_restic(settings.binary);
    let mut command = Command::new(&binary);
    command.env("RESTIC_REPOSITORY", settings.repository).env(
        "RESTIC_PASSWORD",
        resolve_env("destination.password_env", settings.password_env)?,
    );
    // Evita che credenziali gia presenti nell'ambiente del servizio aggirino
    // la separazione fra identita operativa e amministrativa.
    for target in settings
        .environment
        .keys()
        .chain(settings.maintenance_environment.keys())
    {
        command.env_remove(target);
    }
    for (target, source) in environment {
        command.env(target, resolve_env("destination.environment", source)?);
    }
    Ok(command)
}

fn command(settings: &Settings<'_>) -> Result<Command, BackupError> {
    command_with_environment(settings, settings.environment)
}

fn bundled_restic(configured: &str) -> PathBuf {
    if configured != "restic" {
        return PathBuf::from(configured);
    }
    let name = if cfg!(windows) {
        "restic.exe"
    } else {
        "restic"
    };
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(|parent| parent.join(name)))
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(configured))
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
    if settings.append_only {
        tracing::info!(
            job = job_name,
            "retention distruttiva saltata: repository append-only"
        );
        return Ok(());
    }
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

/// Applica retention e prune usando esclusivamente le credenziali
/// amministrative. Questo percorso non viene mai invocato dal daemon.
pub(crate) async fn maintain(
    job_name: &str,
    config: &Config,
    retention: &Retention,
) -> Result<(), BackupError> {
    let settings = settings(job_name, config)?;
    if settings.maintenance_environment.is_empty() {
        return Err(BackupError::Other(format!(
            "job '{job_name}': maintenance_environment non configurato"
        )));
    }
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
    let mut command = command_with_environment(&settings, settings.maintenance_environment)?;
    let output = command.args(&arguments).output().await.map_err(|error| {
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
    Ok(())
}

pub(crate) async fn snapshots(
    job_name: &str,
    config: &Config,
) -> Result<Vec<BackupRef>, BackupError> {
    let settings = settings(job_name, config)?;
    let tag = format!("backuppo-{job_name}");
    let bytes = output(&settings, &["snapshots", "--json", "--tag", &tag], None).await?;
    let values: Vec<Value> = serde_json::from_slice(&bytes).map_err(|error| {
        BackupError::Other(format!("risposta JSON di Restic non valida: {error}"))
    })?;
    let mut snapshots = values
        .into_iter()
        .filter_map(|value| {
            let id = value.get("id")?.as_str()?.to_string();
            let timestamp = value.get("time")?.as_str()?;
            let created_at = chrono::DateTime::parse_from_rfc3339(timestamp)
                .ok()?
                .timestamp();
            Some(BackupRef {
                job: job_name.to_string(),
                engine: "restic".into(),
                id,
                created_at,
                bytes: None,
            })
        })
        .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.created_at));
    Ok(snapshots)
}

pub(crate) async fn browse(
    job_name: &str,
    config: &Config,
    snapshot: &str,
) -> Result<Vec<BackupEntry>, BackupError> {
    let settings = settings(job_name, config)?;
    let tag = format!("backuppo-{job_name}");
    let mut owned = vec!["ls".to_string(), snapshot.to_string(), "--json".to_string()];
    if snapshot == "latest" {
        owned.extend(["--tag".into(), tag]);
    }
    let arguments: Vec<&str> = owned.iter().map(String::as_str).collect();
    let bytes = output(&settings, &arguments, None).await?;
    let mut entries = Vec::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_slice(line).map_err(|error| {
            BackupError::Other(format!("output JSON di `restic ls` non valido: {error}"))
        })?;
        if value.get("struct_type").and_then(Value::as_str) != Some("node") {
            continue;
        }
        let path = value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if path.ends_with(manifest::MANIFEST_FILENAME) {
            continue;
        }
        entries.push(BackupEntry {
            path: path.trim_start_matches('/').to_string(),
            kind: value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("other")
                .to_string(),
            bytes: value.get("size").and_then(Value::as_u64),
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

pub(crate) async fn restore(
    job_name: &str,
    config: &Config,
    request: &RestoreRequest,
) -> Result<RestoreResult, BackupError> {
    let entries = browse(job_name, config, &request.snapshot).await?;
    let selected = super::recovery::select_entries(&entries, &request.include);
    let files = selected.iter().filter(|entry| entry.kind != "dir").count() as u64;
    let bytes = selected.iter().filter_map(|entry| entry.bytes).sum();
    if request.dry_run {
        return Ok(RestoreResult {
            snapshot: request.snapshot.clone(),
            target: request.target.clone(),
            files,
            bytes,
            dry_run: true,
        });
    }

    super::recovery::prepare_target(&request.target, &request.overwrite)?;
    let settings = settings(job_name, config)?;
    let tag = format!("backuppo-{job_name}");
    let target = request.target.to_string_lossy().into_owned();
    let mut owned = vec![
        "restore".to_string(),
        request.snapshot.clone(),
        "--target".into(),
        target,
        "--overwrite".into(),
        match request.overwrite {
            OverwritePolicy::Never => "never".into(),
            OverwritePolicy::Always => "always".into(),
        },
    ];
    if request.snapshot == "latest" {
        owned.extend(["--tag".into(), tag]);
    }
    for include in &request.include {
        owned.extend(["--include".into(), include.clone()]);
    }
    let arguments: Vec<&str> = owned.iter().map(String::as_str).collect();
    output(&settings, &arguments, None).await?;
    Ok(RestoreResult {
        snapshot: request.snapshot.clone(),
        target: request.target.clone(),
        files,
        bytes,
        dry_run: false,
    })
}
