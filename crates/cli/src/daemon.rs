use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use backuppo_core::config::Config;
use tokio::sync::watch;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};

/// Avvia il daemon: registra ogni job configurato sullo scheduler cron-like
/// e resta in esecuzione finché non arriva un segnale di arresto
/// (SIGTERM o Ctrl+C), poi effettua uno shutdown pulito.
pub async fn run(config: Config, config_path: PathBuf) -> Result<()> {
    run_until_path(config, Some(config_path), wait_for_shutdown_signal()).await
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) async fn run_until<F>(config: Config, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
    run_until_path(config, None, shutdown).await
}

async fn run_until_path<F>(config: Config, config_path: Option<PathBuf>, shutdown: F) -> Result<()>
where
    F: Future<Output = ()>,
{
    let config = Arc::new(config);
    let locks = crate::api::build_locks(&config);
    let (reload_tx, mut reload_rx) = watch::channel(Arc::clone(&config));
    let runtime = crate::api::RuntimeState::new(
        Arc::clone(&config),
        config_path,
        Arc::clone(&locks),
        Some(reload_tx),
        crate::api::RuntimeMode::Daemon,
    );
    let api_handle = crate::api::spawn(runtime).await?;
    let mut config = config;
    #[cfg(feature = "hub")]
    let mut hub_handles = spawn_hub_tasks(Arc::clone(&config), Arc::clone(&locks));
    let mut scheduler = start_scheduler(Arc::clone(&config), Arc::clone(&locks)).await?;

    info!(
        jobs = config.jobs.len(),
        reports = config.reports.len(),
        "daemon avviato"
    );

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            changed = reload_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let next = reload_rx.borrow_and_update().clone();
                scheduler.shutdown().await.context("shutdown scheduler durante reload")?;
                #[cfg(feature = "hub")]
                for handle in hub_handles.drain(..) {
                    handle.abort();
                }
                scheduler = start_scheduler(Arc::clone(&next), Arc::clone(&locks)).await?;
                #[cfg(feature = "hub")]
                {
                    hub_handles = spawn_hub_tasks(Arc::clone(&next), Arc::clone(&locks));
                }
                config = next;
                info!(jobs = config.jobs.len(), reports = config.reports.len(), "configurazione applicata al daemon");
            }
        }
    }

    info!("segnale di arresto ricevuto, shutdown in corso");
    scheduler
        .shutdown()
        .await
        .context("errore durante lo shutdown dello scheduler")?;
    if let Some(handle) = api_handle {
        handle.abort();
    }
    #[cfg(feature = "hub")]
    for handle in hub_handles {
        handle.abort();
    }
    info!("shutdown completato");
    Ok(())
}

async fn start_scheduler(config: Arc<Config>, locks: crate::api::JobLocks) -> Result<JobScheduler> {
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
    Ok(scheduler)
}

/// Lock per job condivisi tra scheduler, API locale e comandi remoti.
#[cfg(feature = "hub")]
type Locks = crate::api::JobLocks;

/// Avvia, se in config è presente un notifier di tipo `hub`, un task che
/// manda heartbeat periodici e ritenta l'invio degli eventi rimasti in coda
/// locale e, solo con `remote_commands: true`, un secondo task che ritira ed
/// esegue i comandi "esegui ora"/"verifica ora". Nessun errore verso l'hub
/// deve mai arrestare il daemon o far fallire un job: viene solo loggato.
#[cfg(feature = "hub")]
fn spawn_hub_tasks(config: Arc<Config>, locks: Locks) -> Vec<tokio::task::JoinHandle<()>> {
    use backuppo_core::config::NotifierConfig;

    let Some((url, token_env, heartbeat_seconds, queue_path, remote_commands, poll_seconds)) =
        config
            .notifiers
            .values()
            .find_map(|notifier| match notifier {
                NotifierConfig::Hub {
                    url,
                    token_env,
                    heartbeat_seconds,
                    queue_path,
                    remote_commands,
                    command_poll_seconds,
                } => Some((
                    url.clone(),
                    token_env.clone(),
                    *heartbeat_seconds,
                    queue_path.clone(),
                    *remote_commands,
                    *command_poll_seconds,
                )),
                _ => None,
            })
    else {
        return Vec::new();
    };

    let mut handles = Vec::new();
    let (heartbeat_url, heartbeat_token_env) = (url.clone(), token_env.clone());
    handles.push(tokio::spawn(async move {
        let (url, token_env) = (heartbeat_url, heartbeat_token_env);
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
    }));

    if remote_commands {
        info!("comandi remoti dall'hub abilitati: solo job presenti in questa config");
        handles.push(tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(poll_seconds.max(1)));
            loop {
                interval.tick().await;
                let commands = match backuppo_notifiers::hub::fetch_commands(&url, &token_env).await
                {
                    Ok(commands) => commands,
                    Err(error) => {
                        warn!(%error, "ritiro comandi dall'hub fallito");
                        continue;
                    }
                };
                for command in commands {
                    // Un comando lungo (un backup) non deve bloccare il poll.
                    let (config, locks) = (Arc::clone(&config), Arc::clone(&locks));
                    let (url, token_env) = (url.clone(), token_env.clone());
                    tokio::spawn(async move {
                        info!(
                            id = command.id,
                            job = command.job.as_str(),
                            kind = command.kind.as_str(),
                            "comando remoto ricevuto dall'hub"
                        );
                        let result = execute_command(&command, &config, &locks).await;
                        if let Err(error) = backuppo_notifiers::hub::report_command(
                            &url, &token_env, command.id, &result,
                        )
                        .await
                        {
                            warn!(id = command.id, %error, "invio esito comando all'hub fallito");
                        }
                    });
                }
            }
        }));
    }
    handles
}

