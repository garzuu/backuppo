use std::time::Duration;

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;
use backuppo_core::secrets::resolve_env;
use backuppo_core::traits::Notifier;
use rumqttc::{AsyncClient, Event, MqttOptions, Outgoing, QoS, Transport};

/// Notifier Home Assistant via MQTT Discovery: per ogni job pubblica (payload
/// retained) un `binary_sensor` "ultima esecuzione" e un `sensor` "ultimo
/// successo", raggruppati sotto un unico device Home Assistant "Backuppo".
/// Connessione breve per ogni evento (connect → publish → disconnect),
/// coerente con tutti gli altri notifier di questo crate: nessuna
/// connessione persistente.
pub struct MqttNotifier {
    host: String,
    port: u16,
    user: Option<String>,
    password: Option<String>,
    client_id: String,
    discovery_prefix: String,
    tls: bool,
}

impl MqttNotifier {
    pub fn new(
        host: &str,
        port: u16,
        user: Option<&str>,
        password_env: Option<&str>,
        client_id: &str,
        discovery_prefix: &str,
        tls: bool,
    ) -> Result<Self, BackupError> {
        let password = password_env
            .map(|name| resolve_env("notifier.password_env", name))
            .transpose()?;
        Ok(Self {
            host: host.to_string(),
            port,
            user: user.map(str::to_string),
            password,
            client_id: client_id.to_string(),
            discovery_prefix: discovery_prefix.to_string(),
            tls,
        })
    }
}

struct Publication {
    topic: String,
    payload: String,
}

