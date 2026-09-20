use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use backupper_core::config::{validate, Config};
use backupper_core::model::JobEvent;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "backupper",
    version,
    about = "Backup tool cross-platform con verifica automatica del restore"
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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Check { config } => check(&config),
        Command::Run { config, job } => run(&config, &job).await,
        Command::Verify { config, job } => verify(&config, &job).await,
    }
}

fn load_config(path: &PathBuf) -> Result<Config> {
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

    match backupper_engine::run_job(job_name, &config).await {
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

    match backupper_engine::verify_job(job_name, &config).await {
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
