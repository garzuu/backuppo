use thiserror::Error;

/// Errore generico restituito dalle implementazioni dei trait di `core`
/// (`Source`, `Destination`, `Notifier`) durante l'esecuzione di un job.
#[derive(Debug, Error)]
pub enum BackupError {
    #[error("errore di I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("variabile d'ambiente '{var}' non impostata (richiesta dal campo '{field}')")]
    MissingEnvVar { field: String, var: String },

    #[error("{0}")]
    Other(String),
}
