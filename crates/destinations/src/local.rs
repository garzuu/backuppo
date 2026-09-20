use std::path::Path;

use async_trait::async_trait;
use backupper_core::error::BackupError;
use backupper_core::model::Artifact;
use backupper_core::traits::Destination;
use opendal::{services::Fs, Operator};

/// Destinazione locale: scrive gli archivi in una cartella del filesystem,
/// tramite `opendal` (servizio `fs`).
pub struct LocalDestination {
    op: Operator,
}

impl LocalDestination {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, BackupError> {
        std::fs::create_dir_all(&root)?;
        let builder = Fs::default().root(&root.as_ref().to_string_lossy());
        let op = Operator::new(builder).map_err(to_backup_error)?;
        Ok(Self { op })
    }
}

fn to_backup_error(e: opendal::Error) -> BackupError {
    BackupError::Other(format!("errore storage locale: {e}"))
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
impl Destination for LocalDestination {
    async fn upload(&self, artifact: &Artifact) -> Result<(), BackupError> {
        let name = artifact_name(&artifact.path)?;
        let bytes = tokio::fs::read(&artifact.path).await?;
        self.op.write(&name, bytes).await.map_err(to_backup_error)?;
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
