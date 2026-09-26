use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use backuppo_core::config::{validate, Config};
use backuppo_core::model::JobEvent;
use backuppo_engine::history::{ExecutionRecord, HistoryStore};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use tracing::{info, warn};

pub(crate) type JobLocks = Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>;

#[derive(Clone, Copy)]
pub(crate) enum RuntimeMode {
    Daemon,
    Serve,
}

impl RuntimeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Serve => "serve",
        }
    }
}

#[derive(Clone)]
pub(crate) struct RuntimeState {
    config: Arc<RwLock<Arc<Config>>>,
    config_path: Option<Arc<PathBuf>>,
    locks: JobLocks,
    reload: Option<watch::Sender<Arc<Config>>>,
    mode: RuntimeMode,
}

impl RuntimeState {
    pub(crate) fn new(
        config: Arc<Config>,
        config_path: Option<PathBuf>,
        locks: JobLocks,
        reload: Option<watch::Sender<Arc<Config>>>,
        mode: RuntimeMode,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path: config_path.map(Arc::new),
            locks,
            reload,
            mode,
        }
    }

    pub(crate) fn config(&self) -> Arc<Config> {
        self.config.read().expect("runtime config poisoned").clone()
    }

    fn activate(&self, config: Arc<Config>) {
        let mut locks = self.locks.write().expect("job locks poisoned");
        locks.retain(|name, _| config.jobs.contains_key(name));
        for name in config.jobs.keys() {
            locks
                .entry(name.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())));
        }
        drop(locks);
        *self.config.write().expect("runtime config poisoned") = Arc::clone(&config);
        if let Some(reload) = &self.reload {
            let _ = reload.send(config);
        }
    }
}

#[derive(Clone)]
struct ApiState {
    runtime: RuntimeState,
    operations: Arc<Mutex<HashMap<String, Operation>>>,
    csrf_token: Arc<String>,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    field_errors: Vec<String>,
}

#[derive(Debug)]
struct ApiError(StatusCode, ApiErrorBody);

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self(
            status,
            ApiErrorBody {
                code,
                message: message.into(),
                field_errors: Vec::new(),
            },
        )
    }

    fn validation(errors: Vec<String>) -> Self {
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorBody {
                code: "validation_failed",
                message: "configurazione non valida".into(),
                field_errors: errors,
            },
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

#[derive(Serialize)]
struct Capabilities {
    mode: &'static str,
    api_version: &'static str,
    product_version: &'static str,
    features: Vec<&'static str>,
    auth: &'static str,
}

#[derive(Serialize)]
struct SessionResponse {
    csrf_token: String,
}

#[derive(Serialize)]
struct RuntimeResponse {
    mode: &'static str,
    jobs: usize,
    reports: usize,
    config_path: Option<String>,
}

#[derive(Serialize)]
struct JobStatus {
    job: String,
    schedule: String,
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

#[derive(Debug, Clone, Serialize)]
struct Operation {
    id: String,
    job: String,
    kind: String,
    status: String,
    detail: Option<String>,
    started_at: i64,
    finished_at: Option<i64>,
}

#[derive(Deserialize)]
struct ActionRequest {
    kind: ActionKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Run,
    Verify,
}

impl ActionKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Verify => "verify",
        }
    }
}

#[derive(Deserialize)]
struct ConfigBody {
    yaml: String,
    #[serde(default)]
    revision: Option<String>,
}

#[derive(Serialize)]
struct ConfigResponse {
    yaml: String,
    revision: String,
    active_revision: String,
}

#[derive(Serialize)]
struct ValidationResponse {
    valid: bool,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct SaveResponse {
    revision: String,
    backup_path: String,
}

#[derive(Serialize)]
struct ApplyResponse {
    active_revision: String,
    restart_required: Vec<&'static str>,
}

fn default_limit() -> usize {
    50
}

pub(crate) fn build_locks(config: &Config) -> JobLocks {
    Arc::new(RwLock::new(
        config
            .jobs
            .keys()
            .map(|name| (name.clone(), Arc::new(Mutex::new(()))))
            .collect(),
    ))
}

pub(crate) async fn spawn(runtime: RuntimeState) -> Result<Option<tokio::task::JoinHandle<()>>> {
    let config = runtime.config();
    let Some(settings) = &config.api else {
        return Ok(None);
    };
    let listener = tokio::net::TcpListener::bind(&settings.bind)
        .await
        .with_context(|| format!("impossibile aprire API locale su {}", settings.bind))?;
    let address = listener.local_addr()?;
    let app = router(ApiState {
        runtime,
        operations: Arc::new(Mutex::new(HashMap::new())),
        csrf_token: Arc::new(new_token()),
    });
    info!(%address, "API e Web UI locale avviate");
    Ok(Some(tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            warn!(%error, "server API locale terminato con errore");
        }
    })))
}

