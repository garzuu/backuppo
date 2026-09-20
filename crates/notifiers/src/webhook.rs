use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;
use backuppo_core::traits::Notifier;
use reqwest::{Client, Url};
use serde::Serialize;

/// Webhook HTTP generico. Il payload include sia campi strutturati sia le
/// chiavi testuali comunemente accettate da Slack, Discord e ntfy.
pub struct WebhookNotifier {
    url: Url,
    client: Client,
}

#[derive(Serialize)]
struct WebhookPayload {
    event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<String>,
    text: String,
    content: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<String>,
}

impl WebhookNotifier {
    pub fn new(url: &str) -> Result<Self, BackupError> {
        let url = Url::parse(url)
            .map_err(|error| BackupError::Other(format!("URL webhook non valido: {error}")))?;
        Ok(Self {
            url,
            client: Client::new(),
        })
    }
}

#[async_trait]
impl Notifier for WebhookNotifier {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError> {
        let payload = payload_for(event);
        let response = self
            .client
            .post(self.url.clone())
            .json(&payload)
            .send()
            .await
            .map_err(|error| {
                BackupError::Other(format!("errore di rete verso webhook: {error}"))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BackupError::Other(format!(
                "webhook ha risposto {status}: {body}"
            )));
        }
        Ok(())
    }
}

fn payload_for(event: &JobEvent) -> WebhookPayload {
    let text = event.to_string();
    let (event_name, job, bytes, files, checksum) = match event {
        JobEvent::Success { job, artifact } => (
            "success",
            Some(job.clone()),
            Some(artifact.bytes),
            Some(artifact.files),
            Some(artifact.checksum.clone()),
        ),
        JobEvent::Failure { job, .. } => ("failure", Some(job.clone()), None, None, None),
        JobEvent::RestoreVerified { job, .. } => {
            ("restore_verified", Some(job.clone()), None, None, None)
        }
        JobEvent::Report { .. } => ("report", None, None, None, None),
    };
    WebhookPayload {
        event: event_name,
        job,
        text: text.clone(),
        content: text.clone(),
        message: text,
        bytes,
        files,
        checksum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sends_a_structured_generic_payload() {
        let server = MockServer::start().await;
        let message = "job 'documents': backup fallito: disk full";
        Mock::given(method("POST"))
            .and(path("/events"))
            .and(body_json(serde_json::json!({
                "event": "failure",
                "job": "documents",
                "text": message,
                "content": message,
                "message": message
            })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let notifier = WebhookNotifier::new(&format!("{}/events", server.uri())).unwrap();
        notifier
            .send(&JobEvent::Failure {
                job: "documents".to_string(),
                error: "disk full".to_string(),
            })
            .await
            .expect("webhook");
    }
}
