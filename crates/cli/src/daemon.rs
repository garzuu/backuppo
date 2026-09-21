use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use anyhow::{Context, Result};
use backuppo_core::config::Config;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};

/// Avvia il daemon: registra ogni job configurato sullo scheduler cron-like
/// e resta in esecuzione finché non arriva un segnale di arresto
/// (SIGTERM o Ctrl+C), poi effettua uno shutdown pulito.
pub async fn run(config: Config) -> Result<()> {
    run_until(config, wait_for_shutdown_signal()).await
}

pub(crate) async fn run_until<F>(config: Config, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
    let config = Arc::new(config);
    // Un lock per job: una nuova esecuzione schedulata viene saltata se la
    // precedente per lo stesso job non è ancora terminata.
    let locks: Arc<HashMap<String, Arc<Mutex<()>>>> = Arc::new(
        config
            .jobs
            .keys()
            .map(|name| (name.clone(), Arc::new(Mutex::new(()))))
            .collect(),
    );
    let api_handle = crate::api::spawn(Arc::clone(&config), Arc::clone(&locks)).await?;
    #[cfg(feature = "hub")]
    let hub_handle = spawn_hub_tasks(&config);

    let scheduler = JobScheduler::new()
        .await
        .context("impossibile creare lo scheduler")?;

    for (name, job) in &config.jobs {
        // Il crate usato per lo schedule richiede il campo dei secondi;
        // `job.schedule` è cron standard a 5 campi (già validato in
        // `check`), quindi lo fissiamo sempre a 0.
        let cron_expr = format!("0 {}", job.schedule);
        let job_name = name.clone();
        let config = Arc::clone(&config);
        let locks = Arc::clone(&locks);

        let scheduled = Job::new_async(cron_expr.as_str(), move |_uuid, _scheduler| {
            let job_name = job_name.clone();
            let config = Arc::clone(&config);
            let locks = Arc::clone(&locks);
            Box::pin(async move {
                run_one_tick(job_name, config, locks).await;
            })
        })
        .with_context(|| format!("schedule non valido per il job '{name}': '{cron_expr}'"))?;

        scheduler
            .add(scheduled)
            .await
            .with_context(|| format!("impossibile registrare il job '{name}' nello scheduler"))?;
    }

    for (index, report) in config.reports.iter().enumerate() {
        let cron_expr = format!("0 {}", report.schedule);
        let days = report.days;
        let notifiers = report.notifiers.clone();
        let config = Arc::clone(&config);
        let report_number = index + 1;
        let scheduled = Job::new_async(cron_expr.as_str(), move |_uuid, _scheduler| {
            let config = Arc::clone(&config);
            let notifiers = notifiers.clone();
            Box::pin(async move {
                if let Err(error) = backuppo_engine::report::send(&config, days, &notifiers).await {
                    warn!(report = report_number, %error, "generazione report fallita");
                }
            })
        })
        .with_context(|| {
            format!("schedule non valido per il report #{report_number}: '{cron_expr}'")
        })?;
        scheduler.add(scheduled).await.with_context(|| {
            format!("impossibile registrare il report #{report_number} nello scheduler")
        })?;
    }

    scheduler
        .start()
        .await
        .context("impossibile avviare lo scheduler")?;
    info!(
        jobs = config.jobs.len(),
        reports = config.reports.len(),
        "daemon avviato"
    );

    shutdown.await;
    info!("segnale di arresto ricevuto, shutdown in corso");

    let mut scheduler = scheduler;
    scheduler
        .shutdown()
        .await
        .context("errore durante lo shutdown dello scheduler")?;
    if let Some(handle) = api_handle {
        handle.abort();
    }
    #[cfg(feature = "hub")]
    if let Some(handle) = hub_handle {
        handle.abort();
    }
    info!("shutdown completato");

    Ok(())
}

