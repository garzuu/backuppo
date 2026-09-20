use std::path::Path;

use backuppo_core::error::BackupError;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::archive;

/// Nome del file (nascosto) che accompagna ogni archivio con l'elenco dei
/// file sorgente e il loro checksum, usato da `verify` per controllare che
/// il restore sia integro.
pub const MANIFEST_FILENAME: &str = ".backuppo-manifest.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub files: Vec<ManifestEntry>,
}

/// Calcola il manifest per tutti i file presenti sotto `staging`. Va chiamata
/// prima di [`write`] e prima che l'archivio venga costruito. I/O bloccante.
pub fn build(staging: &Path) -> Result<Manifest, BackupError> {
    let mut files = Vec::new();

    for entry in WalkDir::new(staging).into_iter() {
        let entry = entry.map_err(|e| {
            BackupError::Other(format!("errore leggendo '{}': {e}", staging.display()))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }

        let rel = entry.path().strip_prefix(staging).unwrap_or(entry.path());
        let path = rel.to_string_lossy().replace('\\', "/");
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let sha256 = archive::sha256_file(entry.path())?;

        files.push(ManifestEntry { path, size, sha256 });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Manifest { files })
}

/// Scrive il manifest come file nascosto dentro `staging`, cosicché venga
/// incluso nell'archivio insieme ai dati. I/O bloccante.
pub fn write(staging: &Path, manifest: &Manifest) -> Result<(), BackupError> {
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|e| BackupError::Other(format!("errore serializzazione manifest: {e}")))?;
    std::fs::write(staging.join(MANIFEST_FILENAME), json)?;
    Ok(())
}

/// Legge il manifest da una cartella già estratta da un archivio. I/O
/// bloccante.
pub fn read(extracted: &Path) -> Result<Manifest, BackupError> {
    let bytes = std::fs::read(extracted.join(MANIFEST_FILENAME)).map_err(|e| {
        BackupError::Other(format!(
            "manifest '{}' mancante nell'archivio ripristinato: {e}",
            MANIFEST_FILENAME
        ))
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| BackupError::Other(format!("manifest corrotto o illeggibile: {e}")))
}
