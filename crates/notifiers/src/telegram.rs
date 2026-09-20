use async_trait::async_trait;
use backupper_core::error::BackupError;
use backupper_core::model::JobEvent;
use backupper_core::secrets::resolve_env;
use backupper_core::traits::Notifier;
use reqwest::Client;
use serde::Serialize;

/// Notifier Telegram: invia un messaggio di testo tramite il metodo
/// `sendMessage` della Bot API.
pub struct TelegramNotifier {
    token: String,
    chat_id: String,
    client: Client,
    api_base: String,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
}

impl TelegramNotifier {
    pub fn new(token_env: &str, chat_id: &str) -> Result<Self, BackupError> {
        let token = resolve_env("token_env", token_env)?;
        Ok(Self {
            token,
            chat_id: chat_id.to_string(),
            client: Client::new(),
            api_base: "https://api.telegram.org".to_string(),
        })
    }

    /// Costruttore usato dai test per puntare a un mock server locale invece
    /// che alla vera Bot API.
    #[cfg(test)]
    pub(crate) fn with_api_base(
        token: impl Into<String>,
        chat_id: impl Into<String>,
        api_base: impl Into<String>,
    ) -> Self {
        Self {
            token: token.into(),
            chat_id: chat_id.into(),
            client: Client::new(),
            api_base: api_base.into(),
        }
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError> {
        let url = format!("{}/bot{}/sendMessage", self.api_base, self.token);
        let text = event.to_string();
        let body = SendMessageRequest {
            chat_id: &self.chat_id,
            text: &text,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| BackupError::Other(format!("errore di rete verso Telegram: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BackupError::Other(format!(
                "Telegram ha risposto {status}: {body}"
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sends_expected_request_and_succeeds_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bot123:abc/sendMessage"))
            .and(body_json(serde_json::json!({
                "chat_id": "42",
                "text": "job 'documents': backup fallito: disco pieno"
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let notifier = TelegramNotifier::with_api_base("123:abc", "42", server.uri());
        let event = JobEvent::Failure {
            job: "documents".to_string(),
            error: "disco pieno".to_string(),
        };

        notifier
            .send(&event)
            .await
            .expect("l'invio deve avere successo");
    }

    #[tokio::test]
    async fn returns_error_on_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bot123:abc/sendMessage"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let notifier = TelegramNotifier::with_api_base("123:abc", "42", server.uri());
        let event = JobEvent::Report {
            summary: "test".to_string(),
        };

        let err = notifier
            .send(&event)
            .await
            .expect_err("deve fallire su 401");
        assert!(err.to_string().contains("401"));
    }
}
