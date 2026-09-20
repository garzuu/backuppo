use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use rusqlite::backup::Backup;
use rusqlite::Connection;

const DUMP_FILENAME: &str = "dump.sqlite";

/// Sorgente SQLite: usa l'Online Backup API di SQLite (`rusqlite::backup`)
/// per produrre una copia consistente del database anche mentre è in uso,
/// equivalente al comando `.backup` della CLI `sqlite3`.
pub struct SqliteSource {
    path: PathBuf,
}

impl SqliteSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl Source for SqliteSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        tokio::fs::create_dir_all(staging).await?;
        let source_path = self.path.clone();
        let dest_path = staging.join(DUMP_FILENAME);
        let dest_path_for_task = dest_path.clone();

        tokio::task::spawn_blocking(move || backup_sqlite(&source_path, &dest_path_for_task))
            .await
            .map_err(|e| BackupError::Other(format!("task di backup sqlite interrotto: {e}")))??;

        let bytes = tokio::fs::metadata(&dest_path).await?.len();

        Ok(Artifact {
            path: staging.to_path_buf(),
            bytes,
            files: 1,
            checksum: String::new(),
        })
    }

    async fn cleanup(&self, artifact: &Artifact) -> Result<(), BackupError> {
        if tokio::fs::metadata(&artifact.path).await.is_ok() {
            tokio::fs::remove_dir_all(&artifact.path).await?;
        }
        Ok(())
    }

    async fn verify_restore(&self, restored_dir: &Path) -> Result<(), BackupError> {
        let db_path = restored_dir.join(DUMP_FILENAME);
        tokio::task::spawn_blocking(move || check_integrity(&db_path))
            .await
            .map_err(|e| BackupError::Other(format!("task di verifica sqlite interrotto: {e}")))?
    }
}

fn backup_sqlite(source_path: &Path, dest_path: &Path) -> Result<(), BackupError> {
    let src = Connection::open(source_path).map_err(|e| {
        BackupError::Other(format!(
            "impossibile aprire il database sqlite '{}': {e}",
            source_path.display()
        ))
    })?;
    let mut dst = Connection::open(dest_path)
        .map_err(|e| BackupError::Other(format!("impossibile creare il file di backup: {e}")))?;

    let backup = Backup::new(&src, &mut dst)
        .map_err(|e| BackupError::Other(format!("impossibile avviare il backup sqlite: {e}")))?;
    backup
        .run_to_completion(100, Duration::from_millis(100), None)
        .map_err(|e| BackupError::Other(format!("backup sqlite fallito: {e}")))
}

fn check_integrity(db_path: &Path) -> Result<(), BackupError> {
    let conn = Connection::open(db_path).map_err(|e| {
        BackupError::Other(format!(
            "impossibile aprire il dump sqlite ripristinato: {e}"
        ))
    })?;
    let result: String = conn
        .query_row("PRAGMA integrity_check;", [], |row| row.get(0))
        .map_err(|e| BackupError::Other(format!("PRAGMA integrity_check fallita: {e}")))?;
    if result != "ok" {
        return Err(BackupError::Other(format!(
            "integrity_check ha riportato problemi: {result}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_a_consistent_backup_and_verifies_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_path = temp.path().join("live.sqlite");
        let connection = Connection::open(&source_path).expect("database sorgente");
        connection
            .execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, value TEXT NOT NULL);\
                 INSERT INTO items(value) VALUES ('alpha'), ('beta');",
            )
            .expect("dati di test");

        let staging = temp.path().join("staging");
        let source = SqliteSource::new(&source_path);
        let artifact = source.prepare(&staging).await.expect("backup sqlite");
        assert_eq!(artifact.files, 1);
        assert!(artifact.bytes > 0);

        source
            .verify_restore(&staging)
            .await
            .expect("integrity check");
        let restored = Connection::open(staging.join(DUMP_FILENAME)).expect("dump");
        let count: i64 = restored
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .expect("query sul dump");
        assert_eq!(count, 2);
    }
}
