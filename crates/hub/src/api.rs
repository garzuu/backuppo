use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header::AUTHORIZATION, header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use backuppo_core::config::NotifierConfig;
use backuppo_core::hub_protocol::{
    CommandKind, CommandResult, EventKind, EventPayload, HeartbeatPayload, PendingCommand,
    PolicyConstraints, PolicyDocument, PolicyEnforcement, SignedPolicy,
};
use backuppo_core::model::JobEvent;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

use crate::auth::{self, Claims};
use crate::db::{AgentToken, Command, Customer, Db, NewExecution, Role, Site, UserSummary};
use crate::web::{
    APP_CSS, APP_JS, INDEX_HTML, MANIFEST, SERVICE_WORKER, SQUIRREL_192, SQUIRREL_512,
};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub jwt_secret: Arc<String>,
    pub access_token_minutes: i64,
    pub refresh_token_days: i64,
    pub offline_after_minutes: i64,
    pub notifiers: Arc<HashMap<String, NotifierConfig>>,
    pub notify_on_offline: Arc<Vec<String>>,
    pub notify_on_failure: Arc<Vec<String>>,
    pub browser_csrf: Arc<String>,
    pub policy_signer: Option<Arc<SigningKey>>,
}

type ApiError = (StatusCode, String);

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
        .route("/backuppo-squirrel-192.png", get(squirrel_192))
        .route("/backuppo-squirrel-512.png", get(squirrel_512))
        .route("/api/v1/capabilities", get(capabilities))
        .route(
            "/api/v1/session",
            get(browser_session).delete(browser_logout),
        )
        .route("/api/v1/session/login", post(browser_login))
        .route("/api/v1/session/refresh", post(browser_refresh))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/refresh", post(refresh))
        .route("/v1/events", post(ingest_event))
        .route("/v1/heartbeat", post(heartbeat))
        .route("/v1/customers", get(list_customers).post(create_customer))
        .route("/v1/users", get(list_users).post(create_user))
        .route("/v1/sites", get(list_sites).post(create_site))
        .route("/v1/sites/{id}/executions", get(list_executions_for_site))
        .route("/v1/sites/{id}/jobs", get(list_jobs_for_site))
        .route(
            "/v1/sites/{id}/commands",
            get(list_commands).post(create_command),
        )
        .route("/v1/commands/pending", get(pending_commands))
        .route("/v1/commands/{id}/result", post(command_result))
        .route("/v1/policy/current", get(current_policy))
        .route("/v1/policy/public-key", get(policy_public_key))
        .route(
            "/v1/sites/{id}/policies",
            get(list_policies).post(create_policy),
        )
        .route(
            "/v1/sites/{id}/tokens",
            get(list_agent_tokens).post(create_agent_token),
        )
        .route(
            "/v1/sites/{id}/tokens/{token_id}",
            delete(revoke_agent_token),
        )
        .route(
            "/api/v1/customers",
            get(list_customers).post(create_customer),
        )
        .route("/api/v1/users", get(list_users).post(create_user))
        .route("/api/v1/sites", get(list_sites).post(create_site))
        .route(
            "/api/v1/sites/{id}/executions",
            get(list_executions_for_site),
        )
        .route("/api/v1/sites/{id}/jobs", get(list_jobs_for_site))
        .route(
            "/api/v1/sites/{id}/commands",
            get(list_commands).post(create_command),
        )
        .route(
            "/api/v1/sites/{id}/policies",
            get(list_policies).post(create_policy),
        )
        .route(
            "/api/v1/sites/{id}/tokens",
            get(list_agent_tokens).post(create_agent_token),
        )
        .route(
            "/api/v1/sites/{id}/tokens/{token_id}",
            delete(revoke_agent_token),
        )
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_js() -> impl IntoResponse {
    ([("content-type", "text/javascript; charset=utf-8")], APP_JS)
}

async fn app_css() -> impl IntoResponse {
    ([("content-type", "text/css; charset=utf-8")], APP_CSS)
}

async fn manifest() -> impl IntoResponse {
    ([("content-type", "application/manifest+json")], MANIFEST)
}

async fn service_worker() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-cache"),
        ],
        SERVICE_WORKER,
    )
}

async fn squirrel_192() -> impl IntoResponse {
    ([("content-type", "image/png")], SQUIRREL_192)
}

async fn squirrel_512() -> impl IntoResponse {
    ([("content-type", "image/png")], SQUIRREL_512)
}

#[derive(Serialize)]
struct Capabilities {
    mode: &'static str,
    platform: &'static str,
    api_version: &'static str,
    product_version: &'static str,
    features: Vec<&'static str>,
    auth: &'static str,
}

