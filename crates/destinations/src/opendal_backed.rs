use std::path::Path;
use std::sync::Once;

use async_trait::async_trait;
use backupper_core::config::RetryConfig;
use backupper_core::error::BackupError;
use backupper_core::model::Artifact;
use backupper_core::traits::Destination;
use opendal::layers::{RetryLayer, ThrottleLayer};
use opendal::Operator;
use tokio::io::AsyncReadExt;

/// Dimensione dei chunk con cui viene scritto un upload: `ThrottleLayer`
/// applica il rate limiting una volta per ogni scrittura, quindi caricare
/// tutto il file in un'unica chiamata (col burst dimensionato di
/// conseguenza) vanificherebbe il limite di banda per i file più piccoli
/// del burst stesso.
const UPLOAD_CHUNK_SIZE: usize = 256 * 1024;

static INSTALL_HTTP_TRANSPORT: Once = Once::new();

/// Installa il transport HTTP di default di `opendal` (reqwest+rustls) se
/// non è già stato installato dal suo `#[ctor]` automatico. Va chiamata
/// prima di costruire un `Operator` per un servizio basato su HTTP (s3,
/// webdav): senza, la prima richiesta fallisce con un errore di config.
pub(crate) fn ensure_http_transport_installed() {
    INSTALL_HTTP_TRANSPORT.call_once(opendal::install_default);
}

/// Implementazione di `Destination` generica sopra un `Operator` di
/// `opendal`: usata da tutti i backend basati su opendal (fs, s3, webdav).
/// Applica sempre retry con backoff esponenziale e, se configurato, un
/// limite di banda.
pub struct OpendalDestination {
    op: Operator,
}

impl OpendalDestination {
    pub(crate) fn new(
        op: Operator,
        retry: &RetryConfig,
        bandwidth_limit_kib_s: Option<u64>,
    ) -> Self {
        let op = op.layer(
            RetryLayer::new()
                .with_max_times(retry.max_times)
                .with_jitter(),
        );
        let op = match bandwidth_limit_kib_s {
            Some(kib_s) if kib_s > 0 => {
                let bandwidth = kib_s.saturating_mul(1024).min(u32::MAX as u64) as u32;
                // Il burst deve essere abbastanza grande da non far fallire
                // mai un upload: alcuni backend (S3 sotto la soglia
                // multipart, webdav sempre) accorpano tutti i nostri chunk
                // in un'unica scrittura verso il layer sottostante, quindi
                // un burst troppo piccolo rispetto alla dimensione
                // dell'archivio farebbe fallire il job con un errore di
                // rate-limit anziché limitarne solo la velocità. Il limite
                // di banda resta quindi "best effort": rallenta in modo
                // efficace i trasferimenti sostenuti/ripetuti e gli upload
                // di grandi dimensioni suddivisi in più parti dal backend,
                // ma non garantisce un throughput sub-lineare byte per
                // byte su un singolo file piccolo scritto in un colpo solo.
                let burst = bandwidth.saturating_mul(4).max(64 * 1024 * 1024);
                op.layer(ThrottleLayer::new(bandwidth, burst))
            }
            _ => op,
        };
        Self { op }
    }
}

pub(crate) fn to_backup_error(e: opendal::Error) -> BackupError {
    BackupError::Other(format!("errore storage: {e}"))
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
impl Destination for OpendalDestination {
    async fn upload(&self, artifact: &Artifact) -> Result<(), BackupError> {
        let name = artifact_name(&artifact.path)?;

        // Alcuni backend (es. webdav) supportano solo una singola write()
        // per file: in quel caso il limite di banda si applica comunque
        // (tramite il burst) ma non spalma la singola scrittura nel tempo.
        if self.op.info().capability().write_can_multi {
            let mut file = tokio::fs::File::open(&artifact.path).await?;
            let mut writer = self.op.writer(&name).await.map_err(to_backup_error)?;

            let mut buf = vec![0u8; UPLOAD_CHUNK_SIZE];
            loop {
                let n = file.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                writer
                    .write(buf[..n].to_vec())
                    .await
                    .map_err(to_backup_error)?;
            }
            writer.close().await.map_err(to_backup_error)?;
        } else {
            let bytes = tokio::fs::read(&artifact.path).await?;
            self.op.write(&name, bytes).await.map_err(to_backup_error)?;
        }

        Ok(())
    }

    async fn download(&self, name: &str, dest: &Path) -> Result<Artifact, BackupError> {
        let bytes = self.op.read(name).await.map_err(to_backup_error)?;
        let bytes = bytes.to_vec();
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
        let entries = self.op.list("/").await.map_err(to_backup_error)?;
        Ok(entries
            .into_iter()
            .filter(|e| !e.path().ends_with('/'))
            .map(|e| e.path().to_string())
            .collect())
    }

    async fn delete(&self, name: &str) -> Result<(), BackupError> {
        self.op.delete(name).await.map_err(to_backup_error)?;
        Ok(())
    }
}
