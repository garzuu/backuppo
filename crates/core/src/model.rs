use std::path::PathBuf;

/// Il risultato della preparazione di un `Source`: i dati pronti per essere
/// caricati su una `Destination`.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub checksum: String,
}

/// Evento emesso dall'engine durante l'esecuzione di un job, inoltrato ai
/// `Notifier` configurati.
#[derive(Debug, Clone)]
pub enum JobEvent {
    Success { job: String, artifact: Artifact },
    Failure { job: String, error: String },
    RestoreVerified { job: String, detail: String },
    Report { summary: String },
}

impl std::fmt::Display for JobEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobEvent::Success { job, artifact } => write!(
                f,
                "job '{job}': backup riuscito ({} file, {} byte, checksum {})",
                artifact.files, artifact.bytes, artifact.checksum
            ),
            JobEvent::Failure { job, error } => write!(f, "job '{job}': backup fallito: {error}"),
            JobEvent::RestoreVerified { job, detail } => {
                write!(f, "job '{job}': restore verificato: {detail}")
            }
            JobEvent::Report { summary } => write!(f, "{summary}"),
        }
    }
}
