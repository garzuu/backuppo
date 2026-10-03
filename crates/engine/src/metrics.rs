//! Esposizione dello storico locale in formato di testo Prometheus, per
//! `GET /metrics`. Stessa fonte dati di [`crate::status::generate`] (la
//! status page HTML), solo il formato di output cambia.

use std::fmt::Write;

use backuppo_core::config::Config;
use backuppo_core::error::BackupError;

use crate::history::{ExecutionRecord, HistoryStore};

struct JobMetrics {
    job: String,
    last_run_timestamp: Option<i64>,
    last_run_success: Option<bool>,
    last_run_duration_ms: Option<i64>,
    last_run_bytes: Option<u64>,
    last_verify_timestamp: Option<i64>,
    last_verify_success: Option<bool>,
}

/// Genera il testo in formato Prometheus per tutti i job di `config`,
/// interrogando `store` per l'ultima esecuzione e l'ultima verifica di
/// ciascuno. Un job mai eseguito non emette gauge (convenzione
/// Prometheus: l'assenza si interroga con `absent()`, non con zeri finti).
pub fn render(config: &Config, store: &HistoryStore) -> Result<String, BackupError> {
    let mut jobs: Vec<&String> = config.jobs.keys().collect();
    jobs.sort();

    let metrics = jobs
        .into_iter()
        .map(|job| job_metrics(job, store))
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = String::new();
    write_gauge(
        &mut out,
        "backuppo_last_run_timestamp_seconds",
        "Unix timestamp dell'inizio dell'ultima esecuzione.",
        &metrics,
        |m| m.last_run_timestamp.map(|v| v as f64),
    )?;
    write_gauge(
        &mut out,
        "backuppo_last_run_success",
        "1 se l'ultima esecuzione e' riuscita, 0 altrimenti.",
        &metrics,
        |m| m.last_run_success.map(|v| if v { 1.0 } else { 0.0 }),
    )?;
    write_gauge(
        &mut out,
        "backuppo_last_run_duration_ms",
        "Durata dell'ultima esecuzione in millisecondi.",
        &metrics,
        |m| m.last_run_duration_ms.map(|v| v as f64),
    )?;
    write_gauge(
        &mut out,
        "backuppo_last_run_bytes",
        "Byte trasferiti nell'ultima esecuzione.",
        &metrics,
        |m| m.last_run_bytes.map(|v| v as f64),
    )?;
    write_gauge(
        &mut out,
        "backuppo_last_verify_timestamp_seconds",
        "Unix timestamp dell'inizio dell'ultima verifica di restore.",
        &metrics,
        |m| m.last_verify_timestamp.map(|v| v as f64),
    )?;
    write_gauge(
        &mut out,
        "backuppo_last_verify_success",
        "1 se l'ultima verifica di restore e' riuscita, 0 altrimenti.",
        &metrics,
        |m| m.last_verify_success.map(|v| if v { 1.0 } else { 0.0 }),
    )?;
    Ok(out)
}

fn job_metrics(job: &str, store: &HistoryStore) -> Result<JobMetrics, BackupError> {
    let latest = store.latest_for_job(job)?;
    let verification = store.latest_verification_for_job(job)?;
    Ok(JobMetrics {
        job: job.to_string(),
        last_run_timestamp: latest.as_ref().map(|r| r.started_at),
        last_run_success: latest.as_ref().map(is_success),
        last_run_duration_ms: latest.as_ref().and_then(|r| r.duration_ms),
        last_run_bytes: latest.as_ref().and_then(|r| r.bytes),
        last_verify_timestamp: verification.as_ref().map(|r| r.started_at),
        last_verify_success: verification.as_ref().map(is_verify_success),
    })
}

fn is_success(record: &ExecutionRecord) -> bool {
    record.status == "success"
}

fn is_verify_success(record: &ExecutionRecord) -> bool {
    record.verification_status == "success"
}

fn write_gauge(
    out: &mut String,
    name: &str,
    help: &str,
    metrics: &[JobMetrics],
    value_of: impl Fn(&JobMetrics) -> Option<f64>,
) -> Result<(), BackupError> {
    let values: Vec<(&str, f64)> = metrics
        .iter()
        .filter_map(|m| value_of(m).map(|v| (m.job.as_str(), v)))
        .collect();
    if values.is_empty() {
        return Ok(());
    }
    writeln!(out, "# HELP {name} {help}")
        .and_then(|_| writeln!(out, "# TYPE {name} gauge"))
        .map_err(|error| BackupError::Other(format!("generazione metriche fallita: {error}")))?;
    for (job, value) in values {
        writeln!(out, "{name}{{job=\"{}\"}} {value}", escape_label(job)).map_err(|error| {
            BackupError::Other(format!("generazione metriche fallita: {error}"))
        })?;
    }
    Ok(())
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::config::{Config, EngineKind, JobConfig, SourceConfig};
    use std::collections::HashMap;

    fn job(path: &str) -> JobConfig {
        JobConfig {
            source: SourceConfig::Folder {
                path: path.to_string(),
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

    #[test]
    fn renders_a_job_with_history_and_omits_one_without() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = HistoryStore::open(temp.path().join("history.sqlite")).expect("store");
        let id = store.start("documents", "backup", 100).expect("start");
        store
            .finish(
                id,
                &crate::history::ExecutionCompletion {
                    status: "success",
                    finished_at: 102,
                    duration_ms: 1500,
                    bytes: Some(42),
                    error: None,
                    verification_status: "success",
                    log: "ok\n",
                },
            )
            .expect("finish");

        let mut jobs = HashMap::new();
        jobs.insert("documents".to_string(), job("/srv/documents"));
        jobs.insert("never-run".to_string(), job("/srv/other"));
        let config = Config {
            destinations: HashMap::new(),
            notifiers: HashMap::new(),
            jobs,
            reports: Vec::new(),
            observability: None,
            api: Default::default(),
            updates: None,
        };

        let text = render(&config, &store).expect("render");
        assert!(text.contains("backuppo_last_run_timestamp_seconds{job=\"documents\"} 100"));
        assert!(text.contains("backuppo_last_run_success{job=\"documents\"} 1"));
        assert!(text.contains("backuppo_last_run_duration_ms{job=\"documents\"} 1500"));
        assert!(text.contains("backuppo_last_run_bytes{job=\"documents\"} 42"));
        assert!(text.contains("backuppo_last_verify_success{job=\"documents\"} 1"));
        assert!(!text.contains("never-run"));
    }
}
