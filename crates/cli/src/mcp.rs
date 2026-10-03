//! Server MCP (Model Context Protocol) in sola lettura: espone lo stato
//! dei backup a client come Claude Desktop/Code su stdio. Nessun tool di
//! scrittura (run/verify/restore): è un vincolo di design, non
//! un'omissione temporanea.

use std::sync::Arc;

use anyhow::{Context, Result};
use backuppo_core::config::Config;
use backuppo_engine::history::HistoryStore;
use backuppo_engine::overview::jobs_overview;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) async fn run(config: Config) -> Result<()> {
    let server = BackuppoMcp {
        config: Arc::new(config),
    };
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .context("avvio server MCP fallito")?;
    service.waiting().await.context("server MCP interrotto")?;
    Ok(())
}

#[derive(Serialize, JsonSchema)]
struct JobSummary {
    job: String,
    schedule: String,
    /// "success" | "failure" | "running" | "never_run"
    status: String,
    started_at: Option<i64>,
    /// Esito dell'ultima verifica di restore, se mai eseguita.
    verification_status: Option<String>,
}

#[derive(Serialize, JsonSchema)]
struct ExecutionSummary {
    id: i64,
    kind: String,
    status: String,
    started_at: i64,
    finished_at: Option<i64>,
    duration_ms: Option<i64>,
    bytes: Option<u64>,
    error: Option<String>,
    verification_status: String,
}

#[derive(Deserialize, JsonSchema)]
struct JobHistoryRequest {
    /// Nome del job (come in `jobs:` nella config).
    job: String,
    /// Numero massimo di esecuzioni da restituire (default 20).
    limit: Option<usize>,
}

#[derive(Clone)]
struct BackuppoMcp {
    config: Arc<Config>,
}

impl BackuppoMcp {
    fn history(&self) -> Result<HistoryStore, ErrorData> {
        let settings = self.config.observability.as_ref().ok_or_else(|| {
            ErrorData::internal_error("observability non configurata in questa config", None)
        })?;
        HistoryStore::open(&settings.history_path)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))
    }
}

fn job_status_label(status: &str) -> &'static str {
    match status {
        "success" => "success",
        "failure" => "failure",
        "running" => "running",
        _ => "never_run",
    }
}

#[tool_router]
impl BackuppoMcp {
    #[tool(description = "Elenca i job configurati con schedule e ultimo esito noto.")]
    async fn list_jobs(&self) -> Result<Json<Vec<JobSummary>>, ErrorData> {
        let store = self.history()?;
        let overview = jobs_overview(&self.config, &store)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(Json(
            overview
                .into_iter()
                .map(|item| JobSummary {
                    job: item.job,
                    schedule: item.schedule,
                    status: item
                        .latest
                        .as_ref()
                        .map(|record| job_status_label(&record.status).to_string())
                        .unwrap_or_else(|| "never_run".to_string()),
                    started_at: item.latest.as_ref().map(|record| record.started_at),
                    verification_status: item
                        .latest_verification
                        .as_ref()
                        .map(|record| record.verification_status.clone()),
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Job la cui ultima esecuzione o ultima verifica non sono andate a buon fine. Un job mai eseguito non conta come in difficolta'."
    )]
    async fn failing_jobs(&self) -> Result<Json<Vec<JobSummary>>, ErrorData> {
        let store = self.history()?;
        let overview = jobs_overview(&self.config, &store)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(Json(
            overview
                .iter()
                .filter(|item| item.is_failing())
                .map(|item| JobSummary {
                    job: item.job.clone(),
                    schedule: item.schedule.clone(),
                    status: item
                        .latest
                        .as_ref()
                        .map(|record| job_status_label(&record.status).to_string())
                        .unwrap_or_else(|| "never_run".to_string()),
                    started_at: item.latest.as_ref().map(|record| record.started_at),
                    verification_status: item
                        .latest_verification
                        .as_ref()
                        .map(|record| record.verification_status.clone()),
                })
                .collect(),
        ))
    }

    #[tool(description = "Storico delle esecuzioni recenti di un job (piu' recenti prima).")]
    async fn job_history(
        &self,
        Parameters(request): Parameters<JobHistoryRequest>,
    ) -> Result<Json<Vec<ExecutionSummary>>, ErrorData> {
        if !self.config.jobs.contains_key(&request.job) {
            return Err(ErrorData::invalid_params(
                format!("job '{}' non trovato in config", request.job),
                None,
            ));
        }
        let store = self.history()?;
        let limit = request.limit.unwrap_or(20).clamp(1, 1_000);
        // Stesso compromesso gia' accettato altrove (es. la vecchia
        // implementazione di `/api/v1/status`): `list` non filtra per job,
        // quindi si interroga un lotto ampio e si filtra lato client.
        let records = store
            .list(10_000)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(Json(
            records
                .into_iter()
                .filter(|record| record.job == request.job)
                .take(limit)
                .map(|record| ExecutionSummary {
                    id: record.id,
                    kind: record.kind,
                    status: record.status,
                    started_at: record.started_at,
                    finished_at: record.finished_at,
                    duration_ms: record.duration_ms,
                    bytes: record.bytes,
                    error: record.error,
                    verification_status: record.verification_status,
                })
                .collect(),
        ))
    }
}

#[tool_handler]
impl ServerHandler for BackuppoMcp {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let mut info = rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        );
        info.server_info = rmcp::model::Implementation::new("backuppo", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Interroga lo stato dei backup Backuppo: elenco job, storico esecuzioni, job in \
             difficolta'. Sola lettura: nessun tool avvia, verifica o ripristina backup."
                .to_string(),
        );
        info
    }
}
