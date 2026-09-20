use std::path::{Path, PathBuf};

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;

pub struct DiskImageSource {
    source: PathBuf,
    output_filename: String,
}

impl DiskImageSource {
    pub fn new(
        source: impl Into<PathBuf>,
        output_filename: impl Into<String>,
    ) -> Result<Self, BackupError> {
        let output_filename = output_filename.into();
        let output = Path::new(&output_filename);
        let mut components = output.components();
        if output.is_absolute()
            || !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(BackupError::Other(format!(
                "output_filename immagine disco non valido: '{output_filename}'"
            )));
        }
        Ok(Self {
            source: source.into(),
            output_filename,
        })
    }
}

#[async_trait]
impl Source for DiskImageSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        tokio::fs::create_dir_all(staging).await?;
        let output = staging.join(&self.output_filename);
        let mut input = tokio::fs::File::open(&self.source).await.map_err(|error| {
            BackupError::Other(format!(
                "impossibile aprire immagine/device '{}': {error}",
                self.source.display()
            ))
        })?;
        let mut destination = tokio::fs::File::create(&output).await?;
        let bytes = tokio::io::copy(&mut input, &mut destination).await?;
        Ok(Artifact {
            path: staging.to_path_buf(),
            bytes,
            files: 1,
            checksum: String::new(),
        })
    }

    async fn cleanup(&self, artifact: &Artifact) -> Result<(), BackupError> {
        if tokio::fs::metadata(&artifact.path).await.is_ok() {
            tokio::fs::remove_dir_all(&artifact.path).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn copies_a_disk_image_byte_for_byte() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.img");
        let staging = temp.path().join("staging");
        tokio::fs::write(&source, b"disk-image-data").await.unwrap();

        let artifact = DiskImageSource::new(&source, "disk.img")
            .unwrap()
            .prepare(&staging)
            .await
            .unwrap();

        assert_eq!(artifact.bytes, 15);
        assert_eq!(
            tokio::fs::read(staging.join("disk.img")).await.unwrap(),
            b"disk-image-data"
        );
    }
}
