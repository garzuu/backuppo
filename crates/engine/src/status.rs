use std::fmt::Write;
use std::path::Path;

use backuppo_core::config::Config;
use backuppo_core::error::BackupError;

use crate::history::{ExecutionRecord, HistoryStore};

/// Rigenera la pagina HTML statica configurata usando l'ultima esecuzione e
/// l'ultima verifica note per ciascun job.
pub fn generate(config: &Config) -> Result<(), BackupError> {
    let Some(settings) = &config.observability else {
        return Ok(());
    };
    let Some(output_path) = &settings.status_page else {
        return Ok(());
    };

    let store = HistoryStore::open(&settings.history_path)?;
    let mut jobs: Vec<&String> = config.jobs.keys().collect();
    jobs.sort();

    let mut rows = String::new();
    for job in jobs {
        let latest = store.latest_for_job(job)?;
        let verification = store.latest_verification_for_job(job)?;
        write_job_row(&mut rows, job, latest.as_ref(), verification.as_ref())?;
    }

    let generated_at = chrono::Utc::now().to_rfc3339();
    let html = format!(
        "<!doctype html>\n<html lang=\"it\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Backuppo — Stato</title><style>{STYLE}</style></head>\
         <body><main><h1>Backuppo</h1><p class=\"updated\">Aggiornato: {generated_at}</p>\
         <table><thead><tr><th>Job</th><th>Stato</th><th>Ultima esecuzione</th>\
         <th>Durata</th><th>Byte</th><th>Ultima verifica</th><th>Errore</th></tr></thead>\
         <tbody>{rows}</tbody></table></main></body></html>"
    );
    let path = Path::new(output_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, html)?;
    Ok(())
}

fn write_job_row(
    rows: &mut String,
    job: &str,
    latest: Option<&ExecutionRecord>,
    verification: Option<&ExecutionRecord>,
) -> Result<(), BackupError> {
    let status = latest
        .map(|record| record.status.as_str())
        .unwrap_or("never");
    let status_label = match status {
        "success" => "OK",
        "failure" => "Errore",
        "running" => "In corso",
        _ => "Mai eseguito",
    };
    let started = latest
        .map(|record| format_timestamp(record.started_at))
        .unwrap_or_else(|| "—".to_string());
    let duration = latest
        .and_then(|record| record.duration_ms)
        .map(|value| format!("{value} ms"))
        .unwrap_or_else(|| "—".to_string());
    let bytes = latest
        .and_then(|record| record.bytes)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".to_string());
    let verified = verification
        .map(|record| {
            format!(
                "{} ({})",
                format_timestamp(record.started_at),
                record.verification_status
            )
        })
        .unwrap_or_else(|| "—".to_string());
    let error = latest
        .and_then(|record| record.error.as_deref())
        .unwrap_or("—");

    write!(
        rows,
        "<tr><td>{}</td><td><span class=\"status {}\">{}</span></td>\
         <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
        escape_html(job),
        escape_html(status),
        status_label,
        started,
        duration,
        bytes,
        escape_html(&verified),
        escape_html(error)
    )
    .map_err(|error| BackupError::Other(format!("generazione HTML fallita: {error}")))
}

fn format_timestamp(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = r#"
:root { color-scheme: light dark; font-family: system-ui, sans-serif; }
body { margin: 0; background: #111827; color: #e5e7eb; }
main { max-width: 1100px; margin: 3rem auto; padding: 0 1rem; }
h1 { margin-bottom: .25rem; }
.updated { color: #9ca3af; margin-top: 0; }
table { width: 100%; border-collapse: collapse; background: #1f2937; }
th, td { padding: .75rem; border-bottom: 1px solid #374151; text-align: left; }
th { color: #9ca3af; }
.status { font-weight: 700; }
.status.success { color: #34d399; }
.status.failure { color: #f87171; }
.status.running { color: #fbbf24; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_user_controlled_html() {
        assert_eq!(escape_html("<job & 'x'>"), "&lt;job &amp; &#39;x&#39;&gt;");
    }
}
