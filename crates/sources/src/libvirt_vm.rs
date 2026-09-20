use std::path::{Path, PathBuf};

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use tokio::process::Command;

pub struct LibvirtVmSource {
    name: String,
    virsh_binary: String,
}

impl LibvirtVmSource {
    pub fn new(name: impl Into<String>, virsh_binary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            virsh_binary: virsh_binary.into(),
        }
    }

    async fn virsh(&self, arguments: &[&str]) -> Result<Vec<u8>, BackupError> {
        let output = Command::new(&self.virsh_binary)
            .env("LC_ALL", "C")
            .args(arguments)
            .output()
            .await
            .map_err(|error| {
                BackupError::Other(format!(
                    "impossibile avviare '{}': {error}",
                    self.virsh_binary
                ))
            })?;
        if !output.status.success() {
            return Err(BackupError::Other(format!(
                "virsh {} fallito per VM '{}': {}",
                arguments.join(" "),
                self.name,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    async fn copy_disk(source: &Path, destination: &Path) -> Result<u64, BackupError> {
        let mut input = tokio::fs::File::open(source).await.map_err(|error| {
            BackupError::Other(format!(
                "impossibile aprire disco VM '{}': {error}",
                source.display()
            ))
        })?;
        let mut output = tokio::fs::File::create(destination).await?;
        Ok(tokio::io::copy(&mut input, &mut output).await?)
    }
}

#[async_trait]
impl Source for LibvirtVmSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        let state = self.virsh(&["domstate", &self.name]).await?;
        let state = String::from_utf8_lossy(&state).trim().to_ascii_lowercase();
        if state != "shut off" {
            return Err(BackupError::Other(format!(
                "VM libvirt '{}' deve essere spenta per un'immagine consistente (stato: {state})",
                self.name
            )));
        }

        tokio::fs::create_dir_all(staging.join("disks")).await?;
        let xml = self.virsh(&["dumpxml", &self.name]).await?;
        tokio::fs::write(staging.join("domain.xml"), &xml).await?;

        let listing = self.virsh(&["domblklist", "--details", &self.name]).await?;
        let listing = String::from_utf8_lossy(&listing);
        let mut files = 1u64;
        let mut bytes = xml.len() as u64;
        for line in listing.lines() {
            let mut columns = line.split_whitespace();
            let Some(_disk_type) = columns.next() else {
                continue;
            };
            let Some(device) = columns.next() else {
                continue;
            };
            let Some(target) = columns.next() else {
                continue;
            };
            let source_value = columns.collect::<Vec<_>>().join(" ");
            if device != "disk" || source_value.is_empty() || source_value == "-" {
                continue;
            }
            let source = PathBuf::from(source_value);
            let suffix = source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("disk.img");
            let destination = staging
                .join("disks")
                .join(format!("{target}-{suffix}"));
            bytes += Self::copy_disk(&source, &destination).await?;
            files += 1;
        }
        if files == 1 {
            return Err(BackupError::Other(format!(
                "nessun disco trovato per la VM libvirt '{}'",
                self.name
            )));
        }

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

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[tokio::test]
    async fn exports_xml_and_disks_for_a_stopped_vm() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("vm.qcow2");
        let virsh = temp.path().join("virsh");
        let staging = temp.path().join("staging");
        tokio::fs::write(&disk, b"qcow-data").await.unwrap();
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  domstate) echo 'shut off' ;;\n  dumpxml) echo '<domain/>' ;;\n  domblklist) printf 'Type Device Target Source\\n--------------------------------\\nfile disk vda {}\\n' ;;\n  *) exit 2 ;;\nesac\n",
            disk.display()
        );
        std::fs::write(&virsh, script).unwrap();
        let mut permissions = std::fs::metadata(&virsh).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&virsh, permissions).unwrap();

        let artifact = LibvirtVmSource::new("test-vm", virsh.to_string_lossy())
            .prepare(&staging)
            .await
            .unwrap();

        assert_eq!(artifact.files, 2);
        assert_eq!(
            tokio::fs::read(staging.join("disks/vda-vm.qcow2"))
                .await
                .unwrap(),
            b"qcow-data"
        );
    }
}
