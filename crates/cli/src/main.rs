mod api;
mod daemon;
#[cfg(feature = "hub")]
mod policy;
mod storage_check;
mod updater;
#[cfg(windows)]
mod windows_service;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use backuppo_core::config::{validate, Config};
use backuppo_core::model::{JobEvent, OverwritePolicy, RestoreRequest};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "bkpo",
    version,
    about = "Backuppo: backup tool cross-platform con verifica automatica del restore"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Valida un file di configurazione YAML.
    Check {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
    /// Esegue un job di backup.
    Run {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
    },
    /// Verifica che l'ultimo backup di un job sia ripristinabile e integro.
    Verify {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
    },
    /// Verifica online le protezioni dello storage configurate per un job.
    StorageCheck {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
    },
    /// Applica retention/prune Restic con credenziali amministrative separate.
    Maintain {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
    },
    /// Elenca gli snapshot disponibili per un job.
    Snapshots {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
    },
    /// Elenca file e directory contenuti in uno snapshot.
    Browse {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
        #[arg(long, default_value = "latest")]
        snapshot: String,
    },
    /// Ripristina uno snapshot in una directory locale.
    Restore {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
        #[arg(long, default_value = "latest")]
        snapshot: String,
        #[arg(long)]
        target: PathBuf,
        #[arg(long = "include")]
        include: Vec<String>,
        #[arg(long)]
        dry_run: bool,
        /// Autorizza la scrittura in una directory non vuota.
        #[arg(long)]
        overwrite: bool,
    },
    /// Genera o ruota chiavi di cifratura.
    Keys {
        #[command(subcommand)]
        action: KeysAction,
    },
    /// Controlla, scarica o applica aggiornamenti firmati.
    Update {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[command(subcommand)]
        action: UpdateAction,
    },
    /// Invia un messaggio di prova a uno o tutti i notifier configurati.
    NotifyTest {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        /// Nome del notifier da provare; se omesso li prova tutti.
        #[arg(long)]
        notifier: Option<String>,
    },
    /// Avvia il daemon: esegue i job secondo il loro schedule cron e
    /// applica la retention dopo ogni backup riuscito.
    Daemon {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
    /// Avvia solo l'API e la Web UI locale.
    Serve {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
    /// Elenca le esecuzioni registrate nello storico locale.
    Runs {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Mostra il log associato a una singola esecuzione.
    Logs {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        id: i64,
    },
    /// Avvia Backuppo sotto Windows Service Control Manager.
    #[cfg(windows)]
    Service {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
}

#[derive(Subcommand)]
enum KeysAction {
    /// Genera una nuova identita' age X25519 per l'engine 'archive'
    /// (`encryption.type: age`). Non tocca alcuna configurazione: stampa
    /// solo la nuova identita' e il recipient pubblico.
    GenerateAge,
    /// Elenca le chiavi che proteggono il repository Restic di un job.
    ListRestic {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
    },
    /// Aggiunge una nuova password al repository Restic di un job, senza
    /// ricifrare i dati gia' scritti. La password attuale resta valida
    /// finche' non la rimuovi esplicitamente con 'remove-restic'.
    RotateRestic {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
        /// Nome della variabile d'ambiente che contiene la nuova password
        /// (non la password stessa).
        #[arg(long)]
        new_password_env: String,
    },
    /// Rimuove una chiave dal repository Restic di un job (vedi
    /// 'list-restic' per gli id). Restic rifiuta di rimuovere l'ultima
    /// chiave rimasta.
    RemoveRestic {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        job: String,
        #[arg(long)]
        key_id: String,
    },
}

#[derive(Subcommand)]
enum UpdateAction {
    Check,
    Download,
    Status,
    Apply,
    Rollback,
}

/// Configura i log. Senza `RUST_LOG` i servizi a lunga durata (`daemon`,
/// `serve`) loggano a livello `info`, altrimenti restano invisibili; i comandi
/// one-shot solo da `warn`, per non riempire l'output di chi li lancia a mano.
/// I log vanno su stderr (stdout è per i risultati) e i colori ANSI solo se
/// stderr è un terminale, così file di log e journald restano leggibili.
fn init_logging(command: &Command) {
    use std::io::IsTerminal;

    // `info` solo per i crate del workspace: le dipendenze restano a `warn`.
    const SERVICE_FILTER: &str =
        "warn,bkpo=info,backuppo=info,backuppo_core=info,backuppo_engine=info,\
        backuppo_sources=info,backuppo_destinations=info,backuppo_notifiers=info";
    let default_level = match command {
        Command::Daemon { .. } | Command::Serve { .. } => SERVICE_FILTER,
        #[cfg(windows)]
        Command::Service { .. } => SERVICE_FILTER,
        _ => "warn",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(&cli.command);
    match cli.command {
        Command::Check { config } => check(&config),
        Command::Run { config, job } => run(&config, &job).await,
        Command::Verify { config, job } => verify(&config, &job).await,
        Command::StorageCheck { config, job } => storage_check(&config, &job).await,
        Command::Maintain { config, job } => maintain(&config, &job).await,
        Command::Snapshots { config, job } => snapshots(&config, &job).await,
        Command::Browse {
            config,
            job,
            snapshot,
        } => browse(&config, &job, &snapshot).await,
        Command::Restore {
            config,
            job,
            snapshot,
            target,
            include,
            dry_run,
            overwrite,
        } => restore(&config, &job, snapshot, target, include, dry_run, overwrite).await,
        Command::Keys { action } => keys(action).await,
        Command::Update { config, action } => update(&config, action).await,
        Command::NotifyTest { config, notifier } => notify_test(&config, notifier.as_deref()).await,
        Command::Daemon { config } => daemon::run(load_config(&config)?, config).await,
        Command::Serve { config } => api::serve(load_config(&config)?, config).await,
        Command::Runs { config, limit } => runs(&config, limit),
        Command::Logs { config, id } => logs(&config, id),
        #[cfg(windows)]
        Command::Service { config } => windows_service::run(config),
    }
}

async fn storage_check(config_path: &PathBuf, job_name: &str) -> Result<()> {
    let config = load_config(config_path)?;
    let job = config
        .jobs
        .get(job_name)
        .with_context(|| format!("job '{job_name}' non trovato"))?;
    let destination = config
        .destinations
        .get(&job.destination)
        .with_context(|| format!("destination '{}' non trovata", job.destination))?;
    let object_lock = match destination {
        backuppo_core::config::DestinationConfig::Restic {
            object_lock: Some(settings),
            ..
        } => settings,
        backuppo_core::config::DestinationConfig::Restic { .. } => {
            bail!("job '{job_name}': object_lock non configurato")
        }
        _ => bail!("job '{job_name}': storage-check e' disponibile per repository Restic S3"),
    };
    let report = storage_check::verify_object_lock(object_lock).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn maintain(config_path: &PathBuf, job_name: &str) -> Result<()> {
    let config = load_config(config_path)?;
    backuppo_engine::maintain(job_name, &config).await?;
    println!("manutenzione Restic completata per '{job_name}'");
    Ok(())
}

async fn keys(action: KeysAction) -> Result<()> {
    match action {
        KeysAction::GenerateAge => {
            use age::secrecy::ExposeSecret;
            let identity = age::x25519::Identity::generate();
            let recipient = identity.to_public();
            println!("Nuova identita' age X25519. Non finisce da nessuna parte: salvala subito");
            println!("in un secret manager, non in questo terminale o in uno script.\n");
            println!(
                "Identita' (segreta, va nella variabile referenziata da 'encryption.key_env'):"
            );
            println!("  {}\n", identity.to_string().expose_secret());
            println!("Recipient pubblico (puoi condividerlo: cifra ma non decifra):");
            println!("  {recipient}\n");
            println!("Dopo averla salvata: aggiorna 'encryption.key_env' nella config e riavvia");
            println!("l'agent. Gli archivi gia' scritti restano cifrati con la vecchia chiave:");
            println!("conservala finche' non ripristini o non rifai da zero i backup esistenti.");
            Ok(())
        }
        KeysAction::ListRestic { config, job } => {
            let config = load_config(&config)?;
            let keys = backuppo_engine::list_restic_keys(&job, &config).await?;
            println!("ID\tUTENTE\tHOST\tCREATA\tCORRENTE");
            for key in keys {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    key.id, key.user_name, key.host_name, key.created, key.current
                );
            }
            Ok(())
        }
        KeysAction::RotateRestic {
            config,
            job,
            new_password_env,
        } => {
            let config = load_config(&config)?;
            let new_password =
                backuppo_core::secrets::resolve_env("new_password_env", &new_password_env)?;
            let key = backuppo_engine::rotate_restic_key(&job, &config, &new_password).await?;
            println!("nuova chiave aggiunta al repository: id {}", key.id);
            println!("la password attuale resta valida: non e' stato ricifrato nulla.");
            println!();
            println!("prossimi passi:");
            println!("  1. annota l'id della vecchia chiave con 'bkpo keys list-restic'");
            println!(
                "  2. aggiorna 'password_env' nella config con la variabile della nuova password"
            );
            println!(
                "  3. esegui 'bkpo verify --config ... --job {job}' per confermare che funzioni"
            );
            println!("  4. solo allora rimuovi la vecchia chiave:");
            println!("     bkpo keys remove-restic --config ... --job {job} --key-id <id-vecchia-chiave>");
            Ok(())
        }
        KeysAction::RemoveRestic {
            config,
            job,
            key_id,
        } => {
            let config = load_config(&config)?;
            backuppo_engine::remove_restic_key(&job, &config, &key_id).await?;
            println!("chiave '{key_id}' rimossa dal repository di '{job}'");
            Ok(())
        }
    }
}

async fn update(config_path: &PathBuf, action: UpdateAction) -> Result<()> {
    let config = load_config(config_path)?;
    let settings = config
        .updates
        .as_ref()
        .context("la configurazione non contiene la sezione 'updates'")?;
    match action {
        UpdateAction::Check => match updater::check(settings).await? {
            Some(checked) => println!(
                "aggiornamento disponibile: {} ({} byte, {})",
                checked.manifest.version, checked.artifact.size, checked.artifact.target
            ),
            None => println!("Backuppo e' aggiornato."),
        },
        UpdateAction::Download => {
            let checked = updater::check(settings)
                .await?
                .context("nessun aggiornamento disponibile")?;
            let path = updater::download(settings, &checked).await?;
            println!("aggiornamento verificato e salvato in '{}'", path.display());
        }
        UpdateAction::Status => {
            let status = updater::status(settings)?;
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        UpdateAction::Apply => {
            let backup = updater::apply(settings)?;
            println!(
                "aggiornamento installato; riavvia Backuppo. Versione precedente: '{}'",
                backup.display()
            );
        }
        UpdateAction::Rollback => {
            let replaced = updater::rollback(settings)?;
            println!(
                "rollback completato; riavvia Backuppo. Versione sostituita: '{}'",
                replaced.display()
            );
        }
    }
    Ok(())
}

fn history_store(config: &Config) -> Result<backuppo_engine::history::HistoryStore> {
    let settings = config
        .observability
        .as_ref()
        .context("la configurazione non contiene la sezione 'observability'")?;
    backuppo_engine::history::HistoryStore::open(&settings.history_path)
        .map_err(anyhow::Error::from)
}

fn runs(config_path: &PathBuf, limit: usize) -> Result<()> {
    let config = load_config(config_path)?;
    let records = history_store(&config)?.list(limit)?;
    if records.is_empty() {
        println!("nessuna esecuzione registrata.");
        return Ok(());
    }
    println!("ID\tINIZIO\tJOB\tTIPO\tSTATO\tDURATA\tBYTE\tVERIFICA");
    for record in records {
        let started = format_timestamp(record.started_at);
        let duration = record
            .duration_ms
            .map(|value| format!("{value}ms"))
            .unwrap_or_else(|| "-".to_string());
        let bytes = record
            .bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            record.id,
            started,
            record.job,
            record.kind,
            record.status,
            duration,
            bytes,
            record.verification_status
        );
    }
    Ok(())
}

fn logs(config_path: &PathBuf, id: i64) -> Result<()> {
    let config = load_config(config_path)?;
    let record = history_store(&config)?
        .get(id)?
        .with_context(|| format!("esecuzione #{id} non trovata"))?;
    print!("{}", record.log);
    Ok(())
}

fn format_timestamp(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

pub(crate) fn load_config(path: &PathBuf) -> Result<Config> {
    let yaml = std::fs::read_to_string(path).with_context(|| {
        format!(
            "impossibile leggere il file di configurazione '{}'",
            path.display()
        )
    })?;

    let config = Config::from_yaml(&yaml).map_err(|e| {
        anyhow::anyhow!(
            "'{}' non è un file di configurazione valido: {e}",
            path.display()
        )
    })?;

    if let Err(errors) = validate(&config) {
        eprintln!(
            "'{}' contiene {} errore/i di validazione:",
            path.display(),
            errors.len()
        );
        for error in &errors {
            eprintln!("  - {error}");
        }
        bail!("validazione fallita");
    }

    Ok(config)
}

fn check(path: &PathBuf) -> Result<()> {
    let config = load_config(path)?;
    println!(
        "'{}' è valido: {} destination, {} notifier, {} job.",
        path.display(),
        config.destinations.len(),
        config.notifiers.len(),
        config.jobs.len()
    );
    Ok(())
}

async fn run(config_path: &PathBuf, job_name: &str) -> Result<()> {
    let config = load_config(config_path)?;

    match backuppo_engine::run_job(job_name, &config).await {
        Ok(artifact) => {
            let event = JobEvent::Success {
                job: job_name.to_string(),
                artifact,
            };
            println!("{event}");
            Ok(())
        }
        Err(err) => {
            let event = JobEvent::Failure {
                job: job_name.to_string(),
                error: err.to_string(),
            };
            eprintln!("{event}");
            bail!("job fallito");
        }
    }
}

async fn verify(config_path: &PathBuf, job_name: &str) -> Result<()> {
    let config = load_config(config_path)?;

    match backuppo_engine::verify_job(job_name, &config).await {
        Ok(event) => {
            println!("{event}");
            Ok(())
        }
        Err(err) => {
            let event = JobEvent::Failure {
                job: job_name.to_string(),
                error: err.to_string(),
            };
            eprintln!("{event}");
            bail!("verifica fallita");
        }
    }
}

async fn snapshots(config_path: &PathBuf, job_name: &str) -> Result<()> {
    let config = load_config(config_path)?;
    let snapshots = backuppo_engine::snapshots(job_name, &config).await?;
    println!("CREATO\tMOTORE\tID");
    for snapshot in snapshots {
        println!(
            "{}\t{}\t{}",
            format_timestamp(snapshot.created_at),
            snapshot.engine,
            snapshot.id
        );
    }
    Ok(())
}

async fn browse(config_path: &PathBuf, job_name: &str, snapshot: &str) -> Result<()> {
    let config = load_config(config_path)?;
    let entries = backuppo_engine::browse(job_name, &config, snapshot).await?;
    println!("TIPO\tBYTE\tPERCORSO");
    for entry in entries {
        println!(
            "{}\t{}\t{}",
            entry.kind,
            entry
                .bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into()),
            entry.path
        );
    }
    Ok(())
}

async fn restore(
    config_path: &PathBuf,
    job_name: &str,
    snapshot: String,
    target: PathBuf,
    include: Vec<String>,
    dry_run: bool,
    overwrite: bool,
) -> Result<()> {
    let config = load_config(config_path)?;
    let result = backuppo_engine::restore(
        job_name,
        &config,
        &RestoreRequest {
            snapshot,
            target,
            include,
            dry_run,
            overwrite: if overwrite {
                OverwritePolicy::Always
            } else {
                OverwritePolicy::Never
            },
        },
    )
    .await?;
    println!(
        "{}: {} file, {} byte da '{}' verso '{}'",
        if result.dry_run {
            "dry-run"
        } else {
            "restore completato"
        },
        result.files,
        result.bytes,
        result.snapshot,
        result.target.display()
    );
    Ok(())
}

async fn notify_test(config_path: &PathBuf, notifier_name: Option<&str>) -> Result<()> {
    let config = load_config(config_path)?;

    let names: Vec<String> = match notifier_name {
        Some(name) => vec![name.to_string()],
        None => config.notifiers.keys().cloned().collect(),
    };

    if names.is_empty() {
        println!("nessun notifier configurato.");
        return Ok(());
    }

    let event = JobEvent::Report {
        summary: "backuppo: messaggio di prova (notify-test)".to_string(),
    };

    let mut any_failed = false;
    for name in &names {
        let Some(notifier_config) = config.notifiers.get(name) else {
            eprintln!("notifier '{name}': non trovato in config");
            any_failed = true;
            continue;
        };

        match backuppo_notifiers::build(notifier_config) {
            Ok(notifier) => match notifier.send(&event).await {
                Ok(()) => println!("notifier '{name}': OK"),
                Err(e) => {
                    eprintln!("notifier '{name}': invio fallito: {e}");
                    any_failed = true;
                }
            },
            Err(e) => {
                eprintln!("notifier '{name}': impossibile costruirlo: {e}");
                any_failed = true;
            }
        }
    }

    if any_failed {
        bail!("uno o più notifier hanno fallito il test");
    }
    Ok(())
}
