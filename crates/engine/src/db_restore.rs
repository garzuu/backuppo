//! Restore diretto di un dump Postgres/MySQL in un container gia' in
//! esecuzione o in un server raggiungibile dall'agent, invece di limitarsi
//! a estrarre il dump in una directory locale come fa `bkpo restore`.
//!
//! Per SQLite non serve: il file estratto da `bkpo restore` e' gia' il
//! database. Per immagini disco o VM, vedi la guida al restore manuale.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use backuppo_core::config::{Config, SourceConfig};
use backuppo_core::error::BackupError;
use backuppo_core::model::{OverwritePolicy, RestoreRequest};
use backuppo_sources::docker;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::recovery;

/// Credenziali per applicare il dump al database di destinazione: non sono
/// necessariamente le stesse della sorgente di backup, perche' il target
/// del restore puo' essere un'installazione diversa da quella originale.
pub struct DatabaseCredentials {
    pub user: String,
    pub password: String,
    pub database: String,
}

/// Dove applicare il dump.
pub enum DatabaseRestoreTarget {
    /// Container gia' in esecuzione, raggiunto con `docker exec` (come fa
    /// il backup quando la sorgente ha `container` impostato).
    Container {
        name: String,
        credentials: DatabaseCredentials,
    },
    /// Server raggiungibile direttamente dall'agent con `pg_restore`/`mysql`.
    Server {
        host: String,
        port: u16,
        credentials: DatabaseCredentials,
    },
}

pub struct DatabaseRestoreOutcome {
    pub snapshot: String,
    pub database: String,
    pub bytes: u64,
}

enum DatabaseKind {
    Postgres,
    MySql,
}

/// Ripristina nel database indicato da `target` il dump di un backup
/// Postgres/MySQL (ultimo se `snapshot` e' `"latest"`). Sovrascrive il
/// contenuto del database di destinazione: non e' un merge con i dati
/// esistenti. Funziona sia per l'engine `archive` che per `restic`: estrae
/// il backup con la stessa logica di `bkpo restore`, poi applica il dump
/// trovato con lo strumento del database giusto.
pub async fn restore_database(
    job_name: &str,
    config: &Config,
    snapshot: &str,
    target: &DatabaseRestoreTarget,
) -> Result<DatabaseRestoreOutcome, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato")))?;
    let (kind, dump_filename) = match &job.source {
        SourceConfig::Postgres { .. } => (DatabaseKind::Postgres, "dump.pgcustom"),
        SourceConfig::MySql { .. } => (DatabaseKind::MySql, "dump.sql"),
        _ => {
            return Err(BackupError::Other(
                "il restore diretto in un database e' disponibile solo per sorgenti \
                 'postgres' o 'mysql'; per 'sqlite' usa 'bkpo restore': il file \
                 estratto e' gia' il database"
                    .into(),
            ));
        }
    };

    let staging = tempfile::tempdir()?;
    let request = RestoreRequest {
        snapshot: snapshot.to_string(),
        target: staging.path().to_path_buf(),
        include: Vec::new(),
        dry_run: false,
        overwrite: OverwritePolicy::Always,
    };
    let result = recovery::restore(job_name, config, &request).await?;

    // L'engine restic ripristina preservando il percorso assoluto originale
    // della staging directory (non il contenuto a livello radice come fa
    // l'engine archive): cerchiamo il file per nome invece di assumerne la
    // posizione.
    let dump_path = find_file(staging.path(), dump_filename)?;
    let dump = tokio::fs::read(&dump_path).await.map_err(|error| {
        BackupError::Other(format!(
            "lettura di '{}' fallita: {error}",
            dump_path.display()
        ))
    })?;

    let database = match kind {
        DatabaseKind::Postgres => restore_postgres(target, &dump).await?,
        DatabaseKind::MySql => restore_mysql(target, &dump).await?,
    };

    Ok(DatabaseRestoreOutcome {
        snapshot: result.snapshot,
        database,
        bytes: dump.len() as u64,
    })
}

