use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::hub_protocol::{CommandResult, EventPayload, PendingCommand};
use backuppo_core::model::JobEvent;
use backuppo_core::secrets::resolve_env;
use backuppo_core::traits::Notifier;
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::warn;

/// Notifier verso l'hub multi-sito: spedisce solo metadati (mai contenuto
/// dei backup né segreti). Un invio fallito viene messo in coda locale su
/// SQLite e ritentato più tardi da [`flush_queue`], senza mai far fallire
/// il job che ha generato l'evento (garantito dal chiamante, che logga
/// soltanto l'errore restituito da `send`).
pub struct HubNotifier {
    base_url: String,
    token: String,
    client: Client,
    queue_path: PathBuf,
}

impl HubNotifier {
    pub fn new(url: &str, token_env: &str, queue_path: &str) -> Result<Self, BackupError> {
        let token = resolve_env("token_env", token_env)?;
        Ok(Self {
            base_url: url.trim_end_matches('/').to_string(),
            token,
            client: Client::new(),
            queue_path: PathBuf::from(queue_path),
        })
    }

    #[cfg(test)]
    fn with_base(
        base_url: impl Into<String>,
        token: impl Into<String>,
        queue: &std::path::Path,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            token: token.into(),
            client: Client::new(),
            queue_path: queue.to_path_buf(),
        }
    }

    async fn post(&self, path: &str, body: &impl Serialize) -> Result<(), BackupError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| BackupError::Other(format!("errore di rete verso l'hub: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BackupError::Other(format!(
                "l'hub ha risposto {status}: {body}"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl Notifier for HubNotifier {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError> {
        let payload = EventPayload::from(event);
        if let Err(error) = self.post("/v1/events", &payload).await {
            warn!(%error, "invio evento all'hub fallito, metto in coda per retry");
            let queue = HubQueue::open(&self.queue_path)?;
            let json = serde_json::to_string(&payload)
                .map_err(|e| BackupError::Other(format!("serializzazione evento hub: {e}")))?;
            queue.enqueue(&json, now())?;
            return Err(error);
        }
        Ok(())
    }
}

/// Invia un heartbeat all'hub. Usato dal daemon a intervalli regolari;
/// un fallimento va solo loggato dal chiamante.
pub async fn send_heartbeat(url: &str, token_env: &str) -> Result<(), BackupError> {
    let token = resolve_env("token_env", token_env)?;
    let client = Client::new();
    let response = client
        .post(format!("{}/v1/heartbeat", url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| BackupError::Other(format!("errore di rete verso l'hub: {e}")))?;
    if !response.status().is_success() {
        return Err(BackupError::Other(format!(
            "l'hub ha risposto {} all'heartbeat",
            response.status()
        )));
    }
    Ok(())
}

/// Ritira dall'hub i comandi "esegui ora"/"verifica ora" in attesa per questo
/// sito. Ogni comando viene consegnato una sola volta.
pub async fn fetch_commands(
    url: &str,
    token_env: &str,
) -> Result<Vec<PendingCommand>, BackupError> {
    let token = resolve_env("token_env", token_env)?;
    let response = Client::new()
        .get(format!("{}/v1/commands/pending", url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| BackupError::Other(format!("errore di rete verso l'hub: {e}")))?;
    if !response.status().is_success() {
        return Err(BackupError::Other(format!(
            "l'hub ha risposto {} al ritiro dei comandi",
            response.status()
        )));
    }
    response
        .json()
        .await
        .map_err(|e| BackupError::Other(format!("risposta comandi dell'hub non valida: {e}")))
}

/// Riporta all'hub l'esito di un comando ritirato.
pub async fn report_command(
    url: &str,
    token_env: &str,
    id: i64,
    result: &CommandResult,
) -> Result<(), BackupError> {
    let token = resolve_env("token_env", token_env)?;
    let response = Client::new()
        .post(format!(
            "{}/v1/commands/{id}/result",
            url.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .json(result)
        .send()
        .await
        .map_err(|e| BackupError::Other(format!("errore di rete verso l'hub: {e}")))?;
    if !response.status().is_success() {
        return Err(BackupError::Other(format!(
            "l'hub ha risposto {} all'esito del comando {id}",
            response.status()
        )));
    }
    Ok(())
}

/// Ritenta l'invio degli eventi rimasti in coda locale (per errori di rete
/// o hub irraggiungibile). Ritorna quanti eventi sono stati consegnati.
/// Non deve mai propagare un errore che possa far fallire il daemon: ogni
/// problema di invio viene solo loggato e l'evento resta in coda.
pub async fn flush_queue(
    url: &str,
    token_env: &str,
    queue_path: &str,
) -> Result<usize, BackupError> {
    let queue = HubQueue::open(queue_path)?;
    let token = resolve_env("token_env", token_env)?;
    let client = Client::new();
    let base_url = url.trim_end_matches('/').to_string();
    let mut delivered = 0;

    for (id, payload_json, attempts) in queue.due(now())? {
        let outcome = client
            .post(format!("{base_url}/v1/events"))
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(payload_json)
            .send()
            .await;
        match outcome {
            Ok(response) if response.status().is_success() => {
                queue.remove(id)?;
                delivered += 1;
            }
            Ok(response) => {
                warn!(status = %response.status(), "retry evento hub fallito");
                queue.reschedule(id, attempts, now())?;
            }
            Err(error) => {
                warn!(%error, "retry evento hub fallito");
                queue.reschedule(id, attempts, now())?;
            }
        }
    }
    Ok(delivered)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Backoff esponenziale con tetto a un'ora: 30s, 60s, 120s, ... fino a 3600s.
fn backoff_seconds(attempts: u32) -> i64 {
    let capped = attempts.min(7); // 30 * 2^7 = 3840, già oltre il tetto
    (30i64.saturating_mul(1i64 << capped)).min(3600)
}

struct HubQueue {
    path: PathBuf,
}

impl HubQueue {
    fn open(path: impl Into<PathBuf>) -> Result<Self, BackupError> {
        let queue = Self { path: path.into() };
        let connection = queue.connect()?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS hub_queue (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    payload TEXT NOT NULL,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    next_attempt_at INTEGER NOT NULL
                 );",
            )
            .map_err(sql_error("inizializzazione coda hub"))?;
        Ok(queue)
    }

    fn connect(&self) -> Result<Connection, BackupError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let connection = Connection::open(&self.path).map_err(|error| {
            BackupError::Other(format!(
                "impossibile aprire la coda hub '{}': {error}",
                self.path.display()
            ))
        })?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql_error("configurazione timeout coda hub"))?;
        Ok(connection)
    }

    fn enqueue(&self, payload_json: &str, now: i64) -> Result<(), BackupError> {
        self.connect()?
            .execute(
                "INSERT INTO hub_queue (payload, attempts, next_attempt_at) VALUES (?1, 0, ?2)",
                params![payload_json, now],
            )
            .map_err(sql_error("accodamento evento hub"))?;
        Ok(())
    }

    fn due(&self, now: i64) -> Result<Vec<(i64, String, u32)>, BackupError> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, payload, attempts FROM hub_queue
                 WHERE next_attempt_at <= ?1 ORDER BY id ASC",
            )
            .map_err(sql_error("lettura coda hub"))?;
        let rows = statement
            .query_map(params![now], |row| {
                let attempts: i64 = row.get(2)?;
                Ok((row.get(0)?, row.get(1)?, attempts.max(0) as u32))
            })
            .map_err(sql_error("lettura coda hub"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sql_error("lettura riga coda hub"))
    }

    fn remove(&self, id: i64) -> Result<(), BackupError> {
        self.connect()?
            .execute("DELETE FROM hub_queue WHERE id = ?1", params![id])
            .map_err(sql_error("rimozione evento hub"))?;
        Ok(())
    }

    fn reschedule(&self, id: i64, attempts: u32, now: i64) -> Result<(), BackupError> {
        let next_attempts = attempts.saturating_add(1);
        let next_attempt_at = now + backoff_seconds(next_attempts);
        self.connect()?
            .execute(
                "UPDATE hub_queue SET attempts = ?2, next_attempt_at = ?3 WHERE id = ?1",
                params![id, next_attempts, next_attempt_at],
            )
            .map_err(sql_error("aggiornamento coda hub"))?;
        Ok(())
    }
}

fn sql_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> BackupError {
    move |error| BackupError::Other(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sends_event_and_succeeds_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/events"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().unwrap();
        let notifier = HubNotifier::with_base(
            server.uri(),
            "test-token",
            &temp.path().join("queue.sqlite"),
        );
        let event = JobEvent::Success {
            job: "documents".to_string(),
            artifact: backuppo_core::model::Artifact {
                path: "irrelevante".into(),
                bytes: 10,
                files: 1,
                checksum: "abc".to_string(),
            },
        };

        notifier.send(&event).await.expect("invio riuscito");
    }

    #[tokio::test]
    async fn queues_the_event_when_the_hub_is_unreachable_and_flush_retries_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/events"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let temp = tempfile::tempdir().unwrap();
        let queue_path = temp.path().join("queue.sqlite");
        let notifier = HubNotifier::with_base(server.uri(), "test-token", &queue_path);
        let event = JobEvent::Failure {
            job: "documents".to_string(),
            error: "disco pieno".to_string(),
        };

        let error = notifier
            .send(&event)
            .await
            .expect_err("l'hub non raggiungibile deve restituire un errore");
        assert!(error.to_string().contains("503"));

        // L'evento è ora in coda: un flush contro un hub che risponde 200 lo consegna.
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/v1/events"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        // SAFETY: nome di variabile univoco per questo test, nessuna race.
        unsafe {
            std::env::set_var("BACKUPPO_HUB_TEST_TOKEN", "test-token");
        }
        let delivered = flush_queue(
            &server.uri(),
            "BACKUPPO_HUB_TEST_TOKEN",
            queue_path.to_str().unwrap(),
        )
        .await
        .expect("flush non deve fallire");
        assert_eq!(delivered, 1);
    }

    #[test]
    fn backoff_grows_and_is_capped_at_one_hour() {
        assert_eq!(backoff_seconds(1), 60);
        assert_eq!(backoff_seconds(2), 120);
        assert_eq!(backoff_seconds(10), 3600);
    }

    #[tokio::test]
    async fn fetches_pending_commands_and_reports_the_result() {
        use backuppo_core::hub_protocol::CommandKind;
        std::env::set_var("BKPO_TEST_HUB_CMD_TOKEN", "tk");
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/commands/pending"))
            .and(header("authorization", "Bearer tk"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 4, "kind": "verify", "job": "documents"}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/commands/4/result"))
            .and(wiremock::matchers::body_json(
                serde_json::json!({"ok": true, "detail": "1 file"}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let commands = fetch_commands(&server.uri(), "BKPO_TEST_HUB_CMD_TOKEN")
            .await
            .unwrap();
        assert_eq!(
            commands,
            vec![PendingCommand {
                id: 4,
                kind: CommandKind::Verify,
                job: "documents".to_string()
            }]
        );
        report_command(
            &server.uri(),
            "BKPO_TEST_HUB_CMD_TOKEN",
            4,
            &CommandResult {
                ok: true,
                detail: Some("1 file".to_string()),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn command_http_errors_are_reported() {
        std::env::set_var("BKPO_TEST_HUB_CMD_TOKEN2", "tk");
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let error = fetch_commands(&server.uri(), "BKPO_TEST_HUB_CMD_TOKEN2")
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("401"), "{error}");
    }
}
