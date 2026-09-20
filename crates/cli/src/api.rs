use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use backuppo_core::config::Config;
use backuppo_engine::history::{ExecutionRecord, HistoryStore};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

pub(crate) type JobLocks = Arc<HashMap<String, Arc<Mutex<()>>>>;

#[derive(Clone)]
struct ApiState {
    config: Arc<Config>,
    locks: JobLocks,
}

#[derive(Serialize)]
struct JobStatus {
    job: String,
    latest: Option<ExecutionRecord>,
}

#[derive(Serialize)]
struct RunResponse {
    job: String,
    bytes: u64,
    files: u64,
    checksum: String,
}

#[derive(Deserialize)]
struct RunsQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

type ApiError = (StatusCode, String);

pub(crate) fn build_locks(config: &Config) -> JobLocks {
    Arc::new(
        config
            .jobs
            .keys()
            .map(|name| (name.clone(), Arc::new(Mutex::new(()))))
            .collect(),
    )
}

pub(crate) async fn spawn(
    config: Arc<Config>,
    locks: JobLocks,
) -> Result<Option<tokio::task::JoinHandle<()>>> {
    let Some(settings) = &config.api else {
        return Ok(None);
    };
    let listener = tokio::net::TcpListener::bind(&settings.bind)
        .await
        .with_context(|| format!("impossibile aprire API locale su {}", settings.bind))?;
    let address = listener.local_addr()?;
    let app = router(ApiState { config, locks });
    info!(%address, "API e Web UI locale avviate");
    Ok(Some(tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            warn!(%error, "server API locale terminato con errore");
        }
    })))
}

pub(crate) async fn serve(config: Config) -> Result<()> {
    let config = Arc::new(config);
    let locks = build_locks(&config);
    let handle = spawn(Arc::clone(&config), locks)
        .await?
        .context("la configurazione non contiene la sezione 'api'")?;
    handle.await.context("task API locale interrotto")?;
    Ok(())
}

fn router(state: ApiState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/v1/status", get(status))
        .route("/api/v1/runs", get(runs))
        .route("/api/v1/runs/{id}", get(run_detail))
        .route("/api/v1/jobs/{job}/run", post(run_job))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

fn history(config: &Config) -> Result<HistoryStore, ApiError> {
    let settings = config.observability.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "observability non configurata".to_string(),
    ))?;
    HistoryStore::open(&settings.history_path).map_err(internal_error)
}

async fn status(State(state): State<ApiState>) -> Result<Json<Vec<JobStatus>>, ApiError> {
    let records = history(&state.config)?
        .list(10_000)
        .map_err(internal_error)?;
    let mut jobs: Vec<String> = state.config.jobs.keys().cloned().collect();
    jobs.sort();
    Ok(Json(
        jobs.into_iter()
            .map(|job| JobStatus {
                latest: records
                    .iter()
                    .find(|record| record.job == job && record.kind == "backup")
                    .cloned(),
                job,
            })
            .collect(),
    ))
}

async fn runs(
    State(state): State<ApiState>,
    Query(query): Query<RunsQuery>,
) -> Result<Json<Vec<ExecutionRecord>>, ApiError> {
    let limit = query.limit.clamp(1, 1_000);
    history(&state.config)?
        .list(limit)
        .map(Json)
        .map_err(internal_error)
}

async fn run_detail(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ExecutionRecord>, ApiError> {
    history(&state.config)?
        .get(id)
        .map_err(internal_error)?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "esecuzione non trovata".into()))
}

async fn run_job(
    State(state): State<ApiState>,
    Path(job): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RunResponse>, ApiError> {
    if headers
        .get("x-backuppo-ui")
        .and_then(|value| value.to_str().ok())
        != Some("1")
    {
        return Err((
            StatusCode::FORBIDDEN,
            "header X-Backuppo-UI: 1 richiesto".into(),
        ));
    }
    let lock = state
        .locks
        .get(&job)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "job non trovato".into()))?;
    let _guard = lock.try_lock().map_err(|_| {
        (
            StatusCode::CONFLICT,
            "il job è già in esecuzione".to_string(),
        )
    })?;
    let artifact = backuppo_engine::run_and_retain(&job, &state.config)
        .await
        .map_err(internal_error)?;
    Ok(Json(RunResponse {
        job,
        bytes: artifact.bytes,
        files: artifact.files,
        checksum: artifact.checksum,
    }))
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="it"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Backuppo</title><style>
body{font:15px system-ui;margin:2rem auto;max-width:1000px;padding:0 1rem;background:#111827;color:#e5e7eb}
table{width:100%;border-collapse:collapse;background:#1f2937}th,td{padding:.7rem;border-bottom:1px solid #374151;text-align:left}
button{padding:.45rem .7rem}.success{color:#34d399}.failure{color:#f87171}.running{color:#fbbf24}
</style></head><body><h1>Backuppo</h1><p id="message"></p><table><thead><tr><th>Job</th><th>Stato</th><th>Ultima esecuzione</th><th></th></tr></thead><tbody id="jobs"></tbody></table>
<h2>Storico</h2><table><thead><tr><th>ID</th><th>Job</th><th>Tipo</th><th>Stato</th></tr></thead><tbody id="runs"></tbody></table><h2>Log</h2><pre id="log">Seleziona un'esecuzione.</pre>
<script>
const esc=s=>String(s??'—').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
async function load(){const rows=await fetch('/api/v1/status').then(r=>r.json());jobs.innerHTML=rows.map(x=>`<tr><td>${esc(x.job)}</td><td class="${esc(x.latest?.status)}">${esc(x.latest?.status??'never')}</td><td>${x.latest?new Date(x.latest.started_at*1000).toLocaleString():'—'}</td><td><button onclick="run('${encodeURIComponent(x.job)}')">Esegui</button></td></tr>`).join('');const history=await fetch('/api/v1/runs?limit=50').then(r=>r.json());runs.innerHTML=history.map(x=>`<tr onclick="showLog(${x.id})"><td>${x.id}</td><td>${esc(x.job)}</td><td>${esc(x.kind)}</td><td class="${esc(x.status)}">${esc(x.status)}</td></tr>`).join('')}
async function showLog(id){const item=await fetch('/api/v1/runs/'+id).then(r=>r.json());log.textContent=item.log}
async function run(job){message.textContent='Esecuzione in corso…';const r=await fetch('/api/v1/jobs/'+job+'/run',{method:'POST',headers:{'X-Backuppo-UI':'1'}});message.textContent=r.ok?'Backup completato':await r.text();await load()}load();setInterval(load,10000);
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_lists_configured_jobs_without_history() {
        let temp = tempfile::tempdir().unwrap();
        let yaml = format!(
            r#"
destinations:
  local: {{ type: fs, root: "{}" }}
observability:
  history_path: "{}"
api:
  bind: 127.0.0.1:0
jobs:
  documents:
    source: {{ type: folder, path: "{}" }}
    destination: local
    schedule: "0 3 * * *"
"#,
            temp.path().join("backup").display(),
            temp.path().join("history.sqlite").display(),
            temp.path().join("source").display()
        );
        let config = Arc::new(Config::from_yaml(&yaml).unwrap());
        let state = ApiState {
            locks: build_locks(&config),
            config,
        };

        let response = status(State(state)).await.unwrap();
        assert_eq!(response.0.len(), 1);
        assert_eq!(response.0[0].job, "documents");
        assert!(response.0[0].latest.is_none());
    }
}
