use std::path::{Path, PathBuf};

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::traits::Source;
use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

/// Sorgente "cartella": copia in staging tutti i file sotto `root`, escludendo
/// quelli che matchano i pattern glob in `exclude` (un pattern che termina con
/// `/` esclude un'intera sottocartella).
pub struct FolderSource {
    root: PathBuf,
    dir_excludes: Vec<String>,
    file_excludes: GlobSet,
}

impl FolderSource {
    pub fn new(root: impl Into<PathBuf>, exclude: &[String]) -> Result<Self, BackupError> {
        let mut dir_excludes = Vec::new();
        let mut builder = GlobSetBuilder::new();
        for pattern in exclude {
            if let Some(dir) = pattern.strip_suffix('/') {
                dir_excludes.push(dir.to_string());
            } else {
                let glob = Glob::new(pattern).map_err(|e| {
                    BackupError::Other(format!("pattern exclude '{pattern}' non valido: {e}"))
                })?;
                builder.add(glob);
            }
        }
        let file_excludes = builder
            .build()
            .map_err(|e| BackupError::Other(format!("pattern exclude non validi: {e}")))?;

        Ok(Self {
            root: root.into(),
            dir_excludes,
            file_excludes,
        })
    }

    fn is_excluded(&self, rel: &Path) -> bool {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if self.file_excludes.is_match(&rel_str) {
            return true;
        }
        self.dir_excludes
            .iter()
            .any(|dir| rel_str == *dir || rel_str.starts_with(&format!("{dir}/")))
    }

    /// Copia i file non esclusi da `root` a `staging`, mantenendo la
    /// struttura relativa. Ritorna (numero file, byte totali).
    fn copy_tree(&self, staging: &Path) -> Result<(u64, u64), BackupError> {
        let mut files = 0u64;
        let mut bytes = 0u64;

        let walker = WalkDir::new(&self.root).into_iter().filter_entry(|entry| {
            let rel = entry
                .path()
                .strip_prefix(&self.root)
                .unwrap_or(entry.path());
            rel.as_os_str().is_empty() || !self.is_excluded(rel)
        });

        for entry in walker {
            let entry = entry.map_err(|e| {
                BackupError::Other(format!("errore leggendo '{}': {e}", self.root.display()))
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(&self.root)
                .unwrap_or(entry.path());
            if self.is_excluded(rel) {
                continue;
            }

            let dest = staging.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest)?;
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }

        Ok((files, bytes))
    }
}

#[async_trait]
impl Source for FolderSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        tokio::fs::create_dir_all(staging).await?;

        let root = self.root.clone();
        let dir_excludes = self.dir_excludes.clone();
        let file_excludes = self.file_excludes.clone();
        let staging_path = staging.to_path_buf();
        let staging_for_task = staging_path.clone();

        let (files, bytes) = tokio::task::spawn_blocking(move || {
            FolderSource {
                root,
                dir_excludes,
                file_excludes,
            }
            .copy_tree(&staging_for_task)
        })
        .await
        .map_err(|e| BackupError::Other(format!("task di copia interrotto: {e}")))??;

        Ok(Artifact {
            path: staging_path,
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
