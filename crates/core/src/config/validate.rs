use std::net::SocketAddr;
use std::str::FromStr;

use thiserror::Error;

use super::{Config, DestinationConfig, EncryptionConfig, EngineKind, UpdateChannel};

/// Errore di parsing o validazione della configurazione. Ogni variante porta
/// il nome del campo/job coinvolto per produrre messaggi leggibili.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("errore di parsing YAML: {0}")]
    Parse(#[from] serde_yaml::Error),

    #[error("job '{job}': destination '{name}' referenziata in 'destination' non è definita in 'destinations'")]
    UnknownDestination { job: String, name: String },

    #[error("job '{job}': notifier '{name}' referenziato in 'notify.{list}' non è definito in 'notifiers'")]
    UnknownNotifier {
        job: String,
        list: String,
        name: String,
    },

    #[error("job '{job}': campo 'schedule' non valido ('{expr}'): {message}")]
    InvalidCron {
        job: String,
        expr: String,
        message: String,
    },

    #[error("report #{index}: campo 'schedule' non valido ('{expr}'): {message}")]
    InvalidReportCron {
        index: usize,
        expr: String,
        message: String,
    },

    #[error("report #{index}: notifier '{name}' non definito in 'notifiers'")]
    UnknownReportNotifier { index: usize, name: String },

    #[error("la sezione 'reports' richiede la sezione 'observability'")]
    ReportsWithoutObservability,

    #[error("destination '{name}': serve almeno uno tra 'password_env' e 'key_path' per l'autenticazione SFTP")]
    SftpMissingAuth { name: String },

    #[error("job '{job}': engine 'restic' richiede una destination di tipo 'restic'")]
    ResticDestinationRequired { job: String },

    #[error("job '{job}': una destination di tipo 'restic' richiede engine: restic")]
    ResticEngineRequired { job: String },

    #[error("job '{job}': encryption age richiede esattamente uno tra passphrase_env e key_env")]
    InvalidAgeCredentials { job: String },

    #[error("campo 'api.bind' non valido ('{bind}'): usare un IP:porta loopback oppure impostare api.allow_remote: true")]
    InvalidApiBind { bind: String },

    #[error("campo 'updates.{field}' non valido: {message}")]
    InvalidUpdate { field: String, message: String },

    #[error("destination '{name}': configurazione Object Lock non valida: {message}")]
    InvalidObjectLock { name: String, message: String },

    #[error("destination '{name}': credenziali di manutenzione non separate: {message}")]
    InvalidMaintenanceCredentials { name: String, message: String },

    #[cfg(feature = "hub")]
    #[error("notifier hub '{name}': configurazione policy non valida: {message}")]
    InvalidHubPolicy { name: String, message: String },
}