pub(crate) async fn serve(config: Config, config_path: PathBuf) -> Result<()> {
    let config = Arc::new(config);
    let locks = build_locks(&config);
    let runtime = RuntimeState::new(config, Some(config_path), locks, None, RuntimeMode::Serve);
    let handle = spawn(runtime)
        .await?
        .context("la configurazione non contiene la sezione 'api'")?;
    handle.await.context("task API locale interrotto")?;
    Ok(())
}

fn router(state: ApiState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
        .route("/api/v1/capabilities", get(capabilities))
        .route("/api/v1/session", get(session))
        .route("/api/v1/runtime", get(runtime_status))
        .route("/api/v1/status", get(status))
        .route("/api/v1/runs", get(runs))
        .route("/api/v1/runs/{id}", get(run_detail))
        .route("/api/v1/jobs/{job}/run", post(run_job_legacy))
        .route("/api/v1/jobs/{job}/actions", post(start_action))
        .route("/api/v1/operations/{id}", get(operation))
        .route("/api/v1/notifiers/{name}/test", post(test_notifier))
        .route("/api/v1/config", get(get_config).put(save_config))
        .route("/api/v1/config/schema", get(config_schema))
        .route("/api/v1/config/validate", post(validate_config))
        .route("/api/v1/config/apply", post(apply_config))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../../../apps/web/dist/index.html"))
}

async fn app_js() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../../../apps/web/dist/app.js"),
    )
}

async fn app_css() -> impl IntoResponse {
    (
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../../../apps/web/dist/app.css"),
    )
}

async fn manifest() -> impl IntoResponse {
    (
        [("content-type", "application/manifest+json")],
        include_str!("../../../apps/web/public/manifest.webmanifest"),
    )
}

async fn service_worker() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-cache"),
        ],
        include_str!("../../../apps/web/public/sw.js"),
    )
}

async fn capabilities(State(_state): State<ApiState>) -> Json<Capabilities> {
    Json(Capabilities {
        mode: "agent",
        api_version: "v1",
        product_version: env!("CARGO_PKG_VERSION"),
        features: vec![
            "jobs",
            "history",
            "run",
            "verify",
            "notifier_test",
            "config",
        ],
        auth: "loopback_csrf",
    })
}

async fn session(State(state): State<ApiState>) -> Json<SessionResponse> {
    Json(SessionResponse {
        csrf_token: state.csrf_token.as_ref().clone(),
    })
}

async fn runtime_status(State(state): State<ApiState>) -> Json<RuntimeResponse> {
    let config = state.runtime.config();
    Json(RuntimeResponse {
        mode: state.runtime.mode.as_str(),
        jobs: config.jobs.len(),
        reports: config.reports.len(),
        config_path: state
            .runtime
            .config_path
            .as_ref()
            .map(|path| path.display().to_string()),
    })
}

fn history(config: &Config) -> Result<HistoryStore, ApiError> {
    let settings = config.observability.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "observability_unavailable",
            "observability non configurata",
        )
    })?;
    HistoryStore::open(&settings.history_path).map_err(internal_error)
}

async fn status(State(state): State<ApiState>) -> Result<Json<Vec<JobStatus>>, ApiError> {
    let config = state.runtime.config();
    let records = history(&config)?.list(10_000).map_err(internal_error)?;
    let mut jobs: Vec<_> = config.jobs.iter().collect();
    jobs.sort_by_key(|(name, _)| *name);
    Ok(Json(
        jobs.into_iter()
            .map(|(job, settings)| JobStatus {
                latest: records
                    .iter()
                    .find(|record| record.job == *job && record.kind == "backup")
                    .cloned(),
                job: job.clone(),
                schedule: settings.schedule.clone(),
            })
            .collect(),
    ))
}

async fn runs(
    State(state): State<ApiState>,
    Query(query): Query<RunsQuery>,
) -> Result<Json<Vec<ExecutionRecord>>, ApiError> {
    let config = state.runtime.config();
    let limit = query.limit.clamp(1, 1_000);
    history(&config)?
        .list(limit)
        .map(Json)
        .map_err(internal_error)
}

async fn run_detail(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
) -> Result<Json<ExecutionRecord>, ApiError> {
    let config = state.runtime.config();
    history(&config)?
        .get(id)
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "esecuzione non trovata"))
}

