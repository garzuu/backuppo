use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use backuppo_core::config::NotifierConfig;
use serde::Deserialize;

/// Config YAML dell'hub, indipendente dalla config dell'agent: nessun crate
/// dell'agent importa codice dell'hub e viceversa, condividono solo i tipi
/// in `backuppo-core` dietro la feature `hub`.
#[derive(Debug, Deserialize)]
pub struct HubConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    pub database_path: PathBuf,
    /// Variabile d'ambiente da cui leggere il segreto per firmare i JWT.
    pub jwt_secret_env: String,
    #[serde(default = "default_access_token_minutes")]
    pub access_token_minutes: i64,
    #[serde(default = "default_refresh_token_days")]
    pub refresh_token_days: i64,
    /// Un sito senza heartbeat da più di questi minuti viene marcato offline.
    #[serde(default = "default_offline_after_minutes")]
    pub offline_after_minutes: i64,
    /// Notifier riusati da `backuppo-notifiers` per gli alert dell'hub
    /// stesso (es. sito offline). Stesso formato della config agent.
    #[serde(default)]
    pub notifiers: HashMap<String, NotifierConfig>,
    #[serde(default)]
    pub notify_on_offline: Vec<String>,
    /// Notifica un job/verifica fallita non appena riportata da un agent.
    #[serde(default)]
    pub notify_on_failure: Vec<String>,
}

fn default_bind() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_access_token_minutes() -> i64 {
    15
}

fn default_refresh_token_days() -> i64 {
    30
}

fn default_offline_after_minutes() -> i64 {
    15
}

pub fn load(path: &Path) -> Result<HubConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("lettura config hub '{}'", path.display()))?;
    let config: HubConfig = serde_yaml::from_str(&raw).context("parsing config hub")?;
    Ok(config)
}
