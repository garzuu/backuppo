use std::collections::HashMap;
use std::time::Duration;

use backuppo_core::config::NotifierConfig;
use backuppo_core::model::JobEvent;
use tracing::{info, warn};

use crate::api::AppState;

/// Controlla periodicamente i siti senza heartbeat recente e li marca
/// offline, notificando sui canali configurati. Un errore di notifica non
/// deve mai interrompere il ciclo di controllo.
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let threshold = chrono::Utc::now().timestamp() - state.offline_after_minutes * 60;
        let overdue = match state.db.sites_overdue(threshold) {
            Ok(sites) => sites,
            Err(error) => {
                warn!(%error, "controllo siti offline fallito");
                continue;
            }
        };
        for site in overdue {
            if let Err(error) = state.db.mark_site_offline(site.id) {
                warn!(%error, site = site.name.as_str(), "impossibile marcare il sito offline");
                continue;
            }
            info!(
                site = site.name.as_str(),
                "sito marcato offline per assenza di heartbeat"
            );
            let event = JobEvent::SiteOffline {
                site: site.name.clone(),
                minutes: state.offline_after_minutes,
            };
            dispatch(&state.notifiers, &state.notify_on_offline, &event).await;
        }
    }
}

pub(crate) async fn dispatch(
    notifiers: &HashMap<String, NotifierConfig>,
    names: &[String],
    event: &JobEvent,
) {
    for name in names {
        let Some(config) = notifiers.get(name) else {
            warn!(
                notifier = name.as_str(),
                "notifier non trovato in config hub"
            );
            continue;
        };
        match backuppo_notifiers::build(config) {
            Ok(notifier) => {
                if let Err(error) = notifier.send(event).await {
                    warn!(notifier = name.as_str(), %error, "invio notifica hub fallito");
                }
            }
            Err(error) => {
                warn!(notifier = name.as_str(), %error, "costruzione notifier hub fallita")
            }
        }
    }
}