async fn capabilities() -> Json<Capabilities> {
    Json(Capabilities {
        mode: "hub",
        platform: std::env::consts::OS,
        api_version: "v1",
        product_version: env!("CARGO_PKG_VERSION"),
        features: vec![
            "customers",
            "sites",
            "executions",
            "commands",
            "tokens",
            "signed_policies",
        ],
        auth: "bearer",
    })
}

#[derive(Serialize)]
struct BrowserSession {
    authenticated: bool,
    csrf_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

async fn browser_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<BrowserSession> {
    let claims = require_user(&headers, &state.jwt_secret).ok();
    Json(BrowserSession {
        authenticated: claims.is_some(),
        csrf_token: state.browser_csrf.as_ref().clone(),
        role: claims.map(|claims| claims.role),
    })
}

async fn browser_login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<BrowserSession>), ApiError> {
    let user = state
        .db
        .user_by_username(&body.username)
        .map_err(internal_error)?
        .filter(|(_, hash, _)| auth::verify_password(&body.password, hash))
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "credenziali non valide".to_string(),
        ))?;
    let (user_id, _hash, role) = user;
    let Json(tokens) = issue_token_pair(&state, user_id, role)?;
    Ok((
        session_cookies(&state, &tokens)?,
        Json(BrowserSession {
            authenticated: true,
            csrf_token: state.browser_csrf.as_ref().clone(),
            role: Some(role.as_str().into()),
        }),
    ))
}

async fn browser_refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<BrowserSession>), ApiError> {
    require_browser_csrf(&headers, &state)?;
    let refresh_token = cookie(&headers, "backuppo_refresh").ok_or((
        StatusCode::UNAUTHORIZED,
        "refresh cookie mancante".to_string(),
    ))?;
    let hash = auth::hash_opaque_token(refresh_token);
    let (user_id, role) = state
        .db
        .consume_refresh_token(&hash, now())
        .map_err(internal_error)?
        .ok_or((StatusCode::UNAUTHORIZED, "sessione scaduta".to_string()))?;
    let Json(tokens) = issue_token_pair(&state, user_id, role)?;
    Ok((
        session_cookies(&state, &tokens)?,
        Json(BrowserSession {
            authenticated: true,
            csrf_token: state.browser_csrf.as_ref().clone(),
            role: Some(role.as_str().into()),
        }),
    ))
}

async fn browser_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, StatusCode), ApiError> {
    require_browser_csrf(&headers, &state)?;
    if let Some(refresh_token) = cookie(&headers, "backuppo_refresh") {
        let hash = auth::hash_opaque_token(refresh_token);
        let _ = state.db.consume_refresh_token(&hash, now());
    }
    let mut response_headers = HeaderMap::new();
    response_headers.append(
        SET_COOKIE,
        HeaderValue::from_static(
            "backuppo_access=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0",
        ),
    );
    response_headers.append(
        SET_COOKIE,
        HeaderValue::from_static(
            "backuppo_refresh=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0",
        ),
    );
    Ok((response_headers, StatusCode::NO_CONTENT))
}

fn session_cookies(state: &AppState, tokens: &TokenPair) -> Result<HeaderMap, ApiError> {
    let mut headers = HeaderMap::new();
    let access_max_age = state.access_token_minutes * 60;
    let refresh_max_age = state.refresh_token_days * 24 * 3600;
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "backuppo_access={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={access_max_age}",
            tokens.access_token
        ))
        .map_err(internal_error)?,
    );
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "backuppo_refresh={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={refresh_max_age}",
            tokens.refresh_token
        ))
        .map_err(internal_error)?,
    );
    Ok(headers)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    if let Some(token) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        Some(token)
    } else {
        cookie(headers, "backuppo_access")
    }
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|item| {
            let (key, value) = item.trim().split_once('=')?;
            (key == name).then_some(value)
        })
}

fn require_browser_csrf(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    if headers.contains_key(AUTHORIZATION) {
        return Ok(());
    }
    let valid = headers
        .get("x-backuppo-csrf")
        .and_then(|value| value.to_str().ok())
        == Some(state.browser_csrf.as_str());
    if valid {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "token CSRF non valido".into()))
    }
}

/// Autentica una richiesta dell'agent tramite il token opaco del sito.
fn require_agent(headers: &HeaderMap, db: &Db) -> Result<Site, ApiError> {
    let token = bearer_token(headers).ok_or((
        StatusCode::UNAUTHORIZED,
        "header Authorization: Bearer <token agent> richiesto".to_string(),
    ))?;
    let hash = auth::hash_opaque_token(token);
    db.site_for_agent_token(&hash)
        .map_err(internal_error)?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "token agent non valido".to_string(),
        ))
}

/// Autentica una richiesta utente tramite l'access token JWT.
fn require_user(headers: &HeaderMap, secret: &str) -> Result<Claims, ApiError> {
    let token = bearer_token(headers).ok_or((
        StatusCode::UNAUTHORIZED,
        "header Authorization: Bearer <access token> richiesto".to_string(),
    ))?;
    auth::decode_access_token(secret, token).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            "access token non valido o scaduto".to_string(),
        )
    })
}