fn find_file(root: &Path, filename: &str) -> Result<PathBuf, BackupError> {
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|error| BackupError::Other(error.to_string()))?;
        if entry.file_name() == filename {
            return Ok(entry.path().to_path_buf());
        }
    }
    Err(BackupError::Other(format!(
        "file '{filename}' non trovato nel backup estratto"
    )))
}

async fn restore_postgres(
    target: &DatabaseRestoreTarget,
    dump: &[u8],
) -> Result<String, BackupError> {
    match target {
        DatabaseRestoreTarget::Container { name, credentials } => {
            let client = docker::connect()?;
            let result = docker::exec(
                &client,
                name,
                vec![
                    "pg_restore".into(),
                    "--exit-on-error".into(),
                    "--no-owner".into(),
                    "--clean".into(),
                    "--if-exists".into(),
                    format!("--username={}", credentials.user),
                    format!("--dbname={}", credentials.database),
                ],
                Some(vec![format!("PGPASSWORD={}", credentials.password)]),
                Some(dump),
            )
            .await?;
            check_exit("pg_restore", result.exit_code, &result.stderr)?;
            Ok(credentials.database.clone())
        }
        DatabaseRestoreTarget::Server {
            host,
            port,
            credentials,
        } => {
            run_piped(
                "pg_restore",
                &[
                    "--exit-on-error".to_string(),
                    "--no-owner".to_string(),
                    "--clean".to_string(),
                    "--if-exists".to_string(),
                    "--host".to_string(),
                    host.clone(),
                    "--port".to_string(),
                    port.to_string(),
                    format!("--username={}", credentials.user),
                    format!("--dbname={}", credentials.database),
                ],
                &[("PGPASSWORD", credentials.password.as_str())],
                dump,
            )
            .await?;
            Ok(credentials.database.clone())
        }
    }
}

async fn restore_mysql(target: &DatabaseRestoreTarget, dump: &[u8]) -> Result<String, BackupError> {
    match target {
        DatabaseRestoreTarget::Container { name, credentials } => {
            let client = docker::connect()?;
            let result = docker::exec(
                &client,
                name,
                vec![
                    "mysql".into(),
                    format!("--user={}", credentials.user),
                    credentials.database.clone(),
                ],
                Some(vec![format!("MYSQL_PWD={}", credentials.password)]),
                Some(dump),
            )
            .await?;
            check_exit("mysql", result.exit_code, &result.stderr)?;
            Ok(credentials.database.clone())
        }
        DatabaseRestoreTarget::Server {
            host,
            port,
            credentials,
        } => {
            run_piped(
                "mysql",
                &[
                    "--host".to_string(),
                    host.clone(),
                    "--port".to_string(),
                    port.to_string(),
                    format!("--user={}", credentials.user),
                    credentials.database.clone(),
                ],
                &[("MYSQL_PWD", credentials.password.as_str())],
                dump,
            )
            .await?;
            Ok(credentials.database.clone())
        }
    }
}

fn check_exit(command: &str, exit_code: i64, stderr: &[u8]) -> Result<(), BackupError> {
    if exit_code != 0 {
        return Err(BackupError::Other(format!(
            "{command} è fallito (exit code {exit_code}): {}",
            String::from_utf8_lossy(stderr)
        )));
    }
    Ok(())
}

/// Esegue `binary` localmente passandogli `dump` su stdin: percorso usato
/// per il target 'Server' (nessun container di mezzo).
async fn run_piped(
    binary: &str,
    args: &[String],
    env: &[(&str, &str)],
    stdin_data: &[u8],
) -> Result<(), BackupError> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| BackupError::Other(format!("impossibile avviare '{binary}': {error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .expect("stdin richiesta con Stdio::piped");
    stdin.write_all(stdin_data).await.map_err(|error| {
        BackupError::Other(format!("scrittura stdin di '{binary}' fallita: {error}"))
    })?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| BackupError::Other(format!("'{binary}' interrotto: {error}")))?;
    if !output.status.success() {
        return Err(BackupError::Other(format!(
            "{binary} è fallito ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}
