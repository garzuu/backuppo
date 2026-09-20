use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use backupper_core::config::{validate, Config};
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { config } => check(&config),
    }
}

fn check(path: &PathBuf) -> Result<()> {
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

    println!(
        "'{}' è valido: {} destination, {} notifier, {} job.",
        path.display(),
        config.destinations.len(),
        config.notifiers.len(),
        config.jobs.len()
    );
    Ok(())
}
