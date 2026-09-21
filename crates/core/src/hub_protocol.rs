//! Tipi condivisi tra il client hub dell'agent (`backuppo-notifiers`,
//! dietro la feature `hub`) e il binario `backuppo-hub`: la forma dei
//! payload scambiati su `/v1/events` e `/v1/heartbeat`. Solo metadati, mai
//! contenuto dei backup né segreti.

use serde::{Deserialize, Serialize};

use crate::model::JobEvent;

/// Corpo di `POST /v1/events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub kind: EventKind,
    #[serde(default)]
    pub job: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub files: Option<u64>,
    #[serde(default)]
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Success,
    Failure,
    RestoreVerified,
    Report,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Success => "success",
            EventKind::Failure => "failure",
            EventKind::RestoreVerified => "restore_verified",
            EventKind::Report => "report",
        }
    }
}

impl From<&JobEvent> for EventPayload {
    fn from(event: &JobEvent) -> Self {
        match event {
            JobEvent::Success { job, artifact } => Self {
                kind: EventKind::Success,
                job: Some(job.clone()),
                detail: None,
                bytes: Some(artifact.bytes),
                files: Some(artifact.files),
                checksum: Some(artifact.checksum.clone()),
            },
            JobEvent::Failure { job, error } => Self {
                kind: EventKind::Failure,
                job: Some(job.clone()),
                detail: Some(error.clone()),
                bytes: None,
                files: None,
                checksum: None,
            },
            JobEvent::RestoreVerified { job, detail } => Self {
                kind: EventKind::RestoreVerified,
                job: Some(job.clone()),
                detail: Some(detail.clone()),
                bytes: None,
                files: None,
                checksum: None,
            },
            JobEvent::Report { summary } => Self {
                kind: EventKind::Report,
                job: None,
                detail: Some(summary.clone()),
                bytes: None,
                files: None,
                checksum: None,
            },
            // Evento generato solo dall'hub: un agent non lo emette mai. Per
            // completezza viaggia come fallimento del "job" con il nome del sito.
            JobEvent::SiteOffline { site, .. } => Self {
                kind: EventKind::Failure,
                job: Some(site.clone()),
                detail: Some(event.to_string()),
                bytes: None,
                files: None,
                checksum: None,
            },
        }
    }
}

/// Azione che l'hub può chiedere a un agent. Solo queste due: l'agent non
/// esegue mai comandi arbitrari, solo job già definiti nella sua config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    /// Esegue il backup del job (con retention se riesce).
    Run,
    /// Verifica il restore dell'ultimo backup del job.
    Verify,
}

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CommandKind::Run => "run",
            CommandKind::Verify => "verify",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "run" => Some(CommandKind::Run),
            "verify" => Some(CommandKind::Verify),
            _ => None,
        }
    }
}

/// Elemento di `GET /v1/commands/pending`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCommand {
    pub id: i64,
    pub kind: CommandKind,
    pub job: String,
}

/// Corpo di `POST /v1/commands/{id}/result`: l'esito riportato dall'agent.
/// Il dettaglio dell'esecuzione arriva comunque come evento normale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub ok: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_kind_wire_format_is_snake_case() {
        let command = PendingCommand {
            id: 7,
            kind: CommandKind::Verify,
            job: "documents".to_string(),
        };
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(json, r#"{"id":7,"kind":"verify","job":"documents"}"#);
        assert_eq!(CommandKind::parse("run"), Some(CommandKind::Run));
        assert_eq!(CommandKind::parse("rm -rf"), None);
    }
}
