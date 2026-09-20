use backuppo_core::config::Config;
use backuppo_core::model::JobEvent;
use tracing::warn;

/// Invia `event` a ciascun notifier elencato in `names` (nomi presenti in
/// `config.notifiers`). Un errore di invio (o un notifier mancante/non
/// costruibile) viene solo loggato: non deve mai far fallire il job.
pub(crate) async fn dispatch(config: &Config, names: &[String], event: &JobEvent) {
    for name in names {
        let Some(notifier_config) = config.notifiers.get(name) else {
            warn!(
                notifier = name.as_str(),
                "notifier non trovato in config, notifica saltata"
            );
            continue;
        };

        match backuppo_notifiers::build(notifier_config) {
            Ok(notifier) => {
                if let Err(e) = notifier.send(event).await {
                    warn!(notifier = name.as_str(), error = %e, "invio notifica fallito");
                }
            }
            Err(e) => {
                warn!(notifier = name.as_str(), error = %e, "impossibile costruire il notifier");
            }
        }
    }
}
