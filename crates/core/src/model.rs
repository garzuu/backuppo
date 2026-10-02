use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Il risultato della preparazione di un `Source`: i dati pronti per essere
/// caricati su una `Destination`.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub checksum: String,
}

/// Riferimento uniforme a un backup, indipendente dal motore che lo ha
/// prodotto. `id` e' il nome dell'oggetto per gli archivi e lo snapshot ID
/// per Restic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupRef {
    pub job: String,
    pub engine: String,
    pub id: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

/// Una chiave Restic che protegge la master key del repository (`restic key
/// list`). Rimuovere una chiave non tocca i dati gia' scritti: le master key
/// restano le stesse, cambia solo quali password possono sbloccarle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryKey {
    pub id: String,
    pub user_name: String,
    pub host_name: String,
    pub created: String,
    #[serde(default)]
    pub current: bool,
}

/// Elemento mostrato durante la navigazione del contenuto di un backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupEntry {
    pub path: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    Never,
    Always,
}

/// Richiesta di restore condivisa da CLI, API locale e futuro hub.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreRequest {
    /// Snapshot ID/nome archivio oppure `latest`.
    pub snapshot: String,
    pub target: PathBuf,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default = "default_overwrite_policy")]
    pub overwrite: OverwritePolicy,
}

fn default_overwrite_policy() -> OverwritePolicy {
    OverwritePolicy::Never
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreResult {
    pub snapshot: String,
    pub target: PathBuf,
    pub files: u64,
    pub bytes: u64,
    pub dry_run: bool,
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
