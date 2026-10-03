use std::path::Path;

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use tokio::process::Command;

/// VM vSphere/ESXi spenta: esporta OVF+VMDK con `govc export.ovf`.
/// L'autenticazione (`GOVC_URL`/`GOVC_USERNAME`/`GOVC_PASSWORD`/
/// `GOVC_INSECURE`) resta interamente nell'ambiente del processo agent:
/// govc la legge già da solo, non serve un campo config dedicato.
pub struct VmwareVmSource {
    vm: String,
    govc_binary: String,
}

impl VmwareVmSource {
    pub fn new(vm: impl Into<String>, govc_binary: impl Into<String>) -> Self {
        Self {
            vm: vm.into(),
            govc_binary: govc_binary.into(),
        }
    }

    async fn govc(&self, arguments: &[&str]) -> Result<Vec<u8>, BackupError> {
        let output = Command::new(&self.govc_binary)
            .args(arguments)
            .output()
            .await
            .map_err(|error| {
                BackupError::Other(format!(
                    "impossibile avviare '{}': {error}",
                    self.govc_binary
                ))
            })?;
        if !output.status.success() {
            return Err(BackupError::Other(format!(
                "govc {} fallito per VM '{}': {}",
                arguments.join(" "),
                self.vm,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    /// Estrae il valore della riga "Power state:" dall'output testuale di
    /// `govc vm.info` (non `-json`: lo schema JSON di vSphere è complesso e
    /// interno, il riepilogo testuale è un'interfaccia CLI stabile).
    fn power_state(info: &str) -> Option<&str> {
        info.lines().find_map(|line| {
            let (label, value) = line.split_once(':')?;
            (label.trim() == "Power state").then(|| value.trim())
        })
    }
}

#[async_trait]
impl Source for VmwareVmSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        let info = self.govc(&["vm.info", "-vm", &self.vm]).await?;
        let info = String::from_utf8_lossy(&info);
        let state = Self::power_state(&info).unwrap_or("sconosciuto");
        if state != "poweredOff" {
            return Err(BackupError::Other(format!(
                "VM vSphere '{}' deve essere spenta per un export consistente (stato attuale: {state})",
                self.vm
            )));
        }

        tokio::fs::create_dir_all(staging).await?;
        let staging_str = staging.to_string_lossy().into_owned();
        self.govc(&["export.ovf", "-vm", &self.vm, &staging_str])
            .await?;

        let mut files = 0u64;
        let mut bytes = 0u64;
        for entry in walkdir::WalkDir::new(staging) {
            let entry = entry
                .map_err(|error| BackupError::Other(format!("lettura export OVF: {error}")))?;
            if entry.file_type().is_file() {
                files += 1;
                bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            }
        }
        if files == 0 {
            return Err(BackupError::Other(format!(
                "govc export.ovf non ha prodotto alcun file per la VM '{}' in '{}'",
                self.vm,
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

    fn write_fake_govc(path: &Path, power_state: &str) {
        let script = format!(
            r#"#!/bin/sh
case "$1" in
  vm.info)
    echo "Name:           app01"
    echo "  Power state:  {power_state}"
    ;;
  export.ovf)
    dir="$4"
    echo "ovf-data" > "$dir/app01.ovf"
    echo "vmdk-data" > "$dir/app01-disk1.vmdk"
    ;;
  *) exit 2 ;;
esac
"#
        );
        std::fs::write(path, script).unwrap();
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[tokio::test]
    async fn exports_ovf_and_vmdk_for_a_powered_off_vm() {
        let temp = tempfile::tempdir().unwrap();
        let govc = temp.path().join("govc");
        let staging = temp.path().join("staging");
        write_fake_govc(&govc, "poweredOff");

        let artifact = VmwareVmSource::new("app01", govc.to_string_lossy())
            .prepare(&staging)
            .await
            .unwrap();

        assert_eq!(artifact.files, 2);
        assert_eq!(
            tokio::fs::read(staging.join("app01.ovf")).await.unwrap(),
            b"ovf-data\n"
        );
    }

    #[tokio::test]
    async fn refuses_to_export_a_powered_on_vm_without_calling_export() {
        let temp = tempfile::tempdir().unwrap();
        let govc = temp.path().join("govc");
        let staging = temp.path().join("staging");
        write_fake_govc(&govc, "poweredOn");

        let error = VmwareVmSource::new("app01", govc.to_string_lossy())
            .prepare(&staging)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("deve essere spenta"));
        assert!(!staging.exists(), "export.ovf non doveva essere invocato");
    }
}
