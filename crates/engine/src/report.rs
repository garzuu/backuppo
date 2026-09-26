use backuppo_core::config::Config;
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;

use crate::history::HistoryStore;

/// Genera e invia un report sullo stato reale registrato nello storico.
pub async fn send(
    config: &Config,
    days: u32,
    notifiers: &[String],
) -> Result<JobEvent, BackupError> {
    let event = build(config, days)?;
    crate::notify::dispatch(config, notifiers, &event).await;
    Ok(event)
}

/// Costruisce il report senza inviarlo, utile anche per CLI e test.
pub fn build(config: &Config, days: u32) -> Result<JobEvent, BackupError> {
    let settings = config.observability.as_ref().ok_or_else(|| {
        BackupError::Other(
            "i report richiedono la sezione 'observability' nella configurazione".to_string(),
        )
    })?;
    let store = HistoryStore::open(&settings.history_path)?;
    let since = chrono::Utc::now()
        .timestamp()
        .saturating_sub(i64::from(days) * 86_400);
    let records = store.since(since)?;
    let backup_records: Vec<_> = records
        .iter()
        .filter(|record| record.kind == "backup")
        .collect();
    let succeeded = backup_records
        .iter()
        .filter(|record| record.status == "success")
        .count();
    let failed = backup_records
        .iter()
        .filter(|record| record.status == "failure")
        .count();
    let bytes = backup_records
        .iter()
        .filter_map(|record| record.bytes)
        .fold(0u64, u64::saturating_add);

    let mut summary = format!(
        "Backuppo — report ultimi {days} giorni\nBackup: {succeeded} riusciti, {failed} falliti, {bytes} byte totali"
    );
    let mut jobs: Vec<&String> = config.jobs.keys().collect();
    jobs.sort();
    for job in jobs {
        let latest = store.latest_for_job(job)?;
        let verification = store.latest_verification_for_job(job)?;
        let backup_text = latest
            .as_ref()
            .map(|record| {
                format!(
                    "{} ({})",
                    record.status,
                    format_timestamp(record.started_at)
                )
            })
            .unwrap_or_else(|| "mai eseguito".to_string());
        let verify_text = verification
            .as_ref()
            .map(|record| {
                format!(
                    "{} ({})",
                    record.verification_status,
                    format_timestamp(record.started_at)
                )
            })
            .unwrap_or_else(|| "mai verificato".to_string());
        summary.push_str(&format!(
            "\n- {job}: ultimo backup {backup_text}; ultimo restore test {verify_text}"
        ));
    }
    Ok(JobEvent::Report { summary })
}

fn format_timestamp(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::ExecutionCompletion;
    use backuppo_core::config::ObservabilityConfig;
    use std::collections::HashMap;

    #[test]
    fn report_counts_successes_and_failures_from_history() {
        let temp = tempfile::tempdir().expect("tempdir");
        let history_path = temp.path().join("history.sqlite");
        let store = HistoryStore::open(&history_path).expect("store");
        let now = chrono::Utc::now().timestamp();
        for (job, status, bytes) in [("one", "success", Some(10)), ("two", "failure", None)] {
            let id = store.start(job, "backup", now).expect("start");
            store
                .finish(
                    id,
                    &ExecutionCompletion {
                        status,
                        finished_at: now,
                        duration_ms: 10,
                        bytes,
                        error: None,
                        verification_status: "not_run",
                        log: "done\n",
                    },
                )
                .expect("finish");
        }
        let config = Config {
            destinations: HashMap::new(),
            notifiers: HashMap::new(),
            jobs: HashMap::new(),
            observability: Some(ObservabilityConfig {
                history_path: history_path.to_string_lossy().to_string(),
                status_page: None,
            }),
            reports: Vec::new(),
            api: None,
            updates: None,
        };

        let JobEvent::Report { summary } = build(&config, 7).expect("report") else {
            panic!("evento inatteso");
        };
        assert!(summary.contains("1 riusciti, 1 falliti, 10 byte"));
    }
}
