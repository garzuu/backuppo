//! Crate `notifiers`: implementazioni del trait `Notifier` di `core`.

#[cfg(feature = "hub")]
pub mod hub;
mod ntfy;
mod smtp;
mod telegram;
mod webhook;

#[cfg(feature = "hub")]
pub use hub::HubNotifier;
pub use ntfy::NtfyNotifier;
pub use smtp::SmtpNotifier;
pub use telegram::TelegramNotifier;
pub use webhook::WebhookNotifier;

use backuppo_core::config::NotifierConfig;
use backuppo_core::error::BackupError;
use backuppo_core::traits::Notifier;

/// Costruisce l'implementazione di `Notifier` corrispondente alla config.
pub fn build(config: &NotifierConfig) -> Result<Box<dyn Notifier>, BackupError> {
    match config {
        NotifierConfig::Telegram { token_env, chat_id } => {
            Ok(Box::new(TelegramNotifier::new(token_env, chat_id)?))
        }
        NotifierConfig::Smtp {
            host,
            port,
            user,
            password_env,
            from,
            to,
        } => Ok(Box::new(SmtpNotifier::new(
            host,
            *port,
            user,
            password_env,
            from,
            to,
        )?)),
        NotifierConfig::Webhook { url } => Ok(Box::new(WebhookNotifier::new(url)?)),
        NotifierConfig::Ntfy {
            url,
            topic,
            token_env,
        } => Ok(Box::new(NtfyNotifier::new(
            url,
            topic,
            token_env.as_deref(),
        )?)),
        #[cfg(feature = "hub")]
        NotifierConfig::Hub {
            url,
            token_env,
            heartbeat_seconds: _,
            queue_path,
        } => Ok(Box::new(HubNotifier::new(url, token_env, queue_path)?)),
    }
}
