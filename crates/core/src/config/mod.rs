mod validate;

use std::collections::HashMap;

use serde::Deserialize;

pub use validate::{validate, ConfigError};

/// Configurazione completa letta da un file YAML.
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub destinations: HashMap<String, DestinationConfig>,
    #[serde(default)]
    pub notifiers: HashMap<String, NotifierConfig>,
    #[serde(default)]
    pub jobs: HashMap<String, JobConfig>,
}

impl Config {
    /// Effettua il solo parsing YAML, senza validazione dei riferimenti
    /// incrociati (destination/notifier/cron): usare [`validate`] dopo.
    pub fn from_yaml(yaml: &str) -> Result<Config, ConfigError> {
        Ok(serde_yaml::from_str(yaml)?)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DestinationConfig {
    Fs {
        root: String,
    },
    Sftp {
        host: String,
        #[serde(default = "default_sftp_port")]
        port: u16,
        user: String,
        password_env: String,
        root: String,
    },
    S3 {
        bucket: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
        access_key_id_env: String,
        secret_access_key_env: String,
    },
    Webdav {
        url: String,
        #[serde(default)]
        user: Option<String>,
        #[serde(default)]
        password_env: Option<String>,
    },
}

fn default_sftp_port() -> u16 {
    22
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotifierConfig {
    Telegram {
        token_env: String,
        chat_id: String,
    },
    Smtp {
        host: String,
        port: u16,
        user: String,
        password_env: String,
        from: String,
        to: Vec<String>,
    },
    Webhook {
        url: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    Folder {
        path: String,
        #[serde(default)]
        exclude: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    Zstd,
    None,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EncryptionConfig {
    Age {
        #[serde(default)]
        passphrase_env: Option<String>,
        #[serde(default)]
        key_env: Option<String>,
    },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyRestore {
    Never,
    Every,
    Daily,
    Weekly,
}

#[derive(Debug, Deserialize, Default)]
pub struct Retention {
    #[serde(default)]
    pub daily: Option<u32>,
    #[serde(default)]
    pub weekly: Option<u32>,
    #[serde(default)]
    pub monthly: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
pub struct NotifyConfig {
    #[serde(default)]
    pub on_success: Vec<String>,
    #[serde(default)]
    pub on_failure: Vec<String>,
    #[serde(default)]
    pub on_verify: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct JobConfig {
    pub source: SourceConfig,
    pub destination: String,
    #[serde(default)]
    pub compression: Option<Compression>,
    #[serde(default)]
    pub encryption: Option<EncryptionConfig>,
    pub schedule: String,
    #[serde(default)]
    pub verify_restore: Option<VerifyRestore>,
    /// Soglia (in ore) oltre la quale l'ultimo backup disponibile è
    /// considerato troppo vecchio: `backupper verify` segnala un allarme
    /// (non bloccante) se superata.
    #[serde(default)]
    pub max_backup_age_hours: Option<u64>,
    #[serde(default)]
    pub retention: Retention,
    #[serde(default)]
    pub notify: NotifyConfig,
}
