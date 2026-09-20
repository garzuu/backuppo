use std::net::SocketAddr;
use std::str::FromStr;

use thiserror::Error;

use super::{Config, DestinationConfig, EngineKind};

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

    #[error("campo 'api.bind' non valido ('{bind}'): usare un indirizzo loopback IP:porta")]
    InvalidApiBind { bind: String },
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
    }

    for (job_name, job) in &config.jobs {
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
            .map(|address| address.ip().is_loopback())
            .unwrap_or(false);
        if !valid {
            errors.push(ConfigError::InvalidApiBind {
                bind: api.bind.clone(),
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
    }
}