fn require_admin(claims: &Claims) -> Result<(), ApiError> {
    if claims.role == Role::Admin.as_str() {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "richiesto ruolo admin".to_string()))
    }
}

/// Admin e operator possono chiedere azioni agli agent; read_only no.
fn require_operator(claims: &Claims) -> Result<(), ApiError> {
    if claims.role == Role::Admin.as_str() || claims.role == Role::Operator.as_str() {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "richiesto ruolo admin o operator".to_string(),
        ))
    }
}

// --- autenticazione utenti ------------------------------------------------

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct TokenPair {
    access_token: String,
    refresh_token: String,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    let user = state
        .db
        .user_by_username(&body.username)
        .map_err(internal_error)?
        .filter(|(_, hash, _)| auth::verify_password(&body.password, hash))
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "credenziali non valide".to_string(),
        ))?;
    let (user_id, _hash, role) = user;
    issue_token_pair(&state, user_id, role)
}

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    let hash = auth::hash_opaque_token(&body.refresh_token);
    let (user_id, role) = state
        .db
        .consume_refresh_token(&hash, now())
        .map_err(internal_error)?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "refresh token non valido, scaduto o già usato".to_string(),
        ))?;
    issue_token_pair(&state, user_id, role)
}

fn issue_token_pair(
    state: &AppState,
    user_id: i64,
    role: Role,
) -> Result<Json<TokenPair>, ApiError> {
    let access_token =
        auth::encode_access_token(&state.jwt_secret, user_id, role, state.access_token_minutes)
            .map_err(internal_error)?;
    let (refresh_plaintext, refresh_hash) = auth::generate_opaque_token();
    let expires_at = now() + state.refresh_token_days * 24 * 3600;
    state
        .db
        .store_refresh_token(user_id, &refresh_hash, now(), expires_at)
        .map_err(internal_error)?;
    Ok(Json(TokenPair {
        access_token,
        refresh_token: refresh_plaintext,
    }))
}

// --- ingest dagli agent ----------------------------------------------------

async fn ingest_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<EventPayload>,
) -> Result<StatusCode, ApiError> {
    let site = require_agent(&headers, &state.db)?;
    let received_at = now();
    state
        .db
        .record_execution(&NewExecution {
            site_id: site.id,
            job: payload.job.as_deref(),
            kind: payload.kind.as_str(),
            detail: payload.detail.as_deref(),
            bytes: payload.bytes.map(|value| value as i64),
            files: payload.files.map(|value| value as i64),
            checksum: payload.checksum.as_deref(),
            received_at,
        })
        .map_err(internal_error)?;

    // Un job fallito (backup o verifica restore) riportato da un agent
    // va notificato subito sui canali dell'hub, non solo registrato.
    if payload.kind == EventKind::Failure {
        let event = JobEvent::Failure {
            job: format!("{}/{}", site.name, payload.job.as_deref().unwrap_or("?")),
            error: payload.detail.clone().unwrap_or_default(),
        };
        crate::offline::dispatch(&state.notifiers, &state.notify_on_failure, &event).await;
    }

    Ok(StatusCode::CREATED)
}

async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Option<Json<HeartbeatPayload>>,
) -> Result<StatusCode, ApiError> {
    let site = require_agent(&headers, &state.db)?;
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or(HeartbeatPayload {
            agent_version: "legacy".into(),
            os: "unknown".into(),
            arch: "unknown".into(),
            capabilities: Vec::new(),
        });
    state
        .db
        .record_heartbeat(site.id, now(), &payload)
        .map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// --- clienti ----------------------------------------------------------------

#[derive(Deserialize)]
struct CreateCustomerRequest {
    name: String,
}

async fn list_customers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Customer>>, ApiError> {
    require_user(&headers, &state.jwt_secret)?;
    state.db.list_customers().map(Json).map_err(internal_error)
}

async fn create_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateCustomerRequest>,
) -> Result<Json<Customer>, ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    state
        .db
        .create_customer(&body.name)
        .map(Json)
        .map_err(internal_error)
}

// --- utenti --------------------------------------------------------------

#[derive(Deserialize)]
struct CreateUserRequest {
    username: String,
    password: String,
    role: String,
}

async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<UserSummary>>, ApiError> {
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    state.db.list_users().map(Json).map_err(internal_error)
}

async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserSummary>), ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    let username = body.username.trim();
    if username.is_empty() || username.len() > 100 || body.password.len() < 10 {
        return Err((
            StatusCode::BAD_REQUEST,
            "username non valido o password più corta di 10 caratteri".into(),
        ));
    }
    let role =
        Role::parse(&body.role).ok_or((StatusCode::BAD_REQUEST, "ruolo non valido".into()))?;
    let password_hash = auth::hash_password(&body.password).map_err(internal_error)?;
    let id = state
        .db
        .create_user(username, &password_hash, role)
        .map_err(internal_error)?;
    Ok((
        StatusCode::CREATED,
        Json(UserSummary {
            id,
            username: username.into(),
            role: role.as_str().into(),
        }),
    ))
}

