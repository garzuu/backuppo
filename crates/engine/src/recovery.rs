use std::path::{Component, Path};

use backuppo_core::config::{Config, EngineKind};
use backuppo_core::error::BackupError;
use backuppo_core::model::{
    BackupEntry, BackupRef, OverwritePolicy, RestoreRequest, RestoreResult,
};

use crate::{archive, manifest, naming, restic, runner};

pub async fn snapshots(job_name: &str, config: &Config) -> Result<Vec<BackupRef>, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;
    if job.engine == EngineKind::Restic {
        return restic::snapshots(job_name, config).await;
    }
    let destination = destination(job_name, config)?;
    let prefix = format!("{job_name}-");
    let mut backups = destination
        .list()
        .await?
        .into_iter()
        .filter(|name| name.starts_with(&prefix))
        .filter_map(|name| {
            let created_at = naming::extract_timestamp(&name)? as i64;
            Some(BackupRef {
                job: job_name.to_string(),
                engine: "archive".into(),
                id: name,
                created_at,
                bytes: None,
            })
        })
        .collect::<Vec<_>>();
    backups.sort_by_key(|backup| std::cmp::Reverse(backup.created_at));
    Ok(backups)
}

pub async fn browse(
    job_name: &str,
    config: &Config,
    snapshot: &str,
) -> Result<Vec<BackupEntry>, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;
    if job.engine == EngineKind::Restic {
        return restic::browse(job_name, config, snapshot).await;
    }
    let extracted = extract_native(job_name, config, snapshot).await?;
    let manifest = manifest::read(extracted.path())?;
    Ok(manifest
        .files
        .into_iter()
        .map(|entry| BackupEntry {
            path: entry.path,
            kind: "file".into(),
            bytes: Some(entry.size),
        })
        .collect())
}

pub async fn restore(
    job_name: &str,
    config: &Config,
    request: &RestoreRequest,
) -> Result<RestoreResult, BackupError> {
    let observer = crate::observability::ExecutionObserver::start(
        config,
        job_name,
        if request.dry_run {
            "restore_dry_run"
        } else {
            "restore"
        },
    );
    let result = restore_impl(job_name, config, request).await;
    observer.finish_restore(config, &result);
    result
}

async fn restore_impl(
    job_name: &str,
    config: &Config,
    request: &RestoreRequest,
) -> Result<RestoreResult, BackupError> {
    validate_includes(&request.include)?;
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;
    if job.engine == EngineKind::Restic {
        return restic::restore(job_name, config, request).await;
    }

    let extracted = extract_native(job_name, config, &request.snapshot).await?;
    let manifest = manifest::read(extracted.path())?;
    let entries = manifest
        .files
        .iter()
        .map(|entry| BackupEntry {
            path: entry.path.clone(),
            kind: "file".into(),
            bytes: Some(entry.size),
        })
        .collect::<Vec<_>>();
    let selected = select_entries(&entries, &request.include);
    let files = selected.len() as u64;
    let bytes = selected.iter().filter_map(|entry| entry.bytes).sum();
    if !request.dry_run {
        prepare_target(&request.target, &request.overwrite)?;
        for entry in selected {
            let relative = Path::new(&entry.path);
            let source = extracted.path().join(relative);
            let target = request.target.join(relative);
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::copy(&source, &target).await.map_err(|error| {
                BackupError::Other(format!("ripristino di '{}' fallito: {error}", entry.path))
            })?;
        }
    }
    Ok(RestoreResult {
        snapshot: resolve_snapshot(job_name, config, &request.snapshot).await?,
        target: request.target.clone(),
        files,
        bytes,
        dry_run: request.dry_run,
    })
}