async fn run_job_legacy(
    State(state): State<ApiState>,
    Path(job): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RunResponse>, ApiError> {
    if headers
        .get("x-backuppo-ui")
        .and_then(|value| value.to_str().ok())
        != Some("1")
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "header X-Backuppo-UI: 1 richiesto",
        ));
    }
    let config = state.runtime.config();
    let lock = job_lock(&state, &job)?;
    let _guard = lock.try_lock().map_err(|_| conflict(&job))?;
    let artifact = backuppo_engine::run_and_retain(&job, &config)
        .await
        .map_err(internal_error)?;
    Ok(Json(RunResponse {
        job,
        bytes: artifact.bytes,
        files: artifact.files,
        checksum: artifact.checksum,
    }))
}

async fn start_action(
    State(state): State<ApiState>,
    Path(job): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ActionRequest>,
) -> Result<(StatusCode, Json<Operation>), ApiError> {
    require_csrf(&state, &headers)?;
    let config = state.runtime.config();
    let lock = job_lock(&state, &job)?;
    let guard = Arc::clone(&lock)
        .try_lock_owned()
        .map_err(|_| conflict(&job))?;

    let id = new_token();
    let kind = body.kind.as_str().to_string();
    let operation = Operation {
        id: id.clone(),
        job: job.clone(),
        kind,
        status: "queued".into(),
        detail: None,
        started_at: now(),
        finished_at: None,
    };
    state
        .operations
        .lock()
        .await
        .insert(id.clone(), operation.clone());

    let operations = Arc::clone(&state.operations);
    tokio::spawn(async move {
        let _guard = guard;
        update_operation(&operations, &id, "running", None, false).await;
        let result = match body.kind {
            ActionKind::Run => {
                backuppo_engine::run_and_retain(&job, &config)
                    .await
                    .map(|artifact| {
                        format!(
                            "backup completato: {} file, {} byte, checksum {}",
                            artifact.files, artifact.bytes, artifact.checksum
                        )
                    })
            }
            ActionKind::Verify => backuppo_engine::verify_job(&job, &config)
                .await
                .map(|event| event.to_string()),
        };
        match result {
            Ok(detail) => update_operation(&operations, &id, "success", Some(detail), true).await,
            Err(error) => {
                update_operation(&operations, &id, "failure", Some(error.to_string()), true).await
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(operation)))
}

async fn operation(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Operation>, ApiError> {
    state
        .operations
        .lock()
        .await
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "operazione non trovata"))
}

async fn test_notifier(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    require_csrf(&state, &headers)?;
    let config = state.runtime.config();
    let notifier_config = config
        .notifiers
        .get(&name)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "notifier non trovato"))?;
    let notifier = backuppo_notifiers::build(notifier_config).map_err(internal_error)?;
    notifier
        .send(&JobEvent::Report {
            summary: "backuppo: messaggio di prova (Web UI)".into(),
        })
        .await
        .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_config(State(state): State<ApiState>) -> Result<Json<ConfigResponse>, ApiError> {
    let path = config_path(&state)?;
    let yaml = std::fs::read_to_string(path.as_ref()).map_err(internal_error)?;
    let active_yaml =
        serde_yaml::to_string(state.runtime.config().as_ref()).map_err(internal_error)?;
    Ok(Json(ConfigResponse {
        revision: revision(&yaml),
        yaml,
        active_revision: revision(&active_yaml),
    }))
}

async fn config_schema() -> Json<schemars::Schema> {
    Json(schemars::schema_for!(Config))
}

async fn validate_config(Json(body): Json<ConfigBody>) -> Json<ValidationResponse> {
    let errors = config_errors(&body.yaml);
    Json(ValidationResponse {
        valid: errors.is_empty(),
        errors,
    })
}

async fn save_config(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<ConfigBody>,
) -> Result<Json<SaveResponse>, ApiError> {
    require_csrf(&state, &headers)?;
    let errors = config_errors(&body.yaml);
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }
    let path = config_path(&state)?;
    let current = std::fs::read_to_string(path.as_ref()).map_err(internal_error)?;
    let current_revision = revision(&current);
    if body.revision.as_deref() != Some(current_revision.as_str()) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            "il file è cambiato dopo l'apertura; ricaricalo prima di salvare",
        ));
    }
    let backup = atomic_write(path.as_ref(), &body.yaml).map_err(internal_error)?;
    Ok(Json(SaveResponse {
        revision: revision(&body.yaml),
        backup_path: backup.display().to_string(),
    }))
}