// --- siti ---------------------------------------------------------------

#[derive(Deserialize)]
struct CreateSiteRequest {
    customer_id: i64,
    name: String,
}

#[derive(Serialize)]
struct SiteWithToken {
    site: Site,
    agent_token: String,
}

async fn list_sites(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Site>>, ApiError> {
    require_user(&headers, &state.jwt_secret)?;
    state.db.list_sites().map(Json).map_err(internal_error)
}

async fn create_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateSiteRequest>,
) -> Result<Json<SiteWithToken>, ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    let site = state
        .db
        .create_site(body.customer_id, &body.name)
        .map_err(internal_error)?;
    let (plaintext, hash) = auth::generate_opaque_token();
    state
        .db
        .create_agent_token(site.id, &hash, now())
        .map_err(internal_error)?;
    Ok(Json(SiteWithToken {
        site,
        agent_token: plaintext,
    }))
}

#[derive(Serialize)]
struct AgentTokenResponse {
    agent_token: String,
    token_id: i64,
}

async fn list_agent_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
) -> Result<Json<Vec<AgentToken>>, ApiError> {
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    state
        .db
        .get_site(site_id)
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, "sito non trovato".to_string()))?;
    state
        .db
        .list_agent_tokens(site_id)
        .map(Json)
        .map_err(internal_error)
}

async fn create_agent_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
) -> Result<Json<AgentTokenResponse>, ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    state
        .db
        .get_site(site_id)
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, "sito non trovato".to_string()))?;
    let (plaintext, hash) = auth::generate_opaque_token();
    let token_id = state
        .db
        .create_agent_token(site_id, &hash, now())
        .map_err(internal_error)?;
    Ok(Json(AgentTokenResponse {
        agent_token: plaintext,
        token_id,
    }))
}

async fn revoke_agent_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_id, token_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    let revoked = state
        .db
        .revoke_agent_token(site_id, token_id, now())
        .map_err(internal_error)?;
    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            "token non trovato o già revocato".to_string(),
        ))
    }
}

// --- comandi (esegui ora / verifica ora) ----------------------------------

const MAX_JOB_NAME_LEN: usize = 200;
const MAX_RESULT_DETAIL_LEN: usize = 2000;

#[derive(Deserialize)]
struct CreateCommandRequest {
    kind: CommandKind,
    job: String,
}

/// L'utente chiede un'azione: l'agent la ritirerà al prossimo poll (l'hub non
/// si connette mai all'agent). Il comando scade se non viene ritirato in
/// tempo, così un backup non parte ore dopo la richiesta.
async fn create_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
    Json(body): Json<CreateCommandRequest>,
) -> Result<(StatusCode, Json<Command>), ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_operator(&claims)?;
    let job = body.job.trim();
    if job.is_empty() || job.len() > MAX_JOB_NAME_LEN || job.chars().any(char::is_control) {
        return Err((StatusCode::BAD_REQUEST, "nome job non valido".to_string()));
    }
    state
        .db
        .get_site(site_id)
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, "sito non trovato".to_string()))?;
    let created_by = claims
        .sub
        .parse::<i64>()
        .ok()
        .and_then(|id| state.db.username_by_id(id).ok().flatten())
        .unwrap_or_else(|| claims.sub.clone());
    let command = state
        .db
        .create_command(site_id, body.kind.as_str(), job, &created_by, now())
        .map_err(internal_error)?
        .ok_or((
            StatusCode::CONFLICT,
            "esiste già una richiesta uguale in corso per questo job".to_string(),
        ))?;
    Ok((StatusCode::CREATED, Json(command)))
}

async fn list_commands(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
) -> Result<Json<Vec<Command>>, ApiError> {
    require_user(&headers, &state.jwt_secret)?;
    state
        .db
        .list_commands(site_id, 50, now())
        .map(Json)
        .map_err(internal_error)
}

/// Poll dell'agent: ritira (e marca consegnati) i comandi in attesa del suo
/// sito. Non conta come heartbeat.
async fn pending_commands(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PendingCommand>>, ApiError> {
    let site = require_agent(&headers, &state.db)?;
    let claimed = state
        .db
        .claim_commands(site.id, now())
        .map_err(internal_error)?;
    Ok(Json(
        claimed
            .into_iter()
            .filter_map(|command| {
                Some(PendingCommand {
                    id: command.id,
                    kind: CommandKind::parse(&command.kind)?,
                    job: command.job,
                })
            })
            .collect(),
    ))
}

async fn command_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<CommandResult>,
) -> Result<StatusCode, ApiError> {
    let site = require_agent(&headers, &state.db)?;
    let detail = body
        .detail
        .as_deref()
        .map(|d| d.chars().take(MAX_RESULT_DETAIL_LEN).collect::<String>());
    let recorded = state
        .db
        .finish_command(site.id, id, body.ok, detail.as_deref(), now())
        .map_err(internal_error)?;
    if recorded {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            "comando non trovato, non tuo o già concluso".to_string(),
        ))
    }
}

