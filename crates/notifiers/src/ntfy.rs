use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;
use backuppo_core::secrets::resolve_env;
use backuppo_core::traits::Notifier;
use reqwest::{Client, Url};

/// Notifier ntfy: pubblica il messaggio con una `POST <url>/<topic>`. Titolo,
/// priorità e tag viaggiano negli header (solo ASCII), il testo nel corpo.
pub struct NtfyNotifier {
    url: Url,
    token: Option<String>,
    client: Client,
}

impl NtfyNotifier {
    pub fn new(url: &str, topic: &str, token_env: Option<&str>) -> Result<Self, BackupError> {
        if topic.is_empty() || topic.contains(['/', '?', '#', ' ']) {
            return Err(BackupError::Other(format!(
                "topic ntfy non valido '{topic}': non può essere vuoto né contenere / ? # o spazi"
            )));
        }
        let base = Url::parse(url)
            .map_err(|error| BackupError::Other(format!("URL ntfy non valido: {error}")))?;
        let url = base
            .join(&format!("{}/{topic}", base.path().trim_end_matches('/')))
            .map_err(|error| BackupError::Other(format!("URL ntfy non valido: {error}")))?;
        let token = token_env
            .map(|name| resolve_env("token_env", name))
            .transpose()?;
        Ok(Self {
            url,
            token,
            client: Client::new(),
        })
    }
}

/// (titolo, priorità 1-5, tag emoji di ntfy) per tipo di evento: i
/// fallimenti sono ad alta priorità, il resto non deve svegliare nessuno.
fn presentation(event: &JobEvent) -> (&'static str, &'static str, &'static str) {
    match event {
        JobEvent::Failure { .. } => ("Backup fallito", "4", "rotating_light"),
        JobEvent::Success { .. } => ("Backup riuscito", "3", "white_check_mark"),
        JobEvent::RestoreVerified { .. } => ("Restore verificato", "3", "shield"),
        JobEvent::Report { .. } => ("Report Backuppo", "2", "bar_chart"),
        JobEvent::SiteOffline { .. } => ("Sito offline", "4", "warning"),
    }
}

#[async_trait]
impl Notifier for NtfyNotifier {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError> {
        let (title, priority, tags) = presentation(event);
        let mut request = self
            .client
            .post(self.url.clone())
            .header("Title", title)
            .header("Priority", priority)
            .header("Tags", tags)
            .body(event.to_string());
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|error| BackupError::Other(format!("errore di rete verso ntfy: {error}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BackupError::Other(format!(
                "ntfy ha risposto {status}: {body}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn failure_is_published_high_priority_to_the_topic() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/backup-alerts"))
            .and(header("Title", "Backup fallito"))
            .and(header("Priority", "4"))
            .and(header("Tags", "rotating_light"))
            .and(body_string("job 'documents': backup fallito: disk full"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let notifier = NtfyNotifier::new(&server.uri(), "backup-alerts", None).unwrap();
        notifier
            .send(&JobEvent::Failure {
                job: "documents".to_string(),
                error: "disk full".to_string(),
            })
            .await
            .expect("ntfy");
    }

    #[tokio::test]
    async fn site_offline_is_high_priority_with_its_own_title() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/t"))
            .and(header("Title", "Sito offline"))
            .and(header("Priority", "4"))
            .and(body_string(
                "sito 'sede-b' offline: nessun heartbeat da oltre 15 minuti",
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let notifier = NtfyNotifier::new(&server.uri(), "t", None).unwrap();
        notifier
            .send(&JobEvent::SiteOffline {
                site: "sede-b".to_string(),
                minutes: 15,
            })
            .await
            .expect("ntfy");
    }

    #[tokio::test]
    async fn base_url_with_path_and_bearer_token() {
        let server = MockServer::start().await;
        std::env::set_var("BKPO_TEST_NTFY_TOKEN", "tk_secret");
        Mock::given(method("POST"))
            .and(path("/ntfy/t"))
            .and(header("Authorization", "Bearer tk_secret"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let notifier = NtfyNotifier::new(
            &format!("{}/ntfy/", server.uri()),
            "t",
            Some("BKPO_TEST_NTFY_TOKEN"),
        )
        .unwrap();
        notifier
            .send(&JobEvent::Report {
                summary: "7 job ok".to_string(),
            })
            .await
            .expect("ntfy");
    }

    #[tokio::test]
    async fn http_error_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;
        let notifier = NtfyNotifier::new(&server.uri(), "t", None).unwrap();
        let error = notifier
            .send(&JobEvent::Report {
                summary: "x".to_string(),
            })
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("403"), "{error}");
    }

    #[test]
    fn invalid_topic_is_rejected() {
        for topic in ["", "a/b", "a b", "a?x"] {
            assert!(NtfyNotifier::new("https://ntfy.sh", topic, None).is_err());
        }
        assert!(NtfyNotifier::new("non-un-url", "t", None).is_err());
    }
}