/// Valida i riferimenti incrociati (destination/notifier) e la sintassi cron
/// di ogni job. Il parsing dei tipi/campi obbligatori è già garantito da
/// [`Config::from_yaml`] tramite serde.
pub fn validate(config: &Config) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    for (name, destination) in &config.destinations {
        if let DestinationConfig::Sftp {
            password_env,
            key_path,
            ..
        } = destination
        {
            if password_env.is_none() && key_path.is_none() {
                errors.push(ConfigError::SftpMissingAuth { name: name.clone() });
            }
        }
        if let DestinationConfig::Restic {
            append_only,
            environment,
            maintenance_environment,
            object_lock,
            ..
        } = destination
        {
            for (target, admin_source) in maintenance_environment {
                if environment.get(target) == Some(admin_source) {
                    errors.push(ConfigError::InvalidMaintenanceCredentials {
                        name: name.clone(),
                        message: format!(
                            "'{target}' usa la stessa variabile per backup e manutenzione"
                        ),
                    });
                }
            }
            if let Some(lock) = object_lock {
                if !append_only {
                    errors.push(ConfigError::InvalidObjectLock {
                        name: name.clone(),
                        message: "richiede append_only: true".into(),
                    });
                }
                if lock.bucket.trim().is_empty()
                    || lock.region.trim().is_empty()
                    || lock.access_key_id_env.trim().is_empty()
                    || lock.secret_access_key_env.trim().is_empty()
                {
                    errors.push(ConfigError::InvalidObjectLock {
                        name: name.clone(),
                        message: "bucket, region e variabili credenziali sono obbligatori".into(),
                    });
                }
                if lock.minimum_retention_days == 0 {
                    errors.push(ConfigError::InvalidObjectLock {
                        name: name.clone(),
                        message: "minimum_retention_days deve essere maggiore di zero".into(),
                    });
                }
                if let Some(endpoint) = &lock.endpoint {
                    let local_http = endpoint.starts_with("http://127.0.0.1:")
                        || endpoint.starts_with("http://localhost:");
                    if !endpoint.starts_with("https://") && !local_http {
                        errors.push(ConfigError::InvalidObjectLock {
                            name: name.clone(),
                            message: "endpoint deve usare HTTPS (HTTP solo per localhost)".into(),
                        });
                    }
                }
            }
        }
    }

    #[cfg(feature = "hub")]
    for (name, notifier) in &config.notifiers {
        if let super::NotifierConfig::Hub {
            policy_public_key,
            policy_site_id,
            policy_state_path,
            policy_poll_seconds,
            ..
        } = notifier
        {
            if policy_public_key.is_some() != policy_site_id.is_some() {
                errors.push(ConfigError::InvalidHubPolicy {
                    name: name.clone(),
                    message: "policy_public_key e policy_site_id devono essere configurati insieme"
                        .into(),
                });
            }
            if policy_public_key
                .as_ref()
                .is_some_and(|key| key.trim().is_empty())
            {
                errors.push(ConfigError::InvalidHubPolicy {
                    name: name.clone(),
                    message: "policy_public_key non puo' essere vuota".into(),
                });
            }
            if policy_state_path.trim().is_empty() || *policy_poll_seconds == 0 {
                errors.push(ConfigError::InvalidHubPolicy {
                    name: name.clone(),
                    message: "state path non vuoto e poll maggiore di zero richiesti".into(),
                });
            }
        }
    }

    for (job_name, job) in &config.jobs {
        if let Some(EncryptionConfig::Age {
            passphrase_env,
            key_env,
        }) = &job.encryption
        {
            if passphrase_env.is_some() == key_env.is_some() {
                errors.push(ConfigError::InvalidAgeCredentials {
                    job: job_name.clone(),
                });
            }
        }
        if !config.destinations.contains_key(&job.destination) {
            errors.push(ConfigError::UnknownDestination {
                job: job_name.clone(),
                name: job.destination.clone(),
            });
        }

        if let Some(destination) = config.destinations.get(&job.destination) {
            let is_restic = matches!(destination, DestinationConfig::Restic { .. });
            match (job.engine, is_restic) {
                (EngineKind::Restic, false) => {
                    errors.push(ConfigError::ResticDestinationRequired {
                        job: job_name.clone(),
                    });
                }
                (EngineKind::Archive, true) => {
                    errors.push(ConfigError::ResticEngineRequired {
                        job: job_name.clone(),
                    });
                }
                _ => {}
            }
        }

        let notify_lists = [
            ("on_success", &job.notify.on_success),
            ("on_failure", &job.notify.on_failure),
            ("on_verify", &job.notify.on_verify),
        ];
        for (list_name, names) in notify_lists {
            for name in names {
                if !config.notifiers.contains_key(name) {
                    errors.push(ConfigError::UnknownNotifier {
                        job: job_name.clone(),
                        list: list_name.to_string(),
                        name: name.clone(),
                    });
                }
            }
        }

        if let Err(message) = validate_cron(&job.schedule) {
            errors.push(ConfigError::InvalidCron {
                job: job_name.clone(),
                expr: job.schedule.clone(),
                message,
            });
        }
    }

    if let Some(api) = &config.api {
        let valid = api
            .bind
            .parse::<SocketAddr>()
            .is_ok_and(|address| address.ip().is_loopback() || api.allow_remote);
        if !valid {
            errors.push(ConfigError::InvalidApiBind {
                bind: api.bind.clone(),
            });
        }
    }

    if let Some(updates) = &config.updates {
        let local_http = updates.manifest_url.starts_with("http://127.0.0.1:")
            || updates.manifest_url.starts_with("http://localhost:");
        if !updates.manifest_url.starts_with("https://") && !local_http {
            errors.push(ConfigError::InvalidUpdate {
                field: "manifest_url".into(),
                message: "usare HTTPS (HTTP e' ammesso solo per localhost)".into(),
            });
        }
        if updates.public_key.trim().is_empty() {
            errors.push(ConfigError::InvalidUpdate {
                field: "public_key".into(),
                message: "la chiave Ed25519 base64 e' obbligatoria".into(),
            });
        }
        if updates.download_dir.trim().is_empty() {
            errors.push(ConfigError::InvalidUpdate {
                field: "download_dir".into(),
                message: "indicare una directory di staging".into(),
            });
        }
        if updates.channel == UpdateChannel::Pinned && updates.pinned_version.is_none() {
            errors.push(ConfigError::InvalidUpdate {
                field: "pinned_version".into(),
                message: "obbligatoria quando channel e' pinned".into(),
            });
        }
    }

    if !config.reports.is_empty() && config.observability.is_none() {
        errors.push(ConfigError::ReportsWithoutObservability);
    }
    for (index, report) in config.reports.iter().enumerate() {
        let display_index = index + 1;
        if let Err(message) = validate_cron(&report.schedule) {
            errors.push(ConfigError::InvalidReportCron {
                index: display_index,
                expr: report.schedule.clone(),
                message,
            });
        }
        for name in &report.notifiers {
            if !config.notifiers.contains_key(name) {
                errors.push(ConfigError::UnknownReportNotifier {
                    index: display_index,
                    name: name.clone(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// `job.schedule` usa la sintassi cron standard a 5 campi
/// (minuto ora giorno-mese mese giorno-settimana); il crate `cron` richiede
/// il campo dei secondi, che qui viene sempre fissato a 0.
fn validate_cron(expr: &str) -> Result<(), String> {
    let with_seconds = format!("0 {expr}");
    cron::Schedule::from_str(&with_seconds)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config_yaml() -> &'static str {
        r#"
destinations:
  local-nas:
    type: fs
    root: /mnt/backups

notifiers:
  ops-telegram:
    type: telegram
    token_env: TELEGRAM_BOT_TOKEN
    chat_id: "123"

jobs:
  home-documents:
    source:
      type: folder
      path: /home/user/Documents
      exclude: ["*.tmp"]
    destination: local-nas
    compression: zstd
    encryption:
      type: age
      passphrase_env: BACKUP_PASSPHRASE
    schedule: "0 3 * * *"
    verify_restore: daily
    retention:
      daily: 7
    notify:
      on_success: [ops-telegram]
      on_failure: [ops-telegram]
"#
    }

    #[test]
    fn valid_config_parses_and_validates() {
        let config = Config::from_yaml(valid_config_yaml()).expect("parsing valido");
        validate(&config).expect("validazione valida");
    }

    #[test]
    fn rejects_unknown_destination() {
        let yaml =
            valid_config_yaml().replace("destination: local-nas", "destination: does-not-exist");
        let config = Config::from_yaml(&yaml).expect("parsing valido");
        let errors = validate(&config).expect_err("deve fallire");
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigError::UnknownDestination { name, .. } if name == "does-not-exist")));
    }

    #[test]
    fn rejects_unknown_notifier() {
        let yaml = valid_config_yaml()
            .replace("on_success: [ops-telegram]", "on_success: [ghost-notifier]");
        let config = Config::from_yaml(&yaml).expect("parsing valido");
        let errors = validate(&config).expect_err("deve fallire");
        assert!(errors.iter().any(|e| matches!(
            e,
            ConfigError::UnknownNotifier { name, list, .. }
                if name == "ghost-notifier" && list == "on_success"
        )));
    }

    #[test]
    fn rejects_invalid_cron() {
        let yaml =
            valid_config_yaml().replace(r#"schedule: "0 3 * * *""#, r#"schedule: "not a cron""#);
        let config = Config::from_yaml(&yaml).expect("parsing valido");
        let errors = validate(&config).expect_err("deve fallire");
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigError::InvalidCron { .. })));
    }

    #[test]
    fn rejects_missing_required_field() {
        // manca 'destination', campo obbligatorio di JobConfig
        let yaml = r#"
jobs:
  broken:
    source:
      type: folder
      path: /tmp
    schedule: "0 3 * * *"
"#;
        let err = Config::from_yaml(yaml).expect_err("deve fallire il parsing");
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    #[test]
    fn validates_report_schedule_notifiers_and_observability() {
        let yaml = format!(
            "{}\nobservability:\n  history_path: /tmp/history.sqlite\nreports:\n  - schedule: \"0 8 * * 1\"\n    days: 7\n    notifiers: [ops-telegram]\n",
            valid_config_yaml()
        );
        let config = Config::from_yaml(&yaml).expect("parsing valido");
        validate(&config).expect("report valido");

        let invalid = yaml.replace("ops-telegram]", "missing]");
        let config = Config::from_yaml(&invalid).expect("parsing valido");
        let errors = validate(&config).expect_err("notifier report sconosciuto");
        assert!(errors.iter().any(|error| matches!(
            error,
            ConfigError::UnknownReportNotifier { name, .. } if name == "missing"
        )));
    }

    #[test]
    fn validates_local_api_and_restic_engine_pairing() {
        let yaml = r#"
destinations:
  snapshots:
    type: restic
    repository: /tmp/repository
    password_env: RESTIC_PASSWORD
api:
  bind: 127.0.0.1:8787
jobs:
  incremental:
    engine: restic
    source: { type: folder, path: /tmp/source }
    destination: snapshots
    schedule: "0 3 * * *"
"#;
        let config = Config::from_yaml(yaml).expect("parsing valido");
        validate(&config).expect("restic e API locale validi");

        let invalid = yaml
            .replace("engine: restic", "engine: archive")
            .replace("127.0.0.1:8787", "0.0.0.0:8787");
        let config = Config::from_yaml(&invalid).expect("parsing valido");
        let errors = validate(&config).expect_err("config non sicura");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ConfigError::ResticEngineRequired { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ConfigError::InvalidApiBind { .. })));

        let container = yaml.replace(
            "bind: 127.0.0.1:8787",
            "bind: 0.0.0.0:8787\n  allow_remote: true",
        );
        let config = Config::from_yaml(&container).expect("parsing container valido");
        validate(&config).expect("bind container esplicitamente autorizzato");
    }

    #[test]
    fn validates_update_transport_pinning_and_age_credentials() {
        let yaml = format!(
            "{}\nupdates:\n  manifest_url: https://updates.example/stable.json\n  public_key: ZmFrZS1wdWJsaWMta2V5\n  channel: pinned\n  pinned_version: 1.2.3\n",
            valid_config_yaml()
        );
        let config = Config::from_yaml(&yaml).expect("parsing valido");
        validate(&config).expect("update config valida");

        let invalid = yaml
            .replace("https://updates.example", "http://updates.example")
            .replace("  pinned_version: 1.2.3\n", "")
            .replace(
                "      passphrase_env: BACKUP_PASSPHRASE",
                "      passphrase_env: BACKUP_PASSPHRASE\n      key_env: AGE_KEY",
            );
        let config = Config::from_yaml(&invalid).expect("parsing valido");
        let errors = validate(&config).expect_err("config update non sicura");
        assert!(errors.iter().any(|error| matches!(
            error,
            ConfigError::InvalidUpdate { field, .. } if field == "manifest_url"
        )));
        assert!(errors.iter().any(|error| matches!(
            error,
            ConfigError::InvalidUpdate { field, .. } if field == "pinned_version"
        )));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ConfigError::InvalidAgeCredentials { .. })));
    }

    #[test]
    fn validates_object_lock_and_separate_maintenance_credentials() {
        let yaml = r#"
destinations:
  immutable:
    type: restic
    repository: s3:https://s3.example/backups/repository
    password_env: RESTIC_PASSWORD
    append_only: true
    environment:
      AWS_ACCESS_KEY_ID: BACKUP_ACCESS_KEY
      AWS_SECRET_ACCESS_KEY: BACKUP_SECRET_KEY
    maintenance_environment:
      AWS_ACCESS_KEY_ID: ADMIN_ACCESS_KEY
      AWS_SECRET_ACCESS_KEY: ADMIN_SECRET_KEY
    object_lock:
      bucket: backups
      region: eu-central-1
      endpoint: https://s3.example
      access_key_id_env: LOCK_CHECK_ACCESS_KEY
      secret_access_key_env: LOCK_CHECK_SECRET_KEY
      expected_mode: compliance
      minimum_retention_days: 30
jobs:
  protected:
    engine: restic
    source: { type: folder, path: /tmp/source }
    destination: immutable
    schedule: "0 3 * * *"
"#;
        let config = Config::from_yaml(yaml).expect("parsing valido");
        validate(&config).expect("object lock valido");

        let invalid = yaml
            .replace("append_only: true", "append_only: false")
            .replace("ADMIN_SECRET_KEY", "BACKUP_SECRET_KEY")
            .replace("minimum_retention_days: 30", "minimum_retention_days: 0");
        let config = Config::from_yaml(&invalid).expect("parsing valido");
        let errors = validate(&config).expect_err("protezione storage non valida");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ConfigError::InvalidObjectLock { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ConfigError::InvalidMaintenanceCredentials { .. })));
    }

    #[cfg(feature = "hub")]
    #[test]
    fn parses_hub_notifier_with_defaults() {
        use super::super::NotifierConfig;

        let yaml = r#"
notifiers:
  sede-centrale:
    type: hub
    url: https://hub.example.com
    token_env: HUB_TOKEN
"#;
        let config = Config::from_yaml(yaml).expect("parsing valido");
        let notifier = config
            .notifiers
            .get("sede-centrale")
            .expect("notifier presente");
        match notifier {
            NotifierConfig::Hub {
                url,
                token_env,
                heartbeat_seconds,
                queue_path,
                remote_commands,
                command_poll_seconds,
                policy_public_key,
                policy_site_id,
                policy_state_path,
                policy_poll_seconds,
            } => {
                assert_eq!(url, "https://hub.example.com");
                assert_eq!(token_env, "HUB_TOKEN");
                assert_eq!(*heartbeat_seconds, 60);
                assert_eq!(queue_path, "backuppo-hub-queue.sqlite");
                // I comandi remoti sono opt-in: di default sono spenti.
                assert!(!*remote_commands);
                assert_eq!(*command_poll_seconds, 15);
                assert!(policy_public_key.is_none());
                assert!(policy_site_id.is_none());
                assert_eq!(policy_state_path, "backuppo-policy-state.json");
                assert_eq!(*policy_poll_seconds, 300);
            }
            other => panic!("expected NotifierConfig::Hub, got {other:?}"),
        }
    }
}
