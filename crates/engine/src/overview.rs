//! Vista aggregata job + ultimo esito, condivisa da `/api/v1/status`
//! (`crates/cli/src/api.rs`) e dal server MCP (`crates/cli/src/mcp.rs`):
//! prima di questo modulo ciascuno ricostruiva lo stesso join a mano.

use backuppo_core::config::Config;
use backuppo_core::error::BackupError;

use crate::history::{ExecutionRecord, HistoryStore};

pub struct JobOverview {
    pub job: String,
    pub schedule: String,
    pub latest: Option<ExecutionRecord>,
    pub latest_verification: Option<ExecutionRecord>,
}

/// Un job si considera "in difficoltà" se l'ultima esecuzione nota non è
/// riuscita, o se l'ultima verifica di restore nota non è riuscita. Un job
/// mai eseguito o mai verificato non conta come in difficoltà: è un job
/// nuovo, non uno rotto.
impl JobOverview {
    pub fn is_failing(&self) -> bool {
        let run_failed = self
            .latest
            .as_ref()
            .is_some_and(|record| record.status != "success");
        let verify_failed = self
            .latest_verification
            .as_ref()
            .is_some_and(|record| record.verification_status != "success");
        run_failed || verify_failed
    }
}

/// Elenca tutti i job di `config` ordinati per nome, con l'ultima
/// esecuzione e l'ultima verifica note in `store` (entrambe `None` se il
/// job non è mai stato eseguito).
pub fn jobs_overview(
    config: &Config,
    store: &HistoryStore,
) -> Result<Vec<JobOverview>, BackupError> {
    let mut jobs: Vec<_> = config.jobs.iter().collect();
    jobs.sort_by_key(|(name, _)| *name);
    jobs.into_iter()
        .map(|(job, settings)| {
            Ok(JobOverview {
                latest: store.latest_for_job(job)?,
                latest_verification: store.latest_verification_for_job(job)?,
                job: job.clone(),
                schedule: settings.schedule.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::config::{EngineKind, JobConfig, SourceConfig};
    use std::collections::HashMap;

    fn job() -> JobConfig {
        JobConfig {
            source: SourceConfig::Folder {
                path: "/srv/data".to_string(),
                exclude: Vec::new(),
            },
            destination: "local".to_string(),
            engine: EngineKind::Archive,
            compression: None,
            encryption: None,
            schedule: "0 3 * * *".to_string(),
            verify_restore: None,
            max_backup_age_hours: None,
            retention: Default::default(),
            notify: Default::default(),
            pre: Vec::new(),
            post: Vec::new(),
        }
    }

    fn config_with(jobs: HashMap<String, JobConfig>) -> Config {
        Config {
            destinations: HashMap::new(),
            notifiers: HashMap::new(),
            jobs,
            reports: Vec::new(),
            observability: None,
            api: Default::default(),
            updates: None,
        }
    }

    #[test]
    fn a_never_run_job_is_not_failing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = HistoryStore::open(temp.path().join("history.sqlite")).expect("store");
        let mut jobs = HashMap::new();
        jobs.insert("documenti".to_string(), job());
        let config = config_with(jobs);

        let overview = jobs_overview(&config, &store).expect("overview");
        assert_eq!(overview.len(), 1);
        assert!(!overview[0].is_failing());
    }

    #[test]
    fn a_job_whose_last_run_failed_is_failing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = HistoryStore::open(temp.path().join("history.sqlite")).expect("store");
        let id = store.start("documenti", "backup", 100).expect("start");
        store
            .finish(
                id,
                &crate::history::ExecutionCompletion {
                    status: "failure",
                    finished_at: 101,
                    duration_ms: 10,
                    bytes: None,
                    error: Some("disco pieno"),
                    verification_status: "not_run",
                    log: "errore\n",
                },
            )
            .expect("finish");
        let mut jobs = HashMap::new();
        jobs.insert("documenti".to_string(), job());
        let config = config_with(jobs);

        let overview = jobs_overview(&config, &store).expect("overview");
        assert!(overview[0].is_failing());
    }
}
