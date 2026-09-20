use std::collections::HashSet;

use backuppo_core::config::Config;
use backuppo_core::error::BackupError;
use chrono::{DateTime, Datelike, Utc};
use tracing::{info, instrument};

use crate::naming::extract_timestamp;

/// Esito dell'applicazione della retention: quanti backup sono stati
/// mantenuti e quanti cancellati.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetentionSummary {
    pub kept: usize,
    pub deleted: usize,
}

/// Applica la policy di retention (`daily`/`weekly`/`monthly`) del job,
/// cancellando dalla destination i backup non più necessari.
///
/// Sicurezza: se il job non ha alcuna policy configurata, o ha al più un
/// backup disponibile, non cancella nulla; il backup più recente non viene
/// mai cancellato in nessun caso.
#[instrument(skip(config), fields(job = job_name))]
pub async fn apply(job_name: &str, config: &Config) -> Result<RetentionSummary, BackupError> {
    let job = config
        .jobs
        .get(job_name)
        .ok_or_else(|| BackupError::Other(format!("job '{job_name}' non trovato in config")))?;

    let retention = &job.retention;
    if retention.daily.is_none() && retention.weekly.is_none() && retention.monthly.is_none() {
        return Ok(RetentionSummary::default());
    }

    let dest_config = config.destinations.get(&job.destination).ok_or_else(|| {
        BackupError::Other(format!(
            "destination '{}' non trovata in config (job '{job_name}')",
            job.destination
        ))
    })?;
    let destination = backuppo_destinations::build(dest_config)?;

    let prefix = format!("{job_name}-");
    let mut backups: Vec<(String, u64)> = destination
        .list()
        .await?
        .into_iter()
        .filter(|n| n.starts_with(&prefix))
        .filter_map(|n| extract_timestamp(&n).map(|ts| (n, ts)))
        .collect();

    if backups.len() <= 1 {
        return Ok(RetentionSummary {
            kept: backups.len(),
            deleted: 0,
        });
    }

    // Più recente prima: le funzioni di bucketing assumono questo ordine.
    backups.sort_by_key(|(_, ts)| std::cmp::Reverse(*ts));

    let mut keep: HashSet<String> = HashSet::new();
    // Non cancellare mai il backup più recente, qualunque sia la policy.
    keep.insert(backups[0].0.clone());

    if let Some(n) = retention.daily {
        keep_latest_per_bucket(&backups, n as usize, &mut keep, day_bucket);
    }
    if let Some(n) = retention.weekly {
        keep_latest_per_bucket(&backups, n as usize, &mut keep, week_bucket);
    }
    if let Some(n) = retention.monthly {
        keep_latest_per_bucket(&backups, n as usize, &mut keep, month_bucket);
    }

    let mut summary = RetentionSummary::default();
    for (name, _) in &backups {
        if keep.contains(name) {
            summary.kept += 1;
        } else {
            destination.delete(name).await?;
            summary.deleted += 1;
            info!(backup = %name, "backup scaduto rimosso dalla retention");
        }
    }

    Ok(summary)
}

/// Tiene, per ognuno dei più recenti `max_buckets` bucket distinti (giorno,
/// settimana ISO o mese, a seconda di `bucket_of`), il backup più recente al
/// loro interno. `backups` deve già essere ordinato dal più recente al più
/// vecchio.
fn keep_latest_per_bucket(
    backups: &[(String, u64)],
    max_buckets: usize,
    keep: &mut HashSet<String>,
    bucket_of: impl Fn(u64) -> (i32, u32),
) {
    if max_buckets == 0 {
        return;
    }
    let mut seen_buckets: HashSet<(i32, u32)> = HashSet::new();
    for (name, ts) in backups {
        let bucket = bucket_of(*ts);
        if seen_buckets.contains(&bucket) {
            continue;
        }
        if seen_buckets.len() >= max_buckets {
            break;
        }
        seen_buckets.insert(bucket);
        keep.insert(name.clone());
    }
}

fn day_bucket(ts: u64) -> (i32, u32) {
    let dt = to_datetime(ts);
    (dt.year(), dt.ordinal())
}

fn week_bucket(ts: u64) -> (i32, u32) {
    let iso = to_datetime(ts).iso_week();
    (iso.year(), iso.week())
}

fn month_bucket(ts: u64) -> (i32, u32) {
    let dt = to_datetime(ts);
    (dt.year(), dt.month())
}

fn to_datetime(ts: u64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts as i64, 0).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 3600;

    #[test]
    fn keeps_latest_per_day_bucket() {
        // 5 backup, uno al giorno, in ordine dal più recente al più vecchio.
        let base = 1_700_000_000u64; // un istante arbitrario ma fisso
        let backups: Vec<(String, u64)> =
            (0..5).map(|i| (format!("b{i}"), base - i * DAY)).collect();

        let mut keep = HashSet::new();
        keep.insert(backups[0].0.clone());
        keep_latest_per_bucket(&backups, 2, &mut keep, day_bucket);

        assert_eq!(keep.len(), 2, "solo 2 bucket giornalieri richiesti");
        assert!(keep.contains("b0"));
        assert!(keep.contains("b1"));
    }

    #[test]
    fn multiple_backups_same_day_count_as_one_bucket() {
        let base = 1_700_000_000u64;
        // Ordinati dal più recente al più vecchio, come richiede la funzione.
        let backups = vec![
            ("evening".to_string(), base + 3600),
            ("morning".to_string(), base),
            ("yesterday".to_string(), base - DAY),
        ];

        let mut keep = HashSet::new();
        keep_latest_per_bucket(&backups, 1, &mut keep, day_bucket);

        // "evening" e "morning" sono nello stesso giorno: solo il più
        // recente dei due ("evening", primo nell'ordine di iterazione) viene
        // mantenuto per quel bucket.
        assert_eq!(keep.len(), 1);
        assert!(keep.contains("evening"));
    }
}
