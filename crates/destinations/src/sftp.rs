use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use backon::{ExponentialBuilder, Retryable};
use backupper_core::config::RetryConfig;
use backupper_core::error::BackupError;
use backupper_core::model::Artifact;
use backupper_core::secrets::resolve_env;
use backupper_core::traits::Destination;
use russh::client::{self, Handle, Handler};
use russh::keys::ssh_key::PublicKey;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg};
use russh_sftp::client::SftpSession;
use tracing::warn;

/// Destination SFTP: client SSH/SFTP nativo in Rust (`russh` + `russh-sftp`),
/// non basato su `opendal` (il cui backend sftp fa da wrapper al binario di
/// sistema `ssh` e supporta solo l'autenticazione a chiave). Supporta sia
/// password che chiave privata, e resta un binario statico.
pub struct SftpDestination {
    host: String,
    port: u16,
    user: String,
    auth: SftpAuth,
    root: String,
    host_key_fingerprint: Option<String>,
    retry: RetryConfig,
}

type SftpFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, BackupError>> + Send + 'a>>;

enum SftpAuth {
    Password(String),
    Key {
        path: String,
        passphrase: Option<String>,
    },
}

impl SftpDestination {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        host: &str,
        port: u16,
        user: &str,
        password_env: Option<&str>,
        key_path: Option<&str>,
        key_passphrase_env: Option<&str>,
        root: &str,
        host_key_fingerprint: Option<&str>,
        retry: RetryConfig,
    ) -> Result<Self, BackupError> {
        let auth = if let Some(key_path) = key_path {
            let passphrase = key_passphrase_env
                .map(|var| resolve_env("key_passphrase_env", var))
                .transpose()?;
            SftpAuth::Key {
                path: key_path.to_string(),
                passphrase,
            }
        } else if let Some(password_env) = password_env {
            SftpAuth::Password(resolve_env("password_env", password_env)?)
        } else {
            return Err(BackupError::Other(
                "destination sftp: serve 'password_env' o 'key_path'".to_string(),
            ));
        };

        Ok(Self {
            host: host.to_string(),
            port,
            user: user.to_string(),
            auth,
            root: root.trim_end_matches('/').to_string(),
            host_key_fingerprint: host_key_fingerprint.map(str::to_string),
            retry,
        })
    }

    fn remote_path(&self, name: &str) -> String {
        let name = name.trim_start_matches('/');
        if self.root.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.root)
        }
    }

    async fn connect(&self) -> Result<(Handle<ClientHandler>, SftpSession), BackupError> {
        let config = Arc::new(client::Config::default());
        let handler = ClientHandler {
            expected_fingerprint: self.host_key_fingerprint.clone(),
        };
        let mut handle = client::connect(config, (self.host.as_str(), self.port), handler)
            .await
            .map_err(|e| {
                BackupError::Other(format!(
                    "connessione SSH a '{}:{}' fallita: {e}",
                    self.host, self.port
                ))
            })?;

        let authenticated = match &self.auth {
            SftpAuth::Password(password) => handle
                .authenticate_password(&self.user, password)
                .await
                .map_err(|e| BackupError::Other(format!("autenticazione SSH fallita: {e}")))?,
            SftpAuth::Key { path, passphrase } => {
                let key = load_secret_key(path, passphrase.as_deref()).map_err(|e| {
                    BackupError::Other(format!(
                        "impossibile caricare la chiave privata '{path}': {e}"
                    ))
                })?;
                handle
                    .authenticate_publickey(
                        &self.user,
                        PrivateKeyWithHashAlg::new(Arc::new(key), Some(HashAlg::Sha256)),
                    )
                    .await
                    .map_err(|e| BackupError::Other(format!("autenticazione SSH fallita: {e}")))?
            }
        };
        if !authenticated.success() {
            return Err(BackupError::Other(format!(
                "autenticazione SSH rifiutata per l'utente '{}'",
                self.user
            )));
        }

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| BackupError::Other(format!("apertura canale SSH fallita: {e}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| BackupError::Other(format!("richiesta sottosistema sftp fallita: {e}")))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| BackupError::Other(format!("apertura sessione sftp fallita: {e}")))?;

        if !self.root.is_empty() && !sftp.try_exists(&self.root).await.unwrap_or(true) {
            sftp.create_dir(&self.root).await.map_err(|e| {
                BackupError::Other(format!(
                    "impossibile creare la cartella remota '{}': {e}",
                    self.root
                ))
            })?;
        }

        Ok((handle, sftp))
    }

    /// Esegue `op` su una sessione sftp appena connessa, riprovando l'intera
    /// sequenza connessione+autenticazione+operazione con backoff
    /// esponenziale in caso di errore (fino a `retry.max_times` volte).
    async fn with_sftp<T, F>(&self, op: F) -> Result<T, BackupError>
    where
        F: for<'a> Fn(&'a SftpSession) -> SftpFuture<'a, T>,
    {
        let backoff = ExponentialBuilder::default().with_max_times(self.retry.max_times);
        (|| async {
            let (_handle, sftp) = self.connect().await?;
            op(&sftp).await
        })
        .retry(backoff)
        .notify(|e, dur| {
            warn!(error = %e, retry_in = ?dur, "operazione sftp fallita, nuovo tentativo");
        })
        .await
    }
}