/// Avvia, se in config è presente un notifier di tipo `hub`, un task che
/// manda heartbeat periodici e ritenta l'invio degli eventi rimasti in coda
/// locale. Nessun errore verso l'hub deve mai arrestare il daemon o far
/// fallire un job: viene solo loggato.
#[cfg(feature = "hub")]
fn spawn_hub_tasks(config: &Config) -> Option<tokio::task::JoinHandle<()>> {
    use backuppo_core::config::NotifierConfig;

    let (url, token_env, heartbeat_seconds, queue_path) =
        config
            .notifiers
            .values()
            .find_map(|notifier| match notifier {
                NotifierConfig::Hub {
                    url,
                    token_env,
                    heartbeat_seconds,
                    queue_path,
                } => Some((
                    url.clone(),
                    token_env.clone(),
                    *heartbeat_seconds,
                    queue_path.clone(),
                )),
                _ => None,
            })?;

    Some(tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(heartbeat_seconds.max(1)));
        loop {
            interval.tick().await;
            if let Err(error) = backuppo_notifiers::hub::send_heartbeat(&url, &token_env).await {
                warn!(%error, "heartbeat verso l'hub fallito");
            }
            match backuppo_notifiers::hub::flush_queue(&url, &token_env, &queue_path).await {
                Ok(0) => {}
                Ok(count) => info!(count, "eventi in coda consegnati all'hub"),
                Err(error) => warn!(%error, "flush della coda eventi hub fallito"),
            }
        }
    }))
}

async fn run_one_tick(
    job_name: String,
    config: Arc<Config>,
    locks: Arc<HashMap<String, Arc<Mutex<()>>>>,
) {
    let Some(lock) = locks.get(&job_name) else {
        warn!(job = %job_name, "job non trovato tra i lock registrati, salto il giro");
        return;
    };

    let Ok(_guard) = lock.try_lock() else {
        warn!(job = %job_name, "esecuzione precedente ancora in corso, salto questo giro");
        return;
    };

    // `run_and_retain` applica la retention solo se il backup ha successo,
    // e logga già gli errori di job/retention: qui non c'è altro da fare.
    let _ = backuppo_engine::run_and_retain(&job_name, &config).await;
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "impossibile registrare il gestore SIGTERM, resto in ascolto solo di Ctrl+C");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::config::{
        Compression, DestinationConfig, JobConfig, NotifyConfig, Retention, SourceConfig,
    };
    use std::fs;

    fn write_file(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn build_config(src: &std::path::Path, dst: &std::path::Path) -> Config {
        let mut destinations = HashMap::new();
        destinations.insert(
            "local".to_string(),
            DestinationConfig::Fs {
                root: dst.to_string_lossy().to_string(),
            },
        );

        let mut jobs = HashMap::new();
        jobs.insert(
            "documents".to_string(),
            JobConfig {
                engine: Default::default(),
                source: SourceConfig::Folder {
                    path: src.to_string_lossy().to_string(),
                    exclude: vec![],
                },
                destination: "local".to_string(),
                compression: Some(Compression::None),
                encryption: None,
                schedule: "0 3 * * *".to_string(),
                verify_restore: None,
                max_backup_age_hours: None,
                retention: Retention::default(),
                notify: NotifyConfig::default(),
                pre: Vec::new(),
                post: Vec::new(),
            },
        );

        Config {
            destinations,
            notifiers: HashMap::new(),
            jobs,
            observability: None,
            reports: Vec::new(),
            api: None,
        }
    }

    fn build_locks(config: &Config) -> Arc<HashMap<String, Arc<Mutex<()>>>> {
        Arc::new(
            config
                .jobs
                .keys()
                .map(|name| (name.clone(), Arc::new(Mutex::new(()))))
                .collect(),
        )
    }

    #[tokio::test]
    async fn skips_the_tick_when_the_previous_run_still_holds_the_lock() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        write_file(&src_dir.path().join("a.txt"), "contenuto di test");

        let config = Arc::new(build_config(src_dir.path(), dst_dir.path()));
        let locks = build_locks(&config);

        // Simula un'esecuzione già in corso per "documents".
        let lock = Arc::clone(locks.get("documents").unwrap());
        let held_guard = lock.lock().await;

        run_one_tick(
            "documents".to_string(),
            Arc::clone(&config),
            Arc::clone(&locks),
        )
        .await;

        assert_eq!(
            fs::read_dir(dst_dir.path()).unwrap().count(),
            0,
            "con il lock occupato non deve partire alcun backup"
        );

        drop(held_guard);

        run_one_tick("documents".to_string(), config, locks).await;
        assert_eq!(
            fs::read_dir(dst_dir.path()).unwrap().count(),
            1,
            "una volta libero il lock il job deve poter partire"
        );
    }
}