// --- policy centralizzate firmate ----------------------------------------

#[derive(Deserialize)]
struct CreatePolicyRequest {
    expires_at: i64,
    enforcement: PolicyEnforcement,
    #[serde(default)]
    constraints: PolicyConstraints,
}

async fn policy_public_key(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signer = state.policy_signer.as_ref().ok_or((
        StatusCode::NOT_FOUND,
        "firma policy non configurata".to_string(),
    ))?;
    Ok(Json(serde_json::json!({
        "algorithm": "Ed25519",
        "public_key": base64::engine::general_purpose::STANDARD
            .encode(signer.verifying_key().to_bytes())
    })))
}

async fn create_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
    Json(body): Json<CreatePolicyRequest>,
) -> Result<(StatusCode, Json<SignedPolicy>), ApiError> {
    require_browser_csrf(&headers, &state)?;
    let claims = require_user(&headers, &state.jwt_secret)?;
    require_admin(&claims)?;
    state
        .db
        .get_site(site_id)
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, "sito non trovato".to_string()))?;
    let issued_at = now();
    if body.expires_at <= issued_at {
        return Err((
            StatusCode::BAD_REQUEST,
            "expires_at deve essere nel futuro".to_string(),
        ));
    }
    let signer = state.policy_signer.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "policy_signing_key_env non configurato".to_string(),
    ))?;
    let sequence = state
        .db
        .latest_policy_sequence(site_id)
        .map_err(internal_error)?
        .saturating_add(1);
    let document = PolicyDocument {
        sequence,
        site_id,
        issued_at,
        expires_at: body.expires_at,
        enforcement: body.enforcement,
        constraints: body.constraints,
    };
    let payload_bytes = serde_json::to_vec(&document).map_err(internal_error)?;
    let signed = SignedPolicy {
        payload: base64::engine::general_purpose::STANDARD.encode(&payload_bytes),
        signature: base64::engine::general_purpose::STANDARD
            .encode(signer.sign(&payload_bytes).to_bytes()),
    };
    let created_by = claims
        .sub
        .parse::<i64>()
        .ok()
        .and_then(|id| state.db.username_by_id(id).ok().flatten())
        .unwrap_or(claims.sub);
    state
        .db
        .insert_policy(
            site_id,
            sequence,
            &signed.payload,
            &signed.signature,
            &created_by,
            issued_at,
            document.expires_at,
        )
        .map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(signed)))
}

async fn list_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
) -> Result<Json<Vec<SignedPolicy>>, ApiError> {
    require_user(&headers, &state.jwt_secret)?;
    state
        .db
        .list_policies(site_id)
        .map(Json)
        .map_err(internal_error)
}

async fn current_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Option<SignedPolicy>>), ApiError> {
    let site = require_agent(&headers, &state.db)?;
    match state.db.current_policy(site.id).map_err(internal_error)? {
        Some(policy) => Ok((StatusCode::OK, Json(Some(policy)))),
        None => Ok((StatusCode::NO_CONTENT, Json(None))),
    }
}

// --- esecuzioni -----------------------------------------------------------

async fn list_executions_for_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
) -> Result<Json<Vec<crate::db::Execution>>, ApiError> {
    require_user(&headers, &state.jwt_secret)?;
    state
        .db
        .list_executions_for_site(site_id, 200)
        .map(Json)
        .map_err(internal_error)
}

