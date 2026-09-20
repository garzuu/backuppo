use std::path::Path;

use backupper_core::error::BackupError;
use opendal::services::Fs;
use opendal::Operator;

use crate::opendal_backed::{to_backup_error, OpendalDestination};

/// Costruisce una destination locale (opendal, servizio `fs`), senza retry
/// né limite di banda: I/O locale, non ha senso applicarli.
pub fn build(root: impl AsRef<Path>) -> Result<OpendalDestination, BackupError> {
    std::fs::create_dir_all(&root)?;
    let builder = Fs::default().root(&root.as_ref().to_string_lossy());
    let op = Operator::new(builder).map_err(to_backup_error)?;
    Ok(OpendalDestination::new(op, &Default::default(), None))
}