/// Converte un nome job in un object_id sicuro per i topic MQTT/Home
/// Assistant (solo `[a-z0-9_]`).
fn slugify(job: &str) -> String {
    job.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// `Report` e `SiteOffline` non hanno un singolo job a cui riferirsi: non
/// producono alcuna pubblicazione MQTT.
fn job_name(event: &JobEvent) -> Option<&str> {
    match event {
        JobEvent::Success { job, .. }
        | JobEvent::Failure { job, .. }
        | JobEvent::RestoreVerified { job, .. } => Some(job),
        JobEvent::Report { .. } | JobEvent::SiteOffline { .. } => None,
    }
}

/// Configurazione di discovery Home Assistant per i due sensori di un job.
/// Pubblicata a ogni evento: è lo stesso payload retained ogni volta, quindi
/// non serve una cache per renderla "pubblica una sola volta".
fn discovery_publications(discovery_prefix: &str, object_id: &str, job: &str) -> Vec<Publication> {
    let device = serde_json::json!({
        "identifiers": ["backuppo"],
        "name": "Backuppo",
        "manufacturer": "Backuppo",
    });
    vec![
        Publication {
            topic: format!("{discovery_prefix}/binary_sensor/backuppo_{object_id}/last_run/config"),
            payload: serde_json::json!({
                "unique_id": format!("backuppo_{object_id}_last_run"),
                "name": format!("{job} backup"),
                "state_topic": format!("backuppo/{object_id}/last_run/state"),
                "payload_on": "ON",
                "payload_off": "OFF",
                "device_class": "problem",
                "device": device,
            })
            .to_string(),
        },
        Publication {
            topic: format!("{discovery_prefix}/sensor/backuppo_{object_id}/last_success/config"),
            payload: serde_json::json!({
                "unique_id": format!("backuppo_{object_id}_last_success"),
                "name": format!("{job} ultimo successo"),
                "state_topic": format!("backuppo/{object_id}/last_success/state"),
                "device_class": "timestamp",
                "device": device,
            })
            .to_string(),
        },
    ]
}

/// Stato da pubblicare per `event`: sempre il topic "ultima esecuzione"
/// (`ON` se fallita, `OFF` altrimenti), più il topic "ultimo successo"
/// (timestamp ISO8601) solo su `Success`/`RestoreVerified` — il retained
/// precedente resta come "ultimo buono" se l'ultimo run è fallito.
fn state_publications(object_id: &str, event: &JobEvent) -> Vec<Publication> {
    let mut publications = vec![Publication {
        topic: format!("backuppo/{object_id}/last_run/state"),
        payload: if matches!(event, JobEvent::Failure { .. }) {
            "ON"
        } else {
            "OFF"
        }
        .to_string(),
    }];
    if matches!(
        event,
        JobEvent::Success { .. } | JobEvent::RestoreVerified { .. }
    ) {
        publications.push(Publication {
            topic: format!("backuppo/{object_id}/last_success/state"),
            payload: chrono::Utc::now().to_rfc3339(),
        });
    }
    publications
}

#[async_trait]
impl Notifier for MqttNotifier {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError> {
        let Some(job) = job_name(event) else {
            tracing::debug!(%event, "evento senza job singolo, notifica MQTT saltata");
            return Ok(());
        };
        let object_id = slugify(job);
        let mut publications = discovery_publications(&self.discovery_prefix, &object_id, job);
        publications.extend(state_publications(&object_id, event));
        self.publish_all(&publications).await
    }
}

impl MqttNotifier {
    async fn publish_all(&self, publications: &[Publication]) -> Result<(), BackupError> {
        let mut options = MqttOptions::new(&self.client_id, &self.host, self.port);
        options.set_keep_alive(Duration::from_secs(10));
        if let (Some(user), Some(password)) = (&self.user, &self.password) {
            options.set_credentials(user, password);
        }
        if self.tls {
            options.set_transport(Transport::tls_with_default_config());
        }
        let (client, mut eventloop) = AsyncClient::new(options, publications.len() + 2);

        let publish = async {
            for publication in publications {
                client
                    .publish(
                        &publication.topic,
                        QoS::AtLeastOnce,
                        true,
                        publication.payload.as_bytes(),
                    )
                    .await
                    .map_err(|error| self.connection_error(error))?;
            }
            client
                .disconnect()
                .await
                .map_err(|error| self.connection_error(error))?;
            loop {
                match eventloop.poll().await {
                    Ok(Event::Outgoing(Outgoing::Disconnect)) => return Ok(()),
                    Ok(_) => continue,
                    Err(error) => return Err(self.connection_error(error)),
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), publish)
            .await
            .unwrap_or_else(|_| {
                Err(BackupError::Other(format!(
                    "timeout MQTT verso {}:{}",
                    self.host, self.port
                )))
            })
    }

    fn connection_error(&self, error: impl std::fmt::Display) -> BackupError {
        BackupError::Other(format!(
            "errore MQTT verso {}:{}: {error}",
            self.host, self.port
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::model::Artifact;

    #[test]
    fn slugify_keeps_only_lowercase_alphanumerics() {
        assert_eq!(slugify("Documenti Importanti"), "documenti_importanti");
        assert_eq!(slugify("db-01"), "db_01");
    }

    #[test]
    fn report_and_site_offline_have_no_job() {
        assert_eq!(
            job_name(&JobEvent::Report {
                summary: "x".into()
            }),
            None
        );
        assert_eq!(
            job_name(&JobEvent::SiteOffline {
                site: "sede-1".into(),
                minutes: 10
            }),
            None
        );
    }

    #[test]
    fn success_and_failure_have_a_job() {
        let success = JobEvent::Success {
            job: "documenti".into(),
            artifact: Artifact {
                path: "/tmp/x".into(),
                bytes: 1,
                files: 1,
                checksum: "abc".into(),
            },
        };
        assert_eq!(job_name(&success), Some("documenti"));
    }

    #[test]
    fn discovery_payload_groups_both_sensors_under_one_device() {
        let publications = discovery_publications("homeassistant", "documenti", "documenti");
        assert_eq!(publications.len(), 2);
        assert!(publications[0]
            .topic
            .starts_with("homeassistant/binary_sensor/backuppo_documenti/"));
        assert!(publications[1]
            .topic
            .starts_with("homeassistant/sensor/backuppo_documenti/"));
        for publication in &publications {
            let payload: serde_json::Value = serde_json::from_str(&publication.payload).unwrap();
            assert_eq!(payload["device"]["identifiers"][0], "backuppo");
        }
    }

    #[test]
    fn failure_sets_problem_on_without_touching_last_success() {
        let publications = state_publications(
            "documenti",
            &JobEvent::Failure {
                job: "documenti".into(),
                error: "disco pieno".into(),
            },
        );
        assert_eq!(publications.len(), 1);
        assert_eq!(publications[0].topic, "backuppo/documenti/last_run/state");
        assert_eq!(publications[0].payload, "ON");
    }

    #[test]
    fn success_clears_problem_and_publishes_last_success() {
        let publications = state_publications(
            "documenti",
            &JobEvent::Success {
                job: "documenti".into(),
                artifact: Artifact {
                    path: "/tmp/x".into(),
                    bytes: 1,
                    files: 1,
                    checksum: "abc".into(),
                },
            },
        );
        assert_eq!(publications.len(), 2);
        assert_eq!(publications[0].payload, "OFF");
        assert_eq!(
            publications[1].topic,
            "backuppo/documenti/last_success/state"
        );
    }

    /// Verifica reale contro un broker MQTT locale: `brew install mosquitto
    /// && mosquitto` (porta 1883 di default), poi esegui questo test
    /// esplicitamente. Non gira in CI: nessun broker è disponibile lì.
    #[tokio::test]
    #[ignore = "richiede un broker MQTT locale (es. 'brew install mosquitto && mosquitto')"]
    async fn publishes_retained_discovery_and_state_to_a_real_broker() {
        let subscriber_options = MqttOptions::new("backuppo-test-subscriber", "127.0.0.1", 1883);
        let (subscriber, mut subscriber_eventloop) = AsyncClient::new(subscriber_options, 10);
        subscriber
            .subscribe("homeassistant/#", QoS::AtLeastOnce)
            .await
            .expect("subscribe");
        // Drena il ConnAck/SubAck iniziali prima di pubblicare.
        for _ in 0..2 {
            subscriber_eventloop.poll().await.expect("subscriber poll");
        }

        let notifier = MqttNotifier::new(
            "127.0.0.1",
            1883,
            None,
            None,
            "backuppo-test",
            "homeassistant",
            false,
        )
        .expect("notifier");
        notifier
            .send(&JobEvent::Success {
                job: "documenti-test".into(),
                artifact: Artifact {
                    path: "/tmp/x".into(),
                    bytes: 1,
                    files: 1,
                    checksum: "abc".into(),
                },
            })
            .await
            .expect("send");

        let mut received_discovery = false;
        for _ in 0..10 {
            if let Ok(Event::Incoming(rumqttc::Packet::Publish(publish))) =
                tokio::time::timeout(Duration::from_secs(5), subscriber_eventloop.poll())
                    .await
                    .expect("subscriber timeout")
            {
                if publish.topic.contains("documenti_test") {
                    received_discovery = true;
                    break;
                }
            }
        }
        assert!(
            received_discovery,
            "nessun messaggio di discovery ricevuto dal broker locale"
        );
    }
}
