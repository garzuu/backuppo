mod api;
mod daemon;
#[cfg(windows)]
mod windows_service;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use backuppo_core::config::{validate, Config};
use backuppo_core::model::JobEvent;
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
        Command::NotifyTest { config, notifier } => notify_test(&config, notifier.as_deref()).await,
        Command::Daemon { config } => daemon::run(load_config(&config)?, config).await,
        Command::Serve { config } => api::serve(load_config(&config)?, config).await,
        Command::Runs { config, limit } => runs(&config, limit),
        Command::Logs { config, id } => logs(&config, id),
        #[cfg(windows)]
        Command::Service { config } => windows_service::run(config),
    }
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
