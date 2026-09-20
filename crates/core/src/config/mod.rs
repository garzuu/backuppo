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
        /// Autenticazione a password: almeno uno tra `password_env` e
        /// `key_path` è obbligatorio (validato in `validate`).
        #[serde(default)]
        password_env: Option<String>,
        /// Percorso locale della chiave privata (autenticazione a chiave).
        #[serde(default)]
        key_path: Option<String>,
        /// Passphrase della chiave privata, se cifrata.
        #[serde(default)]
        key_passphrase_env: Option<String>,
        root: String,
        /// Fingerprint SHA256 attesa della host key (formato OpenSSH,
        /// `SHA256:...`). Se assente, la host key viene accettata senza
        /// verifica (loggando un avviso): consigliato impostarla in
        /// produzione per evitare attacchi man-in-the-middle.
        #[serde(default)]
        host_key_fingerprint: Option<String>,
        #[serde(default)]
        retry: RetryConfig,
        /// Limite di banda opzionale in KiB/s per upload/download
        /// (best-effort: alcuni backend possono superarlo su file piccoli).
        #[serde(default)]
        bandwidth_limit_kib_s: Option<u64>,
    },
    S3 {
        bucket: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
        access_key_id_env: String,
        secret_access_key_env: String,
        /// Prefisso delle chiavi dentro il bucket (default: radice).
        #[serde(default)]
        root: Option<String>,
        /// Stile "virtual-hosted" (`bucket.endpoint`) invece del default
        /// "path-style" (`endpoint/bucket`, usato da MinIO e dalla maggior
        /// parte dei provider self-hosted). AWS S3 supporta entrambi.
        #[serde(default)]
        virtual_host_style: bool,
        #[serde(default)]
        retry: RetryConfig,
        #[serde(default)]
        bandwidth_limit_kib_s: Option<u64>,
    },
    Webdav {
        url: String,
        #[serde(default)]
        user: Option<String>,
        #[serde(default)]
        password_env: Option<String>,
        #[serde(default)]
        retry: RetryConfig,
        #[serde(default)]
        bandwidth_limit_kib_s: Option<u64>,
    },
}

fn default_sftp_port() -> u16 {
    22
}

/// Retry con backoff esponenziale sulle operazioni di rete verso una
/// destination remota.
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_max_times")]
    pub max_times: usize,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_times: default_retry_max_times(),
        }
    }
}

fn default_retry_max_times() -> usize {
    3
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
    /// considerato troppo vecchio: `bkpo verify` segnala un allarme
    /// (non bloccante) se superata.
    #[serde(default)]
    pub max_backup_age_hours: Option<u64>,
    #[serde(default)]
    pub retention: Retention,
    #[serde(default)]
    pub notify: NotifyConfig,
}
