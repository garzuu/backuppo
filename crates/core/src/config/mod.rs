mod validate;

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use validate::{validate, ConfigError};

/// Configurazione completa letta da un file YAML.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    #[serde(default)]
    pub destinations: HashMap<String, DestinationConfig>,
    #[serde(default)]
    pub notifiers: HashMap<String, NotifierConfig>,
    #[serde(default)]
    pub jobs: HashMap<String, JobConfig>,
    /// Persistenza locale dello storico e pagina di stato. Se assente,
    /// l'osservabilità su disco è disabilitata.
    #[serde(default)]
    pub observability: Option<ObservabilityConfig>,
    /// Report periodici generati dallo storico locale e inviati ai notifier.
    #[serde(default)]
    pub reports: Vec<ReportConfig>,
    /// API HTTP e Web UI locale. Se assente, il server non viene avviato.
    #[serde(default)]
    pub api: Option<ApiConfig>,
    /// Aggiornamenti firmati. Se assente Backuppo non effettua richieste di
    /// rete per controllare nuove versioni.
    #[serde(default)]
    pub updates: Option<UpdateConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateConfig {
    pub manifest_url: String,
    /// Chiave pubblica Ed25519 raw (32 byte) codificata base64.
    pub public_key: String,
    /// Chiavi root aggiuntive utilizzabili per rotazione o recovery.
    #[serde(default)]
    pub recovery_public_keys: Vec<String>,
    #[serde(default)]
    pub channel: UpdateChannel,
    #[serde(default)]
    pub pinned_version: Option<String>,
    #[serde(default = "default_update_download_dir")]
    pub download_dir: String,
    #[serde(default)]
    pub install_mode: UpdateInstallMode,
    /// Scarica in staging una release valida; l'installazione resta
    /// subordinata ad approvazione/finestra di manutenzione.
    #[serde(default)]
    pub auto_download: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
    Pinned,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallMode {
    /// Scarica e segnala soltanto: adatto a pacchetti di sistema.
    #[default]
    Notify,
    /// Consente la sostituzione atomica di un binario standalone.
    Standalone,
    Package,
    Docker,
}

fn default_update_download_dir() -> String {
    "backuppo-updates".to_string()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApiConfig {
    /// Indirizzo di ascolto. Per impostazione predefinita deve essere loopback.
    #[serde(default = "default_api_bind")]
    pub bind: String,
    /// Consente esplicitamente un bind non-loopback, necessario in container.
    /// La UI non ha login: la porta deve comunque essere pubblicata soltanto
    /// su loopback o protetta da un controllo di accesso esterno.
    #[serde(default)]
    pub allow_remote: bool,
}

fn default_api_bind() -> String {
    "127.0.0.1:8787".to_string()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ObservabilityConfig {
    /// Database SQLite locale delle esecuzioni.
    pub history_path: String,
    /// File HTML statico rigenerato dopo ogni backup o verifica.
    #[serde(default)]
    pub status_page: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReportConfig {
    /// Schedule cron standard a cinque campi.
    pub schedule: String,
    /// Finestra temporale inclusa nel report.
    #[serde(default = "default_report_days")]
    pub days: u32,
    /// Notifier ai quali inviare il report.
    pub notifiers: Vec<String>,
}

fn default_report_days() -> u32 {
    7
}

impl Config {
    /// Effettua il solo parsing YAML, senza validazione dei riferimenti
    /// incrociati (destination/notifier/cron): usare [`validate`] dopo.
    pub fn from_yaml(yaml: &str) -> Result<Config, ConfigError> {
        Ok(serde_yaml::from_str(yaml)?)
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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
    GoogleDrive {
        #[serde(default)]
        root: Option<String>,
        access_token_env: String,
        #[serde(default)]
        refresh_token_env: Option<String>,
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        client_secret_env: Option<String>,
        #[serde(default)]
        retry: RetryConfig,
        #[serde(default)]
        bandwidth_limit_kib_s: Option<u64>,
    },
    Dropbox {
        #[serde(default)]
        root: Option<String>,
        access_token_env: String,
        #[serde(default)]
        refresh_token_env: Option<String>,
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        client_secret_env: Option<String>,
        #[serde(default)]
        retry: RetryConfig,
        #[serde(default)]
        bandwidth_limit_kib_s: Option<u64>,
    },
    OneDrive {
        #[serde(default)]
        root: Option<String>,
        access_token_env: String,
        #[serde(default)]
        refresh_token_env: Option<String>,
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        client_secret_env: Option<String>,
        #[serde(default)]
        retry: RetryConfig,
        #[serde(default)]
        bandwidth_limit_kib_s: Option<u64>,
    },
    /// Repository Restic. Viene usato dai job con `engine: restic` e non
    /// costruisce una destination archivio tradizionale.
    Restic {
        repository: String,
        password_env: String,
        /// Variabili richieste dal backend Restic: la chiave è il nome che
        /// Restic riceve, il valore è il nome della variabile sorgente.
        #[serde(default)]
        environment: HashMap<String, String>,
        #[serde(default = "default_restic_binary")]
        binary: String,
        #[serde(default = "default_true")]
        initialize: bool,
        /// Il repository accetta nuove scritture ma la manutenzione
        /// distruttiva avviene con credenziali separate.
        #[serde(default)]
        append_only: bool,
        /// Credenziali privilegiate usate esclusivamente da `bkpo maintain`.
        /// Non vengono mai caricate durante backup, restore o daemon.
        #[serde(default)]
        maintenance_environment: HashMap<String, String>,
        /// Policy S3 Object Lock attesa per il repository. La verifica e'
        /// esplicita e non modifica mai la configurazione del bucket.
        #[serde(default)]
        object_lock: Option<S3ObjectLockConfig>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct S3ObjectLockConfig {
    pub bucket: String,
    pub region: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub access_key_id_env: String,
    pub secret_access_key_env: String,
    #[serde(default)]
    pub session_token_env: Option<String>,
    #[serde(default)]
    pub virtual_host_style: bool,
    pub expected_mode: ObjectLockMode,
    pub minimum_retention_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ObjectLockMode {
    Governance,
    Compliance,
}

fn default_restic_binary() -> String {
    "restic".to_string()
}

fn default_true() -> bool {
    true
}

fn default_sftp_port() -> u16 {
    22
}

/// Retry con backoff esponenziale sulle operazioni di rete verso una
/// destination remota.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    /// Push tramite ntfy (ntfy.sh o self-hosted): pubblica su `<url>/<topic>`.
    /// `token_env` è opzionale (topic protetti da autenticazione).
    Ntfy {
        url: String,
        topic: String,
        #[serde(default)]
        token_env: Option<String>,
    },
    /// Canale verso l'hub multi-sito: l'agent gli spedisce solo metadati
    /// (esiti, durate, errori), mai contenuto dei backup né segreti.
    #[cfg(feature = "hub")]
    Hub {
        url: String,
        token_env: String,
        #[serde(default = "default_hub_heartbeat_seconds")]
        heartbeat_seconds: u64,
        #[serde(default = "default_hub_queue_path")]
        queue_path: String,
        /// Opt-in: se `true` il daemon ritira dall'hub i comandi "esegui
        /// ora"/"verifica ora" e li esegue, ma solo su job presenti in
        /// questa config. Disattivato di default.
        #[serde(default)]
        remote_commands: bool,
        #[serde(default = "default_hub_command_poll_seconds")]
        command_poll_seconds: u64,
        /// Chiave Ed25519 pubblica base64 usata per verificare le policy
        /// distribuite dall'hub. Se assente il polling policy e' disabilitato.
        #[serde(default)]
        policy_public_key: Option<String>,
        /// ID del sito assegnato dall'hub. Viene incluso nel payload firmato
        /// e impedisce il replay di una policy valida su un altro agent.
        #[serde(default)]
        policy_site_id: Option<i64>,
        #[serde(default = "default_hub_policy_state_path")]
        policy_state_path: String,
        #[serde(default = "default_hub_policy_poll_seconds")]
        policy_poll_seconds: u64,
    },
}

#[cfg(feature = "hub")]
fn default_hub_heartbeat_seconds() -> u64 {
    60
}

#[cfg(feature = "hub")]
fn default_hub_command_poll_seconds() -> u64 {
    15
}

#[cfg(feature = "hub")]
fn default_hub_policy_poll_seconds() -> u64 {
    300
}

#[cfg(feature = "hub")]
fn default_hub_policy_state_path() -> String {
    "backuppo-policy-state.json".to_string()
}

#[cfg(feature = "hub")]
fn default_hub_queue_path() -> String {
    "backuppo-hub-queue.sqlite".to_string()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    Folder {
        path: String,
        #[serde(default)]
        exclude: Vec<String>,
    },
    Postgres {
        host: String,
        #[serde(default = "default_postgres_port")]
        port: u16,
        user: String,
        password_env: String,
        database: String,
        /// Se impostato, `pg_dump` viene eseguito con `docker exec` dentro
        /// questo container invece che con il binario locale (utile per
        /// evitare disallineamenti di versione col server).
        #[serde(default)]
        container: Option<String>,
    },
    MySql {
        host: String,
        #[serde(default = "default_mysql_port")]
        port: u16,
        user: String,
        password_env: String,
        database: String,
        #[serde(default)]
        container: Option<String>,
    },
    Sqlite {
        /// Percorso del file del database SQLite da salvare.
        path: String,
    },
    DockerVolume {
        volume: String,
    },
    /// Sorgente generica: esegue un comando e ne cattura lo stdout come
    /// unico file dell'archivio.
    Command {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// Nome del file in cui viene salvato lo stdout del comando.
        #[serde(default = "default_command_output_filename")]
        output_filename: String,
    },
    /// Immagine byte-per-byte di un file o device a blocchi.
    DiskImage {
        path: String,
        #[serde(default = "default_disk_image_filename")]
        output_filename: String,
    },
    /// VM libvirt spenta: salva XML e tutti i dischi elencati da `virsh`.
    LibvirtVm {
        name: String,
        #[serde(default = "default_virsh_binary")]
        virsh_binary: String,
    },
}

fn default_postgres_port() -> u16 {
    5432
}

fn default_mysql_port() -> u16 {
    3306
}

fn default_command_output_filename() -> String {
    "output".to_string()
}

fn default_disk_image_filename() -> String {
    "disk.img".to_string()
}

fn default_virsh_binary() -> String {
    "virsh".to_string()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    #[default]
    Archive,
    Restic,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    Zstd,
    None,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerifyRestore {
    Never,
    Every,
    Daily,
    Weekly,
}

#[derive(Debug, Serialize, Deserialize, Default, JsonSchema)]
pub struct Retention {
    #[serde(default)]
    pub daily: Option<u32>,
    #[serde(default)]
    pub weekly: Option<u32>,
    #[serde(default)]
    pub monthly: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Default, JsonSchema)]
pub struct NotifyConfig {
    #[serde(default)]
    pub on_success: Vec<String>,
    #[serde(default)]
    pub on_failure: Vec<String>,
    #[serde(default)]
    pub on_verify: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobConfig {
    pub source: SourceConfig,
    pub destination: String,
    /// `archive` mantiene il formato Backuppo; `restic` usa il repository
    /// Restic configurato come destination per deduplica e incrementali.
    #[serde(default)]
    pub engine: EngineKind,
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
    /// Comandi shell eseguiti prima di preparare la sorgente: un loro
    /// fallimento interrompe il job (utile per operazioni di cui il backup
    /// dipende, es. un checkpoint del database).
    #[serde(default)]
    pub pre: Vec<String>,
    /// Comandi shell eseguiti dopo un backup riuscito: un loro fallimento
    /// viene solo loggato, non fa fallire il job (il backup è già andato a
    /// buon fine).
    #[serde(default)]
    pub post: Vec<String>,
}