async fn apply_config(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<ApplyResponse>, ApiError> {
    require_csrf(&state, &headers)?;
    let path = config_path(&state)?;
    let yaml = std::fs::read_to_string(path.as_ref()).map_err(internal_error)?;
    let config = parse_valid_config(&yaml)?;
    let previous_bind = state
        .runtime
        .config()
        .api
        .as_ref()
        .map(|api| api.bind.as_str())
        .unwrap_or_default()
        .to_string();
    let next_bind = config
        .api
        .as_ref()
        .map(|api| api.bind.as_str())
        .unwrap_or_default();
    let restart_required = if previous_bind == next_bind {
        Vec::new()
    } else {
        vec!["api.bind"]
    };
    let active_revision = revision(&yaml);
    state.runtime.activate(Arc::new(config));
    Ok(Json(ApplyResponse {
        active_revision,
        restart_required,
    }))
}

fn config_path(state: &ApiState) -> Result<Arc<PathBuf>, ApiError> {
    state.runtime.config_path.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "config_unavailable",
            "percorso della configurazione non disponibile",
        )
    })
}

fn config_errors(yaml: &str) -> Vec<String> {
    match Config::from_yaml(yaml) {
        Ok(config) => validate(&config)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|error| error.to_string())
            .collect(),
        Err(error) => vec![error.to_string()],
    }
}

fn parse_valid_config(yaml: &str) -> Result<Config, ApiError> {
    let config =
        Config::from_yaml(yaml).map_err(|error| ApiError::validation(vec![error.to_string()]))?;
    if let Err(errors) = validate(&config) {
        return Err(ApiError::validation(
            errors.into_iter().map(|error| error.to_string()).collect(),
        ));
    }
    Ok(config)
}

fn atomic_write(path: &FsPath, contents: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(FsPath::new("."));
    let backup = PathBuf::from(format!("{}.bak", path.display()));
    let permissions = std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    if path.exists() {
        std::fs::copy(path, &backup)
            .with_context(|| format!("creazione backup config '{}'", backup.display()))?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creazione file temporaneo in '{}'", parent.display()))?;
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file().sync_all()?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("sostituzione atomica config '{}'", path.display()))?;
    Ok(backup)
}

fn revision(contents: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    contents.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn job_lock(state: &ApiState, job: &str) -> Result<Arc<Mutex<()>>, ApiError> {
    state
        .runtime
        .locks
        .read()
        .expect("job locks poisoned")
        .get(job)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "job non trovato"))
}

fn require_csrf(state: &ApiState, headers: &HeaderMap) -> Result<(), ApiError> {
    let supplied = headers
        .get("x-backuppo-csrf")
        .and_then(|value| value.to_str().ok());
    if supplied == Some(state.csrf_token.as_str()) {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "csrf_failed",
            "token CSRF mancante o non valido",
        ))
    }
}

fn conflict(job: &str) -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "job_running",
        format!("il job '{job}' è già in esecuzione"),
    )
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        error.to_string(),
    )
}

async fn update_operation(
    operations: &Mutex<HashMap<String, Operation>>,
    id: &str,
    status: &str,
    detail: Option<String>,
    finished: bool,
) {
    if let Some(operation) = operations.lock().await.get_mut(id) {
        operation.status = status.into();
        operation.detail = detail;
        if finished {
            operation.finished_at = Some(now());
        }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn new_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}{:x}", NEXT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_runtime(config: Arc<Config>) -> RuntimeState {
        RuntimeState::new(
            Arc::clone(&config),
            None,
            build_locks(&config),
            None,
            RuntimeMode::Serve,
        )
    }

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
            runtime: test_runtime(config),
            operations: Arc::new(Mutex::new(HashMap::new())),
            csrf_token: Arc::new("test".into()),
        };

        let response = status(State(state)).await.unwrap();
        assert_eq!(response.0.len(), 1);
        assert_eq!(response.0[0].job, "documents");
        assert!(response.0[0].latest.is_none());
    }

    #[test]
    fn rejects_invalid_config_before_writing() {
        assert!(!config_errors("jobs: []").is_empty());
    }

    #[test]
    fn atomic_write_keeps_a_backup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.yaml");
        std::fs::write(&path, "old").unwrap();
        let backup = atomic_write(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "old");
    }

    #[tokio::test]
    async fn activating_config_updates_runtime_and_notifies_daemon() {
        let first = Arc::new(Config::from_yaml("jobs: {}").unwrap());
        let (sender, mut receiver) = watch::channel(Arc::clone(&first));
        let runtime = RuntimeState::new(
            Arc::clone(&first),
            None,
            build_locks(&first),
            Some(sender),
            RuntimeMode::Daemon,
        );
        let second = Arc::new(
            Config::from_yaml(
                r#"
destinations:
  local: { type: fs, root: /tmp/backups }
jobs:
  nightly:
    source: { type: folder, path: /tmp/source }
    destination: local
    schedule: "0 3 * * *"
"#,
            )
            .unwrap(),
        );

        runtime.activate(Arc::clone(&second));
        receiver.changed().await.unwrap();
        assert!(runtime.config().jobs.contains_key("nightly"));
        assert!(receiver.borrow().jobs.contains_key("nightly"));
    }
}
