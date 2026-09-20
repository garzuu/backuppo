use std::str::FromStr;

use thiserror::Error;

use super::Config;

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
}

/// Valida i riferimenti incrociati (destination/notifier) e la sintassi cron
/// di ogni job. Il parsing dei tipi/campi obbligatori è già garantito da
/// [`Config::from_yaml`] tramite serde.
pub fn validate(config: &Config) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    for (job_name, job) in &config.jobs {
        if !config.destinations.contains_key(&job.destination) {
            errors.push(ConfigError::UnknownDestination {
                job: job_name.clone(),
                name: job.destination.clone(),
            });
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
}
