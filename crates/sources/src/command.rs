use std::path::Path;

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use tokio::process::Command;

/// Sorgente generica: esegue un comando e ne salva lo stdout come unico
/// file dell'archivio (es. un tool di dump non ancora supportato
/// nativamente).
pub struct CommandSource {
    command: String,
    args: Vec<String>,
    output_filename: String,
}

impl CommandSource {
    pub fn new(
        command: impl Into<String>,
        args: Vec<String>,
        output_filename: impl Into<String>,
    ) -> Result<Self, BackupError> {
        let output_filename = output_filename.into();
        let output_path = Path::new(&output_filename);
        if output_path.is_absolute()
            || output_path.components().count() != 1
            || output_filename == "."
            || output_filename == ".."
        {
            return Err(BackupError::Other(format!(
                "output_filename '{output_filename}' deve essere un semplice nome di file"
            )));
        }

        Ok(Self {
            command: command.into(),
            args,
            output_filename,
        })
    }
}

#[async_trait]
impl Source for CommandSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        tokio::fs::create_dir_all(staging).await?;

        let output = Command::new(&self.command)
            .args(&self.args)
            .output()
            .await
            .map_err(|e| {
                BackupError::Other(format!(
                    "impossibile eseguire il comando '{}': {e}",
                    self.command
                ))
            })?;

        if !output.status.success() {
            return Err(BackupError::Other(format!(
                "il comando '{}' è uscito con {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let dest = staging.join(&self.output_filename);
        tokio::fs::write(&dest, &output.stdout).await?;

        Ok(Artifact {
            path: staging.to_path_buf(),
            bytes: output.stdout.len() as u64,
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
    async fn captures_stdout_in_staging() {
        let staging = tempfile::tempdir().expect("tempdir");
        let output = staging.path().join("data");
        let source = CommandSource::new(
            if cfg!(windows) { "cmd" } else { "printf" },
            if cfg!(windows) {
                vec!["/C".into(), "<nul set /p=hello".into()]
            } else {
                vec!["hello".into()]
            },
            "dump.txt",
        )
        .expect("config valida");

        let artifact = source.prepare(&output).await.expect("prepare");
        assert_eq!(artifact.files, 1);
        assert_eq!(artifact.bytes, 5);
        assert_eq!(
            tokio::fs::read(output.join("dump.txt")).await.unwrap(),
            b"hello"
        );
    }

    #[test]
    fn rejects_output_outside_staging() {
        assert!(CommandSource::new("tool", Vec::new(), "../secret").is_err());
        assert!(CommandSource::new("tool", Vec::new(), "/tmp/secret").is_err());
    }
}
