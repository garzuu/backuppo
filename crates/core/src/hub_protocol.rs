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
        }
    }
}
