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
    Success {
        job: String,
        artifact: Artifact,
    },
    Failure {
        job: String,
        error: String,
    },
    RestoreVerified {
        job: String,
        detail: String,
    },
    Report {
        summary: String,
    },
    /// Un sito non manda heartbeat da oltre `minutes` minuti (solo hub).
    SiteOffline {
        site: String,
        minutes: i64,
    },
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
            JobEvent::SiteOffline { site, minutes } => write!(
                f,
                "sito '{site}' offline: nessun heartbeat da oltre {minutes} {}",
                if *minutes == 1 { "minuto" } else { "minuti" }
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_offline_message_is_worded_for_a_site_and_pluralizes() {
        let one = JobEvent::SiteOffline {
            site: "sede-b".to_string(),
            minutes: 1,
        };
        assert_eq!(
            one.to_string(),
            "sito 'sede-b' offline: nessun heartbeat da oltre 1 minuto"
        );
        let many = JobEvent::SiteOffline {
            site: "sede-b".to_string(),
            minutes: 15,
        };
        assert_eq!(
            many.to_string(),
            "sito 'sede-b' offline: nessun heartbeat da oltre 15 minuti"
        );
    }
}
