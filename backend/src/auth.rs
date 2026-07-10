//! Authentication handlers: provider-blind login flow, dev login, sessions,
//! and the owner-gated skeleton endpoint (Impl Spec §5.2, §5.3).
//!
//! Theorem T2 (docs/LedgerZero_Theorems.md): these handlers contain no
//! provider-specific logic. A new authentication domain is a registry entry;
//! nothing here changes.

use crate::auth_provider::AuthenticatedIdentity;
use crate::authz::Action;
use crate::error::ApiError;
use crate::state::SharedState;
use crate::users::User;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SESSION_COOKIE: &str = "lz_session";
const OAUTH_STATE_TTL: Duration = Duration::from_secs(600);

// ---------- helpers ----------

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(concat!("lz_session", "=")) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn with_session_cookie(mut response: Response, token: &str, ttl_seconds: u64) -> Response {
    // Note for non-local deployments: add "; Secure" once served over HTTPS.
    let cookie =
        format!("{SESSION_COOKIE}={token}; HttpOnly; Path=/; Max-Age={ttl_seconds}; SameSite=Lax");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

fn clear_session_cookie(mut response: Response) -> Response {
    let cookie = format!("{SESSION_COOKIE}=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// Resolve the authenticated user from the session cookie.
pub fn current_user(state: &SharedState, headers: &HeaderMap) -> Result<User, ApiError> {
    let Some(token) = session_token(headers) else {
        return Err(ApiError::unauthenticated("no session"));
    };
    match state.sessions.lookup(&token) {
        Some(user_id) => state
            .users
            .get(user_id)
            .ok_or_else(|| ApiError::unauthenticated("unknown user")),
        None => {
            state.audit.record(
                "session_validation",
                "unknown",
                "denied",
                "invalid or expired session token presented",
            );
            Err(ApiError::unauthenticated("invalid or expired session"))
        }
    }
}

/// Establish a session for a verified external identity. Provider-blind:
/// every authentication domain funnels through here (Theorem T5).
fn establish_session(state: &SharedState, identity: &AuthenticatedIdentity) -> (User, String) {
    let user = state.users.resolve_identity(
        &identity.provider_id,
        &identity.subject,
        &identity.email,
        &identity.display_name,
    );
    let token = state.sessions.create(user.user_id);
    (user, token)
}

// ---------- responses ----------

#[derive(Serialize)]
pub struct MeResponse {
    pub user: User,
    pub is_bootstrap_owner: bool,
    pub allowed_actions: Vec<&'static str>,
}

impl MeResponse {
    fn build(state: &SharedState, user: &User) -> Self {
        Self {
            user: user.clone(),
            is_bootstrap_owner: state.authz.is_bootstrap_owner(user),
            allowed_actions: state.authz.allowed_actions(user),
        }
    }
}

#[derive(Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct AuthConfigResponse {
    pub providers: Vec<ProviderInfo>,
    pub dev_login_enabled: bool,
}

// ---------- handlers ----------

/// GET /api/health — liveness, no auth.
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "engine_version": ledgerzero_engine::ENGINE_VERSION,
    }))
}

/// GET /api/auth/config — login methods the launcher should offer. Read from
/// the registry per-request, so runtime-added domains appear immediately (T3).
pub async fn auth_config(State(state): State<SharedState>) -> Json<AuthConfigResponse> {
    let providers = state
        .providers
        .list()
        .into_iter()
        .map(|(id, display_name)| ProviderInfo { id, display_name })
        .collect();
    Json(AuthConfigResponse {
        providers,
        dev_login_enabled: state.config.dev_login.enabled,
    })
}

/// GET /api/auth/me — identity and authority of the current session.
pub async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let user = current_user(&state, &headers)?;
    Ok(Json(MeResponse::build(&state, &user)))
}

fn unknown_provider(provider_id: &str) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "UNKNOWN_PROVIDER",
        format!("no authentication provider '{provider_id}' is registered"),
    )
}

