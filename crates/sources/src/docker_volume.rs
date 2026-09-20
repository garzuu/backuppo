use std::path::Path;

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use bollard::models::{HostConfig, Mount, MountType};

use crate::docker;

const HELPER_IMAGE: &str = "alpine:3.22";

pub struct DockerVolumeSource {
    volume: String,
}

impl DockerVolumeSource {
    pub fn new(volume: impl Into<String>) -> Self {
        Self {
            volume: volume.into(),
        }
    }
}

#[async_trait]
impl Source for DockerVolumeSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        let client = docker::connect()?;
        let name = docker::unique_container_name("volume");
        let mount = Mount {
            target: Some("/source".to_string()),
            source: Some(self.volume.clone()),
            typ: Some(MountType::VOLUME),
            read_only: Some(true),
            ..Default::default()
        };
        let host_config = HostConfig {
            mounts: Some(vec![mount]),
            ..Default::default()
        };
        docker::run_throwaway_container(
            &client,
            &name,
            HELPER_IMAGE,
            Vec::new(),
            Some(vec!["sleep".into(), "3600".into()]),
            Some(host_config),
        )
        .await?;

        let result = async {
            let archive = docker::exec(
                &client,
                &name,
                vec![
                    "tar".into(),
                    "-C".into(),
                    "/source".into(),
                    "-cf".into(),
                    "-".into(),
                    ".".into(),
                ],
                None,
                None,
            )
            .await?;
            if archive.exit_code != 0 {
                return Err(BackupError::Other(format!(
                    "lettura del volume Docker '{}' fallita: {}",
                    self.volume,
                    String::from_utf8_lossy(&archive.stderr)
                )));
            }
            let staging_path = staging.to_path_buf();
            tokio::task::spawn_blocking(move || unpack_and_measure(&archive.stdout, &staging_path))
                .await
                .map_err(|e| {
                    BackupError::Other(format!("task di estrazione volume interrotto: {e}"))
                })?
        }
        .await;

        docker::remove_container(&client, &name).await;
        let (files, bytes) = result?;
        Ok(Artifact {
            path: staging.to_path_buf(),
            bytes,
            files,
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

fn unpack_and_measure(bytes: &[u8], staging: &Path) -> Result<(u64, u64), BackupError> {
    std::fs::create_dir_all(staging)?;
    let mut archive = tar::Archive::new(bytes);
    archive.unpack(staging)?;

    let mut files = 0;
    let mut total_bytes = 0;
    for entry in walkdir::WalkDir::new(staging) {
        let entry = entry
            .map_err(|e| BackupError::Other(format!("lettura volume estratto fallita: {e}")))?;
        if entry.file_type().is_file() {
            files += 1;
            total_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok((files, total_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::VolumeCreateRequest;

    #[tokio::test]
    #[ignore = "richiede un demone Docker locale e può scaricare alpine:3.22"]
    async fn copies_the_contents_of_a_named_volume() {
        let client = docker::connect().expect("Docker");
        let volume = docker::unique_container_name("test-volume-data");
        client
            .create_volume(VolumeCreateRequest {
                name: Some(volume.clone()),
                ..Default::default()
            })
            .await
            .expect("volume");

        let writer = docker::unique_container_name("test-volume-writer");
        let mount = Mount {
            target: Some("/data".to_string()),
            source: Some(volume.clone()),
            typ: Some(MountType::VOLUME),
            ..Default::default()
        };
        docker::run_throwaway_container(
            &client,
            &writer,
            HELPER_IMAGE,
            Vec::new(),
            Some(vec!["sleep".into(), "3600".into()]),
            Some(HostConfig {
                mounts: Some(vec![mount]),
                ..Default::default()
            }),
        )
        .await
        .expect("container writer");

        let result = async {
            let write = docker::exec(
                &client,
                &writer,
                vec![
                    "sh".into(),
                    "-c".into(),
                    "mkdir -p /data/nested && printf volume-content > /data/nested/file.txt".into(),
                ],
                None,
                None,
            )
            .await?;
            if write.exit_code != 0 {
                return Err(BackupError::Other(format!(
                    "preparazione volume fallita: {}",
                    String::from_utf8_lossy(&write.stderr)
                )));
            }

            let staging = tempfile::tempdir()?;
            let data = staging.path().join("data");
            let artifact = DockerVolumeSource::new(&volume).prepare(&data).await?;
            assert_eq!(artifact.files, 1);
            assert_eq!(artifact.bytes, 14);
            assert_eq!(
                tokio::fs::read(data.join("nested/file.txt")).await?,
                b"volume-content"
            );
            Ok::<(), BackupError>(())
        }
        .await;

        docker::remove_container(&client, &writer).await;
        let _ = client
            .remove_volume(
                &volume,
                None::<bollard::query_parameters::RemoveVolumeOptions>,
            )
            .await;
        result.expect("backup volume Docker");
    }
}