/// Esegue un comando remoto. Sicurezza: il comando può solo nominare un job
/// già definito nella config locale e scegliere tra le due azioni note
/// (`run`, `verify`); il lock per job evita esecuzioni sovrapposte.
#[cfg(feature = "hub")]
async fn execute_command(
    command: &backuppo_core::hub_protocol::PendingCommand,
    config: &Config,
    locks: &Locks,
) -> backuppo_core::hub_protocol::CommandResult {
    use backuppo_core::hub_protocol::{CommandKind, CommandResult};

    let failed = |detail: String| CommandResult {
        ok: false,
        detail: Some(detail),
    };
    let job = command.job.as_str();
    let Some(lock) = locks.read().expect("job locks poisoned").get(job).cloned() else {
        return failed(format!(
            "job '{job}' non presente nella config di questo agent"
        ));
    };
    let Ok(_guard) = lock.try_lock() else {
        return failed(format!("il job '{job}' è già in esecuzione"));
    };
    match command.kind {
        CommandKind::Run => match backuppo_engine::run_and_retain(job, config).await {
            Ok(artifact) => CommandResult {
                ok: true,
                detail: Some(format!(
                    "backup riuscito: {} file, {} byte",
                    artifact.files, artifact.bytes
                )),
            },
            Err(error) => failed(error.to_string()),
        },
        CommandKind::Verify => match backuppo_engine::verify_job(job, config).await {
            Ok(event) => CommandResult {
                ok: true,
                detail: Some(event.to_string()),
            },
            Err(error) => failed(error.to_string()),
        },
    }
}

async fn run_one_tick(job_name: String, config: Arc<Config>, locks: crate::api::JobLocks) {
    let Some(lock) = locks
        .read()
        .expect("job locks poisoned")
        .get(&job_name)
        .cloned()
    else {
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
        let mut destinations = std::collections::HashMap::new();
        destinations.insert(
            "local".to_string(),
            DestinationConfig::Fs {
                root: dst.to_string_lossy().to_string(),
            },
        );

        let mut jobs = std::collections::HashMap::new();
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
            notifiers: std::collections::HashMap::new(),
            jobs,
            observability: None,
            reports: Vec::new(),
            api: None,
        }
    }

    #[tokio::test]
    async fn skips_the_tick_when_the_previous_run_still_holds_the_lock() {
        let src_dir = tempfile::tempdir().unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        write_file(&src_dir.path().join("a.txt"), "contenuto di test");

        let config = Arc::new(build_config(src_dir.path(), dst_dir.path()));
        let locks = crate::api::build_locks(&config);

        // Simula un'esecuzione già in corso per "documents".
        let lock = locks.read().unwrap().get("documents").cloned().unwrap();
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

    #[cfg(feature = "hub")]
    fn locks_for(config: &Config) -> Locks {
        crate::api::build_locks(config)
    }

    #[cfg(feature = "hub")]
    fn command(
        kind: backuppo_core::hub_protocol::CommandKind,
        job: &str,
    ) -> backuppo_core::hub_protocol::PendingCommand {
        backuppo_core::hub_protocol::PendingCommand {
            id: 1,
            kind,
            job: job.to_string(),
        }
    }

    #[cfg(feature = "hub")]
    #[tokio::test]
    async fn remote_run_and_verify_execute_a_configured_job() {
        use backuppo_core::hub_protocol::CommandKind;
        let temp = tempfile::tempdir().unwrap();
        let (src, dst) = (temp.path().join("src"), temp.path().join("dst"));
        write_file(&src.join("a.txt"), "ciao");
        let config = build_config(&src, &dst);
        let locks = locks_for(&config);

        let run = execute_command(&command(CommandKind::Run, "documents"), &config, &locks).await;
        assert!(run.ok, "{run:?}");
        assert!(run.detail.unwrap().contains("1 file"));
        assert!(
            std::fs::read_dir(&dst).unwrap().next().is_some(),
            "archivio creato"
        );

        let verify =
            execute_command(&command(CommandKind::Verify, "documents"), &config, &locks).await;
        assert!(verify.ok, "{verify:?}");
    }

    #[cfg(feature = "hub")]
    #[tokio::test]
    async fn remote_command_for_an_unknown_job_is_refused() {
        use backuppo_core::hub_protocol::CommandKind;
        let temp = tempfile::tempdir().unwrap();
        let config = build_config(&temp.path().join("src"), &temp.path().join("dst"));
        let locks = locks_for(&config);

        let result = execute_command(
            &command(CommandKind::Run, "../../etc/passwd"),
            &config,
            &locks,
        )
        .await;
        assert!(!result.ok);
        assert!(result.detail.unwrap().contains("non presente"));
    }

    #[cfg(feature = "hub")]
    #[tokio::test]
    async fn remote_command_is_refused_while_the_job_is_running() {
        use backuppo_core::hub_protocol::CommandKind;
        let temp = tempfile::tempdir().unwrap();
        let (src, dst) = (temp.path().join("src"), temp.path().join("dst"));
        write_file(&src.join("a.txt"), "ciao");
        let config = build_config(&src, &dst);
        let locks = locks_for(&config);
        let lock = locks.read().unwrap().get("documents").cloned().unwrap();
        let _running = lock.lock().await;

        let result =
            execute_command(&command(CommandKind::Run, "documents"), &config, &locks).await;
        assert!(!result.ok);
        assert!(result.detail.unwrap().contains("già in esecuzione"));
        assert!(!dst.exists(), "nessun backup deve essere partito");
    }
}
