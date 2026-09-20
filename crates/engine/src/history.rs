use std::path::{Path, PathBuf};

use backuppo_core::error::BackupError;
use rusqlite::{params, Connection, OptionalExtension, Row};

/// Riga dello storico locale, usata anche dai comandi CLI `runs` e `logs`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExecutionRecord {
    pub id: i64,
    pub job: String,
    pub kind: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub bytes: Option<u64>,
    pub error: Option<String>,
    pub verification_status: String,
    pub log: String,
}

pub(crate) struct ExecutionCompletion<'a> {
    pub status: &'a str,
    pub finished_at: i64,
    pub duration_ms: u128,
    pub bytes: Option<u64>,
    pub error: Option<&'a str>,
    pub verification_status: &'a str,
    pub log: &'a str,
}

/// Accesso al database SQLite locale. La struct conserva solo il percorso:
/// ogni operazione apre una connessione breve, così può essere usata senza
/// condividere una `Connection` tra task concorrenti.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, BackupError> {
        let store = Self { path: path.into() };
        store.initialize()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn start(&self, job: &str, kind: &str, started_at: i64) -> Result<i64, BackupError> {
        let connection = self.connect()?;
        let log = format!(
            "{} esecuzione {kind} avviata\n",
            format_timestamp(started_at)
        );
        connection
            .execute(
                "INSERT INTO executions
                 (job, kind, status, started_at, verification_status, log)
                 VALUES (?1, ?2, 'running', ?3, 'not_run', ?4)",
                params![job, kind, started_at, log],
            )
            .map_err(sql_error("creazione esecuzione"))?;
        Ok(connection.last_insert_rowid())
    }

    pub(crate) fn finish(
        &self,
        id: i64,
        completion: &ExecutionCompletion<'_>,
    ) -> Result<(), BackupError> {
        let connection = self.connect()?;
        let duration_ms = i64::try_from(completion.duration_ms).unwrap_or(i64::MAX);
        let bytes = completion
            .bytes
            .map(|value| i64::try_from(value).unwrap_or(i64::MAX));
        connection
            .execute(
                "UPDATE executions SET
                    status = ?2,
                    finished_at = ?3,
                    duration_ms = ?4,
                    bytes = ?5,
                    error = ?6,
                    verification_status = ?7,
                    log = log || ?8
                 WHERE id = ?1",
                params![
                    id,
                    completion.status,
                    completion.finished_at,
                    duration_ms,
                    bytes,
                    completion.error,
                    completion.verification_status,
                    completion.log,
                ],
            )
            .map_err(sql_error("chiusura esecuzione"))?;
        Ok(())
    }

    pub(crate) fn append_log(&self, id: i64, message: &str) -> Result<(), BackupError> {
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE executions SET log = log || ?2 WHERE id = ?1",
                params![id, message],
            )
            .map_err(sql_error("aggiornamento log esecuzione"))?;
        Ok(())
    }

    pub fn list(&self, limit: usize) -> Result<Vec<ExecutionRecord>, BackupError> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, job, kind, status, started_at, finished_at,
                        duration_ms, bytes, error, verification_status, log
                 FROM executions ORDER BY id DESC LIMIT ?1",
            )
            .map_err(sql_error("preparazione elenco esecuzioni"))?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = statement
            .query_map([limit], record_from_row)
            .map_err(sql_error("lettura elenco esecuzioni"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sql_error("lettura esecuzione"))
    }

    pub fn get(&self, id: i64) -> Result<Option<ExecutionRecord>, BackupError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, job, kind, status, started_at, finished_at,
                        duration_ms, bytes, error, verification_status, log
                 FROM executions WHERE id = ?1",
                [id],
                record_from_row,
            )
            .optional()
            .map_err(sql_error("lettura esecuzione"))
    }

    pub(crate) fn since(&self, timestamp: i64) -> Result<Vec<ExecutionRecord>, BackupError> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, job, kind, status, started_at, finished_at,
                        duration_ms, bytes, error, verification_status, log
                 FROM executions WHERE started_at >= ?1 ORDER BY id DESC",
            )
            .map_err(sql_error("preparazione storico report"))?;
        let rows = statement
            .query_map([timestamp], record_from_row)
            .map_err(sql_error("lettura storico report"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sql_error("lettura esecuzione del report"))
    }

    pub(crate) fn latest_for_job(&self, job: &str) -> Result<Option<ExecutionRecord>, BackupError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, job, kind, status, started_at, finished_at,
                        duration_ms, bytes, error, verification_status, log
                 FROM executions
                 WHERE job = ?1 AND kind = 'backup'
                 ORDER BY id DESC LIMIT 1",
                [job],
                record_from_row,
            )
            .optional()
            .map_err(sql_error("lettura ultima esecuzione"))
    }

    pub(crate) fn latest_verification_for_job(
        &self,
        job: &str,
    ) -> Result<Option<ExecutionRecord>, BackupError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id, job, kind, status, started_at, finished_at,
                        duration_ms, bytes, error, verification_status, log
                 FROM executions
                 WHERE job = ?1 AND verification_status != 'not_run'
                 ORDER BY id DESC LIMIT 1",
                [job],
                record_from_row,
            )
            .optional()
            .map_err(sql_error("lettura ultima verifica"))
    }

    fn connect(&self) -> Result<Connection, BackupError> {
        if self.path != Path::new(":memory:") {
            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
        let connection = Connection::open(&self.path).map_err(|error| {
            BackupError::Other(format!(
                "impossibile aprire lo storico SQLite '{}': {error}",
                self.path.display()
            ))
        })?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql_error("configurazione timeout SQLite"))?;
        Ok(connection)
    }

    fn initialize(&self) -> Result<(), BackupError> {
        let connection = self.connect()?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS executions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    started_at INTEGER NOT NULL,
                    finished_at INTEGER,
                    duration_ms INTEGER,
                    bytes INTEGER,
                    error TEXT,
                    verification_status TEXT NOT NULL,
                    log TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS executions_job_id
                    ON executions(job, id DESC);
                 CREATE INDEX IF NOT EXISTS executions_started_at
                    ON executions(started_at DESC);",
            )
            .map_err(sql_error("inizializzazione schema storico"))?;
        Ok(())
    }
}

fn record_from_row(row: &Row<'_>) -> rusqlite::Result<ExecutionRecord> {
    let bytes: Option<i64> = row.get(7)?;
    Ok(ExecutionRecord {
        id: row.get(0)?,
        job: row.get(1)?,
        kind: row.get(2)?,
        status: row.get(3)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        duration_ms: row.get(6)?,
        bytes: bytes.map(|value| value.max(0) as u64),
        error: row.get(8)?,
        verification_status: row.get(9)?,
        log: row.get(10)?,
    })
}

fn sql_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> BackupError {
    move |error| BackupError::Other(format!("{context}: {error}"))
}

fn format_timestamp(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_a_completed_execution() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = HistoryStore::open(temp.path().join("history.sqlite")).expect("store");
        let id = store.start("documents", "backup", 100).expect("start");
        store
            .finish(
                id,
                &ExecutionCompletion {
                    status: "success",
                    finished_at: 102,
                    duration_ms: 1500,
                    bytes: Some(42),
                    error: None,
                    verification_status: "success",
                    log: "completata\n",
                },
            )
            .expect("finish");

        let record = store.get(id).expect("get").expect("record");
        assert_eq!(record.job, "documents");
        assert_eq!(record.status, "success");
        assert_eq!(record.bytes, Some(42));
        assert!(record.log.contains("completata"));
        assert_eq!(store.list(10).expect("list"), vec![record]);
    }
}