pub(crate) fn select_entries<'a>(
    entries: &'a [BackupEntry],
    includes: &[String],
) -> Vec<&'a BackupEntry> {
    if includes.is_empty() {
        return entries.iter().collect();
    }
    entries
        .iter()
        .filter(|entry| {
            includes.iter().any(|include| {
                let include = include.trim_matches('/');
                entry.path == include
                    || entry
                        .path
                        .strip_prefix(include)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        })
        .collect()
}

pub(crate) fn prepare_target(
    target: &Path,
    overwrite: &OverwritePolicy,
) -> Result<(), BackupError> {
    if target.exists() {
        if !target.is_dir() {
            return Err(BackupError::Other(format!(
                "la destinazione restore '{}' non e' una directory",
                target.display()
            )));
        }
        if matches!(overwrite, OverwritePolicy::Never)
            && std::fs::read_dir(target)?.next().is_some()
        {
            return Err(BackupError::Other(format!(
                "la destinazione restore '{}' non e' vuota; usare --overwrite per autorizzare la sostituzione",
                target.display()
            )));
        }
    } else {
        std::fs::create_dir_all(target)?;
    }
    Ok(())
}

fn validate_includes(includes: &[String]) -> Result<(), BackupError> {
    for include in includes {
        let path = Path::new(include);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(BackupError::Other(format!(
                "percorso include non sicuro: '{include}'"
            )));
        }
    }
    Ok(())
}

async fn extract_native(
    job_name: &str,
    config: &Config,
    requested: &str,
) -> Result<tempfile::TempDir, BackupError> {
    let snapshot = resolve_snapshot(job_name, config, requested).await?;
    let job = config.jobs.get(job_name).expect("job checked");
    let destination = destination(job_name, config)?;
    let encrypted = snapshot.ends_with(".age");
    let base = snapshot.strip_suffix(".age").unwrap_or(&snapshot);
    let compressed = base.ends_with(".zst");
    let passphrase = if encrypted {
        runner::resolve_encryption(job_name, job.encryption.as_ref())?
    } else {
        None
    };
    let download = tempfile::tempdir()?;
    let archive_path = download.path().join("backup");
    let result = tempfile::tempdir()?;
    let extracted = result.path().to_path_buf();
    destination.download(&snapshot, &archive_path).await?;
    let archive_task = archive_path.clone();
    let extracted_task = extracted.clone();
    tokio::task::spawn_blocking(move || {
        archive::extract_archive(
            &archive_task,
            &extracted_task,
            compressed,
            passphrase.as_ref(),
        )
    })
    .await
    .map_err(|error| BackupError::Other(format!("task restore interrotto: {error}")))??;

    Ok(result)
}

async fn resolve_snapshot(
    job_name: &str,
    config: &Config,
    requested: &str,
) -> Result<String, BackupError> {
    let available = snapshots(job_name, config).await?;
    if requested == "latest" {
        return available
            .first()
            .map(|backup| backup.id.clone())
            .ok_or_else(|| BackupError::Other(format!("nessun backup trovato per '{job_name}'")));
    }
    available
        .into_iter()
        .find(|backup| backup.id == requested)
        .map(|backup| backup.id)
        .ok_or_else(|| {
            BackupError::Other(format!(
                "backup '{requested}' non trovato per il job '{job_name}'"
            ))
        })
}

fn destination(
    job_name: &str,
    config: &Config,
) -> Result<Box<dyn backuppo_core::traits::Destination>, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;
    let destination = config.destinations.get(&job.destination).ok_or_else(|| {
        BackupError::Other(format!("destination '{}' non trovata", job.destination))
    })?;
    backuppo_destinations::build(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_select_a_file_or_a_subtree() {
        let entries = vec![
            BackupEntry {
                path: "docs/a.txt".into(),
                kind: "file".into(),
                bytes: Some(1),
            },
            BackupEntry {
                path: "docs/sub/b.txt".into(),
                kind: "file".into(),
                bytes: Some(2),
            },
            BackupEntry {
                path: "other.txt".into(),
                kind: "file".into(),
                bytes: Some(3),
            },
        ];
        let selected = select_entries(&entries, &["docs".into()]);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn rejects_parent_directory_includes() {
        assert!(validate_includes(&["../secret".into()]).is_err());
        assert!(validate_includes(&["safe/path".into()]).is_ok());
    }
}
