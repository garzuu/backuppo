use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::JobEvent;
use backuppo_core::secrets::resolve_env;
use backuppo_core::traits::Notifier;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// Notifier email via SMTP (STARTTLS o TLS implicito a seconda della porta,
/// gestiti da `lettre::relay`).
pub struct SmtpNotifier {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    to: Vec<Mailbox>,
}

impl SmtpNotifier {
    pub fn new(
        host: &str,
        port: u16,
        user: &str,
        password_env: &str,
        from: &str,
        to: &[String],
    ) -> Result<Self, BackupError> {
        let password = resolve_env("password_env", password_env)?;
        let creds = Credentials::new(user.to_string(), password);

        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|e| {
                BackupError::Other(format!(
                    "configurazione SMTP non valida per host '{host}': {e}"
                ))
            })?
            .port(port)
            .credentials(creds)
            .build();

        Self::from_transport(transport, from, to)
    }

    /// Transport senza TLS/autenticazione verso un server SMTP locale: usato
    /// solo nei test, per parlare con un mock server in chiaro.
    #[cfg(test)]
    pub(crate) fn dangerous(
        host: &str,
        port: u16,
        from: &str,
        to: &[String],
    ) -> Result<Self, BackupError> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .build();
        Self::from_transport(transport, from, to)
    }

    fn from_transport(
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: &str,
        to: &[String],
    ) -> Result<Self, BackupError> {
        let from = from.parse::<Mailbox>().map_err(|e| {
            BackupError::Other(format!("indirizzo 'from' non valido '{from}': {e}"))
        })?;
        let to = to
            .iter()
            .map(|addr| {
                addr.parse::<Mailbox>().map_err(|e| {
                    BackupError::Other(format!("indirizzo 'to' non valido '{addr}': {e}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            transport,
            from,
            to,
        })
    }
}

#[async_trait]
impl Notifier for SmtpNotifier {
    async fn send(&self, event: &JobEvent) -> Result<(), BackupError> {
        let mut builder = Message::builder()
            .from(self.from.clone())
            .subject(subject_for(event));
        for to in &self.to {
            builder = builder.to(to.clone());
        }
        let message = builder
            .body(event.to_string())
            .map_err(|e| BackupError::Other(format!("errore costruzione email: {e}")))?;

        self.transport
            .send(message)
            .await
            .map_err(|e| BackupError::Other(format!("errore invio SMTP: {e}")))?;
        Ok(())
    }
}

fn subject_for(event: &JobEvent) -> String {
    match event {
        JobEvent::Success { job, .. } => format!("[backuppo] backup riuscito: {job}"),
        JobEvent::Failure { job, .. } => format!("[backuppo] backup FALLITO: {job}"),
        JobEvent::RestoreVerified { job, .. } => format!("[backuppo] restore verificato: {job}"),
        JobEvent::Report { .. } => "[backuppo] report".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    /// Server SMTP giocattolo: risponde ai comandi minimi necessari a
    /// `lettre` per completare una send, e restituisce il testo del comando
    /// `DATA` ricevuto (il corpo dell'email) al chiamante.
    async fn spawn_fake_smtp_server() -> (u16, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = socket.into_split();
            let mut reader = BufReader::new(read_half);

            write_half
                .write_all(b"220 localhost fake smtp\r\n")
                .await
                .unwrap();

            let mut data = String::new();
            let mut in_data = false;
            let mut body = String::new();

            loop {
                let mut line = String::new();
                let n = reader.read_line(&mut line).await.unwrap();
                if n == 0 {
                    break;
                }

                if in_data {
                    if line.trim_end() == "." {
                        write_half.write_all(b"250 OK\r\n").await.unwrap();
                        let _ = tx.send(body);
                        break;
                    }
                    body.push_str(&line);
                    continue;
                }

                data.push_str(&line);
                let upper = line.to_ascii_uppercase();
                if upper.starts_with("EHLO") {
                    write_half
                        .write_all(b"250-localhost\r\n250 OK\r\n")
                        .await
                        .unwrap();
                } else if upper.starts_with("MAIL FROM") || upper.starts_with("RCPT TO") {
                    write_half.write_all(b"250 OK\r\n").await.unwrap();
                } else if upper.starts_with("DATA") {
                    write_half
                        .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                        .await
                        .unwrap();
                    in_data = true;
                } else if upper.starts_with("QUIT") {
                    write_half.write_all(b"221 Bye\r\n").await.unwrap();
                    break;
                }
            }
        });

        (port, rx)
    }

    #[tokio::test]
    async fn sends_email_with_expected_subject_and_body() {
        let (port, body_rx) = spawn_fake_smtp_server().await;

        let notifier = SmtpNotifier::dangerous(
            "127.0.0.1",
            port,
            "backups@example.com",
            &["ops@example.com".to_string()],
        )
        .unwrap();

        let event = JobEvent::Failure {
            job: "documents".to_string(),
            error: "disco pieno".to_string(),
        };
        notifier
            .send(&event)
            .await
            .expect("l'invio deve avere successo");

        let body = body_rx.await.expect("il server deve aver ricevuto un DATA");
        assert!(body.contains("Subject: [backuppo] backup FALLITO: documents"));
        assert!(body.contains("job 'documents': backup fallito: disco pieno"));
    }

    #[tokio::test]
    async fn sends_periodic_report_with_report_subject() {
        let (port, body_rx) = spawn_fake_smtp_server().await;
        let notifier = SmtpNotifier::dangerous(
            "127.0.0.1",
            port,
            "backups@example.com",
            &["ops@example.com".to_string()],
        )
        .unwrap();
        notifier
            .send(&JobEvent::Report {
                summary: "Backup: 7 riusciti, 1 fallito".to_string(),
            })
            .await
            .expect("invio report");

        let body = body_rx.await.expect("DATA ricevuto");
        assert!(body.contains("Subject: [backuppo] report"));
        assert!(body.contains("Backup: 7 riusciti, 1 fallito"));
    }
}
