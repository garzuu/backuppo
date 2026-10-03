use std::path::Path;

use async_trait::async_trait;
use backuppo_core::config::ProxmoxBackupMode;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use tokio::process::Command;

/// VM o container Proxmox: `vzdump` rileva da solo se il vmid indicato è un
/// QEMU o un LXC, quindi non serve distinguerli in config. Scrive
/// l'archivio direttamente nella directory di staging con `--dumpdir`:
/// nessuna copia manuale, a differenza di `LibvirtVmSource` che deve
/// copiare ogni disco a mano.
pub struct ProxmoxVmSource {
    vmid: u32,
    mode: ProxmoxBackupMode,
    vzdump_binary: String,
}

impl ProxmoxVmSource {
    pub fn new(vmid: u32, mode: ProxmoxBackupMode, vzdump_binary: impl Into<String>) -> Self {
        Self {
            vmid,
            mode,
            vzdump_binary: vzdump_binary.into(),
        }
    }

    fn mode_arg(&self) -> &'static str {
        match self.mode {
            ProxmoxBackupMode::Snapshot => "snapshot",
            ProxmoxBackupMode::Suspend => "suspend",
            ProxmoxBackupMode::Stop => "stop",
        }
    }
}

#[async_trait]
impl Source for ProxmoxVmSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        tokio::fs::create_dir_all(staging).await?;

        let vmid = self.vmid.to_string();
        let dumpdir = staging.to_string_lossy().into_owned();
        let output = Command::new(&self.vzdump_binary)
            .env("LC_ALL", "C")
            .args([
                vmid.as_str(),
                "--mode",
                self.mode_arg(),
                "--dumpdir",
                dumpdir.as_str(),
            ])
            .output()
            .await
            .map_err(|error| {
                BackupError::Other(format!(
                    "impossibile avviare '{}': {error}",
                    self.vzdump_binary
                ))
            })?;
        if !output.status.success() {
            return Err(BackupError::Other(format!(
                "vzdump fallito per il vmid {}: {}",
                self.vmid,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let mut files = 0u64;
        let mut bytes = 0u64;
        for entry in walkdir::WalkDir::new(staging) {
            let entry =
                entry.map_err(|error| BackupError::Other(format!("lettura dumpdir: {error}")))?;
            if entry.file_type().is_file() {
                files += 1;
                bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            }
        }
        if files == 0 {
            return Err(BackupError::Other(format!(
                "vzdump non ha prodotto alcun file per il vmid {} in '{}'",
                self.vmid,
                staging.display()
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
    async fn runs_vzdump_with_dumpdir_and_counts_the_resulting_files() {
        let temp = tempfile::tempdir().unwrap();
        let vzdump = temp.path().join("vzdump");
        let staging = temp.path().join("staging");
        // Il finto vzdump scrive l'archivio (e un .log, come fa il vero
        // vzdump) direttamente nella dumpdir passata con --dumpdir.
        let script = r#"#!/bin/sh
dumpdir=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dumpdir) dumpdir="$2"; shift 2 ;;
    --mode) shift 2 ;;
    *) shift ;;
  esac
done
echo "vma-data" > "$dumpdir/vzdump-qemu-100-test.vma.zst"
echo "log" > "$dumpdir/vzdump-qemu-100-test.log"
"#;
        std::fs::write(&vzdump, script).unwrap();
        let mut permissions = std::fs::metadata(&vzdump).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&vzdump, permissions).unwrap();

        let artifact =
            ProxmoxVmSource::new(100, ProxmoxBackupMode::Snapshot, vzdump.to_string_lossy())
                .prepare(&staging)
                .await
                .unwrap();

        assert_eq!(artifact.files, 2);
        assert_eq!(
            tokio::fs::read(staging.join("vzdump-qemu-100-test.vma.zst"))
                .await
                .unwrap(),
            b"vma-data\n"
        );
    }

    #[tokio::test]
    async fn fails_clearly_when_vzdump_exits_with_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let vzdump = temp.path().join("vzdump");
        let staging = temp.path().join("staging");
        std::fs::write(&vzdump, "#!/bin/sh\necho 'no such vmid' >&2\nexit 2\n").unwrap();
        let mut permissions = std::fs::metadata(&vzdump).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&vzdump, permissions).unwrap();

        let error =
            ProxmoxVmSource::new(999, ProxmoxBackupMode::Snapshot, vzdump.to_string_lossy())
                .prepare(&staging)
                .await
                .unwrap_err();

        assert!(error.to_string().contains("no such vmid"));
    }
}
