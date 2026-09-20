//! Crate `notifiers`: implementazioni del trait `Notifier` di `core`.

mod smtp;
mod telegram;

pub use smtp::SmtpNotifier;
pub use telegram::TelegramNotifier;

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
        NotifierConfig::Webhook { .. } => Err(BackupError::Other(
            "notifier 'webhook' non ancora implementato (previsto in Fase 8)".to_string(),
        )),
    }
}
