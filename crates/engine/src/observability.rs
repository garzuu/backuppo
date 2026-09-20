use std::time::Instant;

use backuppo_core::config::Config;
use backuppo_core::model::Artifact;
use tracing::warn;

use crate::history::{ExecutionCompletion, HistoryStore};

pub(crate) struct ExecutionObserver {
    store: Option<HistoryStore>,
    id: Option<i64>,
    timer: Instant,
    verification_status: &'static str,
}

impl ExecutionObserver {
    pub(crate) fn start(config: &Config, job: &str, kind: &str) -> Self {
        let started_at = chrono::Utc::now().timestamp();
        let mut observer = Self {
            store: None,
            id: None,
            timer: Instant::now(),
            verification_status: "not_run",
        };
        let Some(settings) = &config.observability else {
            return observer;
        };

        match HistoryStore::open(&settings.history_path)
            .and_then(|store| store.start(job, kind, started_at).map(|id| (store, id)))
        {
            Ok((store, id)) => {
                observer.store = Some(store);
                observer.id = Some(id);
            }
            Err(error) => {
                warn!(job, %error, "impossibile avviare la registrazione nello storico");
            }
        }
        observer
    }

    pub(crate) fn verification_started(&mut self) {
        self.verification_status = "failed";
        self.log("verifica restore avviata");
    }

    pub(crate) fn verification_succeeded(&mut self) {
        self.verification_status = "success";
        self.log("verifica restore completata");
    }

    pub(crate) fn log(&self, message: &str) {
        if let (Some(store), Some(id)) = (&self.store, self.id) {
            let line = format!("{} {message}\n", now_rfc3339());
            if let Err(error) = store.append_log(id, &line) {
                warn!(execution_id = id, %error, "impossibile aggiornare il log esecuzione");
            }
        }
    }

    pub(crate) fn finish_backup(
        &self,
        config: &Config,
        result: &Result<Artifact, backuppo_core::error::BackupError>,
    ) {
        let (status, bytes, error, message) = match result {
            Ok(artifact) => (
                "success",
                Some(artifact.bytes),
                None,
                format!(
                    "{} backup completato: {} byte, {} file\n",
                    now_rfc3339(),
                    artifact.bytes,
                    artifact.files
                ),
            ),
            Err(error) => (
                "failure",
                None,
                Some(error.to_string()),
                format!("{} backup fallito: {error}\n", now_rfc3339()),
            ),
        };
        self.finish(config, status, bytes, error.as_deref(), message);
    }

    pub(crate) fn finish_verify(
        &mut self,
        config: &Config,
        result: &Result<backuppo_core::model::JobEvent, backuppo_core::error::BackupError>,
    ) {
        let (status, error, message) = match result {
            Ok(event) => {
                self.verification_succeeded();
                ("success", None, format!("{} {event}\n", now_rfc3339()))
            }
            Err(error) => {
                self.verification_status = "failed";
                (
                    "failure",
                    Some(error.to_string()),
                    format!("{} verifica fallita: {error}\n", now_rfc3339()),
                )
            }
        };
        self.finish(config, status, None, error.as_deref(), message);
    }

    fn finish(
        &self,
        config: &Config,
        status: &str,
        bytes: Option<u64>,
        error: Option<&str>,
        message: String,
    ) {
        if let (Some(store), Some(id)) = (&self.store, self.id) {
            let completion = ExecutionCompletion {
                status,
                finished_at: chrono::Utc::now().timestamp(),
                duration_ms: self.timer.elapsed().as_millis(),
                bytes,
                error,
                verification_status: self.verification_status,
                log: &message,
            };
            if let Err(history_error) = store.finish(id, &completion) {
                warn!(execution_id = id, error = %history_error, "impossibile aggiornare lo storico");
            }
        }

        if let Err(error) = crate::status::generate(config) {
            warn!(%error, "generazione pagina di stato fallita");
        }
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
