mod api;
mod auth;
mod config;
mod db;
mod offline;
mod policy;
mod web;
#[cfg(windows)]
mod windows_service;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use api::AppState;
use db::{Db, Role};

#[derive(Parser)]
#[command(name = "backuppo-hub", version, about = "Hub multi-sito per Backuppo")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Valida la configurazione senza avviare il servizio.
    Check {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
    /// Avvia l'hub: API `/v1`, Web UI e controllo heartbeat.
    Serve {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
    /// Crea un utente per il login alla Web UI/API (admin o read_only).
    CreateUser {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long)]
        username: String,
        /// Variabile d'ambiente da cui leggere la password in chiaro.
        #[arg(long)]
        password_env: String,
        #[arg(long, default_value = "admin")]
        role: String,
    },
    /// Avvia Backuppo Hub sotto Windows Service Control Manager.
    #[cfg(windows)]
    Service {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Check { config } => {
            config::load(&config)?;
            println!("'{}' è una configurazione Hub valida.", config.display());
            Ok(())
        }
        Command::Serve { config } => serve(&config).await,
        Command::CreateUser {
            config,
            username,
            password_env,
            role,
        } => create_user(&config, &username, &password_env, &role),
        #[cfg(windows)]
        Command::Service { config } => windows_service::run(config),
    }
}

async fn serve(config_path: &Path) -> Result<()> {
    serve_until(config_path, std::future::pending()).await
}

async fn serve_until<F>(config_path: &Path, shutdown: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let config = config::load(config_path)?;
    let db = Db::open(&config.database_path)?;
    let jwt_secret = std::env::var(&config.jwt_secret_env).with_context(|| {
        format!(
            "variabile d'ambiente '{}' (jwt_secret_env) non impostata",
            config.jwt_secret_env
        )
    })?;

    let state = AppState {
        db,
        jwt_secret: Arc::new(jwt_secret),
        access_token_minutes: config.access_token_minutes,
        refresh_token_days: config.refresh_token_days,
        offline_after_minutes: config.offline_after_minutes,
        notifiers: Arc::new(config.notifiers),
        notify_on_offline: Arc::new(config.notify_on_offline),
        notify_on_failure: Arc::new(config.notify_on_failure),
        browser_csrf: Arc::new(auth::generate_opaque_token().0),
        policy_signer: config
            .policy_signing_key_env
            .as_deref()
            .map(policy::load_signing_key)
            .transpose()?
            .map(Arc::new),
    };

    tokio::spawn(offline::run(state.clone()));

    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("impossibile aprire l'hub su {}", config.bind))?;
    tracing::info!(bind = %config.bind, "hub avviato");
    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(shutdown)
        .await
        .context("server hub terminato con errore")?;
    Ok(())
}

fn create_user(config_path: &Path, username: &str, password_env: &str, role: &str) -> Result<()> {
    let config = config::load(config_path)?;
    let db = Db::open(&config.database_path)?;
    let role = Role::parse(role).with_context(|| {
        format!("ruolo non valido '{role}': usare 'admin', 'operator' o 'read_only'")
    })?;
    let password = std::env::var(password_env)
        .with_context(|| format!("variabile d'ambiente '{password_env}' non impostata"))?;
    let hash = auth::hash_password(&password)?;
    let id = db.create_user(username, &hash, role)?;
    println!(
        "utente '{username}' creato (id {id}, ruolo {})",
        role.as_str()
    );
    Ok(())
}