/// GET /api/auth/{provider}/login — redirect to the domain's consent screen.
pub async fn provider_login(
    State(state): State<SharedState>,
    Path(provider_id): Path<String>,
) -> Result<Response, ApiError> {
    let Some(provider) = state.providers.get(&provider_id) else {
        return Err(unknown_provider(&provider_id));
    };
    let csrf = Uuid::new_v4().to_string();
    {
        let mut states = state.oauth_states.write().expect("oauth state lock");
        let now = Instant::now();
        states.retain(|_, (created, _)| now.duration_since(*created) < OAUTH_STATE_TTL);
        states.insert(csrf.clone(), (now, provider_id.clone()));
    }
    let url = provider.authorization_url(&csrf);
    Ok(Redirect::temporary(&url).into_response())
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// GET /api/auth/{provider}/callback — code exchange, identity, session.
pub async fn provider_callback(
    State(state): State<SharedState>,
    Path(provider_id): Path<String>,
    Query(params): Query<CallbackParams>,
) -> Result<Response, ApiError> {
    let Some(provider) = state.providers.get(&provider_id) else {
        return Err(unknown_provider(&provider_id));
    };
    if let Some(error) = params.error {
        state.audit.record("login", &provider_id, "denied", &error);
        return Ok(Redirect::temporary("/?login_error=denied").into_response());
    }
    let (Some(code), Some(csrf)) = (params.code, params.state) else {
        return Err(ApiError::invalid_input("missing code or state"));
    };
    {
        let mut states = state.oauth_states.write().expect("oauth state lock");
        let valid = states
            .remove(&csrf)
            .map(|(created, for_provider)| {
                created.elapsed() < OAUTH_STATE_TTL && for_provider == provider_id
            })
            .unwrap_or(false);
        if !valid {
            state.audit.record(
                "login",
                &provider_id,
                "denied",
                "unknown, expired, or mismatched OAuth state (possible CSRF)",
            );
            return Err(ApiError::invalid_input("unknown or expired OAuth state"));
        }
    }

    let identity = provider.exchange_code(&code).await?;
    if !identity.email_verified {
        state.audit.record(
            "login",
            &identity.email,
            "denied",
            "email not verified by provider",
        );
        return Err(ApiError::unauthenticated("email is not verified"));
    }

    let (_user, token) = establish_session(&state, &identity);
    let response = Redirect::temporary("/").into_response();
    Ok(with_session_cookie(
        response,
        &token,
        state.config.session_ttl_seconds,
    ))
}

#[derive(Deserialize)]
pub struct DevLoginRequest {
    pub email: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// POST /api/auth/dev-login — development-only login bypassing OAuth.
pub async fn dev_login(
    State(state): State<SharedState>,
    Json(request): Json<DevLoginRequest>,
) -> Result<Response, ApiError> {
    if !state.config.dev_login.enabled {
        state.audit.record(
            "dev_login",
            &request.email,
            "denied",
            "dev login is disabled",
        );
        return Err(ApiError::unauthorized_api("dev login is disabled"));
    }
    let email = request.email.trim().to_string();
    if email.is_empty() || !email.contains('@') {
        return Err(ApiError::invalid_input("a valid email is required"));
    }
    let identity = AuthenticatedIdentity {
        provider_id: "dev".to_string(),
        subject: email.clone(),
        display_name: request.display_name.unwrap_or_else(|| email.clone()),
        email,
        email_verified: true,
    };
    let (user, token) = establish_session(&state, &identity);
    let body = Json(MeResponse::build(&state, &user));
    let response = (StatusCode::OK, body).into_response();
    Ok(with_session_cookie(
        response,
        &token,
        state.config.session_ttl_seconds,
    ))
}

/// POST /api/auth/refresh — rotate the session token (Impl Spec §5.2).
pub async fn refresh(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(token) = session_token(&headers) else {
        return Err(ApiError::unauthenticated("no session"));
    };
    let Some(new_token) = state.sessions.rotate(&token) else {
        state.audit.record(
            "session_refresh",
            "unknown",
            "denied",
            "invalid or expired session token presented",
        );
        return Err(ApiError::unauthenticated("invalid or expired session"));
    };
    let response = (
        StatusCode::OK,
        Json(serde_json::json!({ "rotated": true })),
    )
        .into_response();
    Ok(with_session_cookie(
        response,
        &new_token,
        state.config.session_ttl_seconds,
    ))
}

/// POST /api/auth/logout — revoke the session.
pub async fn logout(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        state.sessions.revoke(&token);
    }
    let response = (
        StatusCode::OK,
        Json(serde_json::json!({ "logged_out": true })),
    )
        .into_response();
    clear_session_cookie(response)
}

/// GET /api/admin/ping — owner-gated skeleton endpoint proving authorization
/// end to end. Replaced by real book APIs in M4.
pub async fn admin_ping(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = current_user(&state, &headers)?;
    if let Err(err) = state.authz.authorize(&user, Action::AdminPing) {
        state.audit.record(
            "authorization",
            &user.email,
            "denied",
            "admin_ping requires bootstrap owner",
        );
        return Err(err);
    }
    Ok(Json(serde_json::json!({
        "message": "pong",
        "owner": user.email,
    })))
}