/// Verifica (o accetta senza verifica, con un avviso) la host key SSH del
/// server, confrontandone il fingerprint SHA256 con quello configurato.
struct ClientHandler {
    expected_fingerprint: Option<String>,
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual = server_public_key.fingerprint(HashAlg::Sha256).to_string();

        match &self.expected_fingerprint {
            Some(expected) if expected == &actual => Ok(true),
            Some(expected) => {
                warn!(expected = %expected, actual = %actual, "fingerprint della host key SFTP non corrispondente: connessione rifiutata");
                Ok(false)
            }
            None => {
                warn!(fingerprint = %actual, "host key SFTP accettata senza verifica (nessun 'host_key_fingerprint' configurato): rischio man-in-the-middle");
                Ok(true)
            }
        }
    }
}

fn to_sftp_error(e: russh_sftp::client::error::Error) -> BackupError {
    BackupError::Other(format!("errore sftp: {e}"))
}

fn artifact_name(path: &Path) -> Result<String, BackupError> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            BackupError::Other(format!("percorso artifact non valido: {}", path.display()))
        })
}

#[async_trait]
impl Destination for SftpDestination {
    async fn upload(&self, artifact: &Artifact) -> Result<(), BackupError> {
        let name = artifact_name(&artifact.path)?;
        let remote = self.remote_path(&name);
        let bytes = tokio::fs::read(&artifact.path).await?;

        self.with_sftp(|sftp| {
            let remote = remote.clone();
            let bytes = bytes.clone();
            Box::pin(async move {
                use tokio::io::AsyncWriteExt;
                // `sftp.write()` richiede che il file esista già (usa solo
                // il flag WRITE); `create()` invece lo crea/tronca se serve.
                let mut file = sftp.create(remote).await.map_err(to_sftp_error)?;
                file.write_all(&bytes)
                    .await
                    .map_err(|e| BackupError::Other(format!("errore scrittura sftp: {e}")))?;
                file.shutdown()
                    .await
                    .map_err(|e| BackupError::Other(format!("errore chiusura file sftp: {e}")))?;
                Ok(())
            })
        })
        .await
    }

    async fn download(&self, name: &str, dest: &Path) -> Result<Artifact, BackupError> {
        let remote = self.remote_path(name);

        let bytes = self
            .with_sftp(|sftp| {
                let remote = remote.clone();
                Box::pin(async move { sftp.read(remote).await.map_err(to_sftp_error) })
            })
            .await?;

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(dest, &bytes).await?;

        Ok(Artifact {
            path: dest.to_path_buf(),
            bytes: bytes.len() as u64,
            files: 0,
            checksum: String::new(),
        })
    }

    async fn list(&self) -> Result<Vec<String>, BackupError> {
        let root = self.root.clone();
        self.with_sftp(|sftp| {
            let root = root.clone();
            Box::pin(async move {
                let dir = if root.is_empty() {
                    ".".to_string()
                } else {
                    root
                };
                let entries = sftp.read_dir(&dir).await.map_err(to_sftp_error)?;
                Ok(entries
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.file_name())
                    .collect())
            })
        })
        .await
    }

    async fn delete(&self, name: &str) -> Result<(), BackupError> {
        let remote = self.remote_path(name);
        self.with_sftp(|sftp| {
            let remote = remote.clone();
            Box::pin(async move { sftp.remove_file(remote).await.map_err(to_sftp_error) })
        })
        .await
    }
}
