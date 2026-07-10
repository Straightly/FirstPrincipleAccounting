//! Authentication handlers: Google OAuth flow, dev login, sessions, and the
//! owner-gated skeleton endpoint (Impl Spec §5.2, §5.3).

use crate::authz::Action;
use crate::error::ApiError;
use crate::state::SharedState;
use crate::users::User;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SESSION_COOKIE: &str = "lz_session";
const OAUTH_STATE_TTL: Duration = Duration::from_secs(600);

// ---------- helpers ----------

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

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

fn login_user(state: &SharedState, user: &User) -> Response {
    let token = state.sessions.create(user.user_id);
    let body = Json(MeResponse::build(state, user));
    let response = (StatusCode::OK, body).into_response();
    with_session_cookie(response, &token, state.config.session_ttl_seconds)
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
pub struct AuthConfigResponse {
    pub google_configured: bool,
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

/// GET /api/auth/config — what login methods the launcher should offer.
pub async fn auth_config(State(state): State<SharedState>) -> Json<AuthConfigResponse> {
    Json(AuthConfigResponse {
        google_configured: !state.config.oauth.google.client_id.is_empty(),
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

/// GET /api/auth/google/login — redirect to Google's consent screen.
pub async fn google_login(State(state): State<SharedState>) -> Result<Response, ApiError> {
    let google = &state.config.oauth.google;
    if google.client_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "OAUTH_NOT_CONFIGURED",
            "Google OAuth client is not configured in server.config.toml",
        ));
    }
    let csrf = Uuid::new_v4().to_string();
    {
        let mut states = state.oauth_states.write().expect("oauth state lock");
        let now = Instant::now();
        states.retain(|_, created| now.duration_since(*created) < OAUTH_STATE_TTL);
        states.insert(csrf.clone(), now);
    }
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
        urlencode(&google.client_id),
        urlencode(&google.redirect_url),
        urlencode("openid email profile"),
        urlencode(&csrf),
    );
    Ok(Redirect::temporary(&url).into_response())
}

#[derive(Deserialize)]
pub struct GoogleCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
}

/// GET /api/auth/google/callback — code exchange, identity resolution, session.
pub async fn google_callback(
    State(state): State<SharedState>,
    Query(params): Query<GoogleCallbackParams>,
) -> Result<Response, ApiError> {
    if let Some(error) = params.error {
        state
            .audit
            .record("google_login", "unknown", "denied", &error);
        return Ok(Redirect::temporary("/?login_error=denied").into_response());
    }
    let (Some(code), Some(csrf)) = (params.code, params.state) else {
        return Err(ApiError::invalid_input("missing code or state"));
    };
    {
        let mut states = state.oauth_states.write().expect("oauth state lock");
        let valid = states
            .remove(&csrf)
            .map(|created| created.elapsed() < OAUTH_STATE_TTL)
            .unwrap_or(false);
        if !valid {
            state.audit.record(
                "google_login",
                "unknown",
                "denied",
                "unknown or expired OAuth state (possible CSRF)",
            );
            return Err(ApiError::invalid_input("unknown or expired OAuth state"));
        }
    }

    let google = &state.config.oauth.google;
    let client = reqwest::Client::new();
    let token: GoogleTokenResponse = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", google.client_id.as_str()),
            ("client_secret", google.client_secret.as_str()),
            ("redirect_uri", google.redirect_url.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("token exchange failed: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("token response invalid: {e}")))?;

    let info: GoogleUserInfo = client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("userinfo failed: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("userinfo response invalid: {e}")))?;

    let Some(email) = info.email else {
        state.audit.record(
            "google_login",
            &info.sub,
            "denied",
            "Google identity has no email claim",
        );
        return Err(ApiError::unauthenticated("Google identity has no email"));
    };
    if info.email_verified == Some(false) {
        state
            .audit
            .record("google_login", &email, "denied", "email not verified");
        return Err(ApiError::unauthenticated("Google email is not verified"));
    }

    let display_name = info.name.unwrap_or_else(|| email.clone());
    let user = state
        .users
        .resolve_identity("google", &info.sub, &email, &display_name);
    let token = state.sessions.create(user.user_id);
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
    let display_name = request.display_name.unwrap_or_else(|| email.clone());
    let user = state
        .users
        .resolve_identity("dev", &email, &email, &display_name);
    Ok(login_user(&state, &user))
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