/// Elenco dei job distinti riportati dagli eventi di un sito. Nessuna
/// tabella dedicata: un job "esiste" per l'hub non appena un agent invia
/// il primo evento che lo nomina.
async fn list_jobs_for_site(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_id): Path<i64>,
) -> Result<Json<Vec<String>>, ApiError> {
    require_user(&headers, &state.jwt_secret)?;
    state
        .db
        .list_jobs_for_site(site_id)
        .map(Json)
        .map_err(internal_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::hub_protocol::EventKind;

    fn test_state() -> AppState {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("hub.sqlite");
        // Il TempDir va tenuto in vita per tutta la durata del test: se
        // venisse droppato qui la directory (e il DB SQLite) sparirebbe
        // subito dopo l'inizializzazione dello schema.
        std::mem::forget(temp);
        AppState {
            db: Db::open(db_path).unwrap(),
            jwt_secret: Arc::new("test-secret".to_string()),
            access_token_minutes: 15,
            refresh_token_days: 30,
            offline_after_minutes: 15,
            notifiers: Arc::new(HashMap::new()),
            notify_on_offline: Arc::new(Vec::new()),
            notify_on_failure: Arc::new(Vec::new()),
            browser_csrf: Arc::new("test-csrf".to_string()),
            policy_signer: Some(Arc::new(SigningKey::from_bytes(&[9_u8; 32]))),
        }
    }

    fn auth_header(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn login_issues_tokens_and_rejects_wrong_password() {
        let state = test_state();
        let hash = auth::hash_password("hunter2").unwrap();
        state.db.create_user("admin", &hash, Role::Admin).unwrap();

        let ok = login(
            State(state.clone()),
            Json(LoginRequest {
                username: "admin".to_string(),
                password: "hunter2".to_string(),
            }),
        )
        .await;
        assert!(ok.is_ok());

        let err = login(
            State(state),
            Json(LoginRequest {
                username: "admin".to_string(),
                password: "wrong".to_string(),
            }),
        )
        .await;
        assert_eq!(err.unwrap_err().0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn browser_login_uses_http_only_cookies() {
        let state = test_state();
        let hash = auth::hash_password("very-secret-password").unwrap();
        state.db.create_user("admin", &hash, Role::Admin).unwrap();

        let (response_headers, Json(session)) = browser_login(
            State(state.clone()),
            Json(LoginRequest {
                username: "admin".into(),
                password: "very-secret-password".into(),
            }),
        )
        .await
        .unwrap();

        assert!(session.authenticated);
        let cookies: Vec<_> = response_headers.get_all(SET_COOKIE).iter().collect();
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().all(|value| value
            .to_str()
            .unwrap()
            .contains("HttpOnly; Secure; SameSite=Strict")));
    }

    #[tokio::test]
    async fn only_admin_can_create_customers() {
        let state = test_state();
        let admin_hash = auth::hash_password("secret").unwrap();
        state
            .db
            .create_user("admin", &admin_hash, Role::Admin)
            .unwrap();
        let reader_hash = auth::hash_password("secret").unwrap();
        state
            .db
            .create_user("viewer", &reader_hash, Role::ReadOnly)
            .unwrap();

        let admin_token = auth::encode_access_token(&state.jwt_secret, 1, Role::Admin, 15).unwrap();
        let reader_token =
            auth::encode_access_token(&state.jwt_secret, 2, Role::ReadOnly, 15).unwrap();

        let created = create_customer(
            State(state.clone()),
            auth_header(&admin_token),
            Json(CreateCustomerRequest {
                name: "Acme".to_string(),
            }),
        )
        .await;
        assert!(created.is_ok());

        let forbidden = create_customer(
            State(state),
            auth_header(&reader_token),
            Json(CreateCustomerRequest {
                name: "Beta".to_string(),
            }),
        )
        .await;
        assert_eq!(forbidden.unwrap_err().0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn ingest_event_requires_a_valid_agent_token_and_records_it() {
        let state = test_state();
        let customer = state.db.create_customer("Acme").unwrap();
        let site = state.db.create_site(customer.id, "sede-1").unwrap();
        let (plaintext, hash) = auth::generate_opaque_token();
        state.db.create_agent_token(site.id, &hash, 0).unwrap();

        let rejected = ingest_event(
            State(state.clone()),
            auth_header("token-non-valido"),
            Json(EventPayload {
                kind: EventKind::Success,
                job: Some("documents".to_string()),
                detail: None,
                bytes: Some(10),
                files: Some(1),
                checksum: Some("abc".to_string()),
            }),
        )
        .await;
        assert_eq!(rejected.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let accepted = ingest_event(
            State(state.clone()),
            auth_header(&plaintext),
            Json(EventPayload {
                kind: EventKind::Success,
                job: Some("documents".to_string()),
                detail: None,
                bytes: Some(10),
                files: Some(1),
                checksum: Some("abc".to_string()),
            }),
        )
        .await;
        assert_eq!(accepted.unwrap(), StatusCode::CREATED);

        let executions = state.db.list_executions_for_site(site.id, 10).unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].job.as_deref(), Some("documents"));

        let jobs = list_jobs_for_site(
            State(state.clone()),
            auth_header(
                &auth::encode_access_token(&state.jwt_secret, 1, Role::ReadOnly, 15).unwrap(),
            ),
            Path(site.id),
        )
        .await
        .unwrap();
        assert_eq!(jobs.0, vec!["documents".to_string()]);
    }

    #[tokio::test]
    async fn ingest_event_notifies_configured_channel_on_failure() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let mut state = test_state();
        let mut notifiers = HashMap::new();
        notifiers.insert(
            "ops".to_string(),
            NotifierConfig::Webhook {
                url: format!("{}/hook", server.uri()),
            },
        );
        state.notifiers = Arc::new(notifiers);
        state.notify_on_failure = Arc::new(vec!["ops".to_string()]);

        let customer = state.db.create_customer("Acme").unwrap();
        let site = state.db.create_site(customer.id, "sede-1").unwrap();
        let (plaintext, hash) = auth::generate_opaque_token();
        state.db.create_agent_token(site.id, &hash, 0).unwrap();

        let result = ingest_event(
            State(state.clone()),
            auth_header(&plaintext),
            Json(EventPayload {
                kind: EventKind::Failure,
                job: Some("documents".to_string()),
                detail: Some("disco pieno".to_string()),
                bytes: None,
                files: None,
                checksum: None,
            }),
        )
        .await;
        assert_eq!(result.unwrap(), StatusCode::CREATED);

        // wiremock verifica `.expect(1)` allo smontaggio del MockServer:
        // se la richiesta non arriva il test fallisce qui.
        drop(server);
    }

    #[tokio::test]
    async fn failure_reaches_the_ntfy_topic_with_high_priority() {
        use wiremock::matchers::{body_string_contains, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/backuppo-test"))
            .and(header("Priority", "4"))
            .and(body_string_contains("disco pieno"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let mut state = test_state();
        let mut notifiers = HashMap::new();
        notifiers.insert(
            "push".to_string(),
            NotifierConfig::Ntfy {
                url: server.uri(),
                topic: "backuppo-test".to_string(),
                token_env: None,
            },
        );
        state.notifiers = Arc::new(notifiers);
        state.notify_on_failure = Arc::new(vec!["push".to_string()]);

        let customer = state.db.create_customer("Acme").unwrap();
        let site = state.db.create_site(customer.id, "sede-1").unwrap();
        let (plaintext, hash) = auth::generate_opaque_token();
        state.db.create_agent_token(site.id, &hash, 0).unwrap();

        let result = ingest_event(
            State(state),
            auth_header(&plaintext),
            Json(EventPayload {
                kind: EventKind::Failure,
                job: Some("documents".to_string()),
                detail: Some("disco pieno".to_string()),
                bytes: None,
                files: None,
                checksum: None,
            }),
        )
        .await;
        assert_eq!(result.unwrap(), StatusCode::CREATED);
        drop(server);
    }

    #[tokio::test]
    async fn heartbeat_marks_the_authenticated_site_online() {
        let state = test_state();
        let customer = state.db.create_customer("Acme").unwrap();
        let site = state.db.create_site(customer.id, "sede-1").unwrap();
        let (plaintext, hash) = auth::generate_opaque_token();
        state.db.create_agent_token(site.id, &hash, 0).unwrap();

        let result = heartbeat(
            State(state.clone()),
            auth_header(&plaintext),
            Some(Json(HeartbeatPayload {
                agent_version: "1.2.3".into(),
                os: "linux".into(),
                arch: "x86_64".into(),
                capabilities: vec!["updates".into()],
            })),
        )
        .await;
        assert_eq!(result.unwrap(), StatusCode::NO_CONTENT);
        let refreshed = state.db.get_site(site.id).unwrap().unwrap();
        assert_eq!(refreshed.status, "online");
        assert_eq!(refreshed.agent_version.as_deref(), Some("1.2.3"));
    }

    fn user_headers(state: &AppState, name: &str, role: Role) -> HeaderMap {
        let id = state.db.create_user(name, "hash", role).unwrap();
        let token = auth::encode_access_token(&state.jwt_secret, id, role, 15).unwrap();
        auth_header(&token)
    }

    fn site_with_agent(state: &AppState, name: &str) -> (i64, String) {
        let customer = state
            .db
            .list_customers()
            .unwrap()
            .into_iter()
            .next()
            .unwrap_or_else(|| state.db.create_customer("Acme").unwrap());
        let site = state.db.create_site(customer.id, name).unwrap();
        let (plaintext, hash) = auth::generate_opaque_token();
        state.db.create_agent_token(site.id, &hash, 0).unwrap();
        (site.id, plaintext)
    }

    #[tokio::test]
    async fn admin_publishes_a_signed_policy_scoped_to_the_agent_site() {
        let state = test_state();
        let (site_id, agent_token) = site_with_agent(&state, "policy-site");
        let mut headers = user_headers(&state, "policy-admin", Role::Admin);
        headers.insert("x-backuppo-csrf", "test-csrf".parse().unwrap());
        let (_, Json(created)) = create_policy(
            State(state.clone()),
            headers,
            Path(site_id),
            Json(CreatePolicyRequest {
                expires_at: now() + 3600,
                enforcement: PolicyEnforcement::Block,
                constraints: PolicyConstraints {
                    require_append_only: true,
                    require_object_lock: true,
                    minimum_object_lock_days: Some(30),
                    require_signed_updates: true,
                    allow_remote_commands: false,
                },
            }),
        )
        .await
        .unwrap();
        let payload = base64::engine::general_purpose::STANDARD
            .decode(&created.payload)
            .unwrap();
        let document: PolicyDocument = serde_json::from_slice(&payload).unwrap();
        assert_eq!(document.sequence, 1);
        assert_eq!(document.site_id, site_id);

        let (status, Json(current)) = current_policy(State(state), auth_header(&agent_token))
            .await
            .unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(current.unwrap().payload, created.payload);
    }

    fn run_request(job: &str) -> Json<CreateCommandRequest> {
        Json(CreateCommandRequest {
            kind: CommandKind::Run,
            job: job.to_string(),
        })
    }

    #[tokio::test]
    async fn only_admin_and_operator_can_create_commands() {
        let state = test_state();
        let (site, _) = site_with_agent(&state, "sede-1");

        let read_only = user_headers(&state, "lettore", Role::ReadOnly);
        let denied = create_command(
            State(state.clone()),
            read_only,
            Path(site),
            run_request("documents"),
        )
        .await;
        assert_eq!(denied.unwrap_err().0, StatusCode::FORBIDDEN);

        for (name, role) in [("op", Role::Operator), ("boss", Role::Admin)] {
            let headers = user_headers(&state, name, role);
            let job = format!("job-{name}");
            let (status, Json(command)) =
                create_command(State(state.clone()), headers, Path(site), run_request(&job))
                    .await
                    .unwrap();
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(command.status, "pending");
            assert_eq!(command.created_by, name);
        }
    }

    #[tokio::test]
    async fn create_command_validates_site_job_and_duplicates() {
        let state = test_state();
        let (site, _) = site_with_agent(&state, "sede-1");
        let admin = user_headers(&state, "boss", Role::Admin);

        let missing = create_command(
            State(state.clone()),
            admin.clone(),
            Path(999),
            run_request("documents"),
        )
        .await;
        assert_eq!(missing.unwrap_err().0, StatusCode::NOT_FOUND);

        for bad in ["", "   ", "a\nb"] {
            let result = create_command(
                State(state.clone()),
                admin.clone(),
                Path(site),
                run_request(bad),
            )
            .await;
            assert_eq!(result.unwrap_err().0, StatusCode::BAD_REQUEST, "{bad:?}");
        }

        let _ = create_command(
            State(state.clone()),
            admin.clone(),
            Path(site),
            run_request("documents"),
        )
        .await
        .unwrap();
        let duplicate = create_command(
            State(state.clone()),
            admin,
            Path(site),
            run_request("documents"),
        )
        .await;
        assert_eq!(duplicate.unwrap_err().0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn agent_polls_its_own_commands_and_reports_the_result() {
        let state = test_state();
        let (site_a, token_a) = site_with_agent(&state, "sede-a");
        let (_site_b, token_b) = site_with_agent(&state, "sede-b");
        let admin = user_headers(&state, "boss", Role::Admin);
        let (_, Json(created)) = create_command(
            State(state.clone()),
            admin.clone(),
            Path(site_a),
            run_request("documents"),
        )
        .await
        .unwrap();

        // Senza token agent: rifiutato. L'agent dell'altro sito non vede nulla.
        let anonymous = pending_commands(State(state.clone()), HeaderMap::new()).await;
        assert_eq!(anonymous.unwrap_err().0, StatusCode::UNAUTHORIZED);
        let Json(other) = pending_commands(State(state.clone()), auth_header(&token_b))
            .await
            .unwrap();
        assert!(other.is_empty());

        let Json(mine) = pending_commands(State(state.clone()), auth_header(&token_a))
            .await
            .unwrap();
        assert_eq!(
            mine,
            vec![PendingCommand {
                id: created.id,
                kind: CommandKind::Run,
                job: "documents".to_string()
            }]
        );
        let Json(again) = pending_commands(State(state.clone()), auth_header(&token_a))
            .await
            .unwrap();
        assert!(again.is_empty(), "un comando si ritira una volta sola");

        // L'agent dell'altro sito non può chiudere il comando.
        let forged = command_result(
            State(state.clone()),
            auth_header(&token_b),
            Path(created.id),
            Json(CommandResult {
                ok: true,
                detail: None,
            }),
        )
        .await;
        assert_eq!(forged.unwrap_err().0, StatusCode::NOT_FOUND);

        let done = command_result(
            State(state.clone()),
            auth_header(&token_a),
            Path(created.id),
            Json(CommandResult {
                ok: false,
                detail: Some("disco pieno".to_string()),
            }),
        )
        .await;
        assert_eq!(done.unwrap(), StatusCode::NO_CONTENT);

        let Json(listed) = list_commands(State(state.clone()), admin, Path(site_a))
            .await
            .unwrap();
        assert_eq!(listed[0].status, "failed");
        assert_eq!(listed[0].detail.as_deref(), Some("disco pieno"));
    }
}
