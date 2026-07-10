//! M1 integration tests: authentication, authorization, sessions (Impl Plan M1).

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ledgerzero_backend::app::build_router;
use ledgerzero_backend::config::{
    DevLoginConfig, GoogleOAuthConfig, OAuthConfig, ServerConfig,
};
use ledgerzero_backend::state::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

const OWNER: &str = "zhian.job@gmail.com";

fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
    let audit_path =
        std::env::temp_dir().join(format!("lz_test_audit_{}.jsonl", uuid::Uuid::new_v4()));
    let config = ServerConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        books_dir: "./books".to_string(),
        frontend_dist: "./nonexistent-dist".to_string(),
        ops_audit_log: audit_path.to_string_lossy().to_string(),
        bootstrap_owner_email: OWNER.to_string(),
        session_ttl_seconds: 3600,
        oauth: OAuthConfig {
            google: GoogleOAuthConfig {
                client_id: String::new(),
                client_secret: String::new(),
                redirect_url: String::new(),
            },
        },
        dev_login: DevLoginConfig { enabled: true },
    };
    (Arc::new(AppState::new(config)), audit_path)
}

fn app() -> (Router, std::path::PathBuf) {
    let (state, audit_path) = test_state();
    (build_router(state), audit_path)
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn session_cookie(response: &axum::response::Response) -> String {
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie present")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

async fn dev_login(app: &Router, email: &str) -> (String, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/dev-login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "email": email }).to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie(&response);
    let body = body_json(response).await;
    (cookie, body)
}

async fn get_with_cookie(app: &Router, uri: &str, cookie: &str) -> axum::response::Response {
    let request = Request::builder()
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

async fn post_with_cookie(app: &Router, uri: &str, cookie: &str) -> axum::response::Response {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn health_is_public() {
    let (app, _) = app();
    let response = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn unauthenticated_me_is_rejected_with_structured_error() {
    let (app, _) = app();
    let response = app
        .oneshot(Request::builder().uri("/api/auth/me").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["error_code"], "UNAUTHENTICATED");
}

#[tokio::test]
async fn owner_login_reports_identity_and_authority() {
    let (app, _) = app();
    let (cookie, login_body) = dev_login(&app, OWNER).await;
    assert_eq!(login_body["is_bootstrap_owner"], true);

    let response = get_with_cookie(&app, "/api/auth/me", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["user"]["email"], OWNER);
    assert_eq!(body["is_bootstrap_owner"], true);
    let actions = body["allowed_actions"].as_array().unwrap();
    assert!(actions.iter().any(|a| a == "create_accounting_book"));
    assert!(actions.iter().any(|a| a == "open_book"));
}

#[tokio::test]
async fn owner_passes_owner_gated_endpoint() {
    let (app, _) = app();
    let (cookie, _) = dev_login(&app, OWNER).await;
    let response = get_with_cookie(&app, "/api/admin/ping", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["message"], "pong");
}

#[tokio::test]
async fn non_owner_is_denied_and_audited() {
    let (app, audit_path) = app();
    let (cookie, login_body) = dev_login(&app, "intruder@example.com").await;
    assert_eq!(login_body["is_bootstrap_owner"], false);
    assert!(login_body["allowed_actions"].as_array().unwrap().is_empty());

    let response = get_with_cookie(&app, "/api/admin/ping", &cookie).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_json(response).await;
    assert_eq!(body["error_code"], "UNAUTHORIZED_API");

    let audit = std::fs::read_to_string(&audit_path).expect("audit log written");
    assert!(
        audit.contains("intruder@example.com") && audit.contains("denied"),
        "authorization denial must be recorded in the operational audit log"
    );
    let _ = std::fs::remove_file(&audit_path);
}

#[tokio::test]
async fn refresh_rotates_and_invalidates_old_token() {
    let (app, _) = app();
    let (cookie, _) = dev_login(&app, OWNER).await;

    let response = post_with_cookie(&app, "/api/auth/refresh", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let new_cookie = session_cookie(&response);
    assert_ne!(new_cookie, cookie, "token must rotate");

    let old = get_with_cookie(&app, "/api/auth/me", &cookie).await;
    assert_eq!(old.status(), StatusCode::UNAUTHORIZED, "old token invalid");

    let fresh = get_with_cookie(&app, "/api/auth/me", &new_cookie).await;
    assert_eq!(fresh.status(), StatusCode::OK, "rotated token valid");
}

#[tokio::test]
async fn logout_revokes_session() {
    let (app, _) = app();
    let (cookie, _) = dev_login(&app, OWNER).await;

    let response = post_with_cookie(&app, "/api/auth/logout", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = get_with_cookie(&app, "/api/auth/me", &cookie).await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dev_login_rejected_when_disabled() {
    let (state, _) = test_state();
    // Rebuild state with dev login disabled.
    let mut config = state.config.clone();
    config.dev_login.enabled = false;
    let app = build_router(Arc::new(AppState::new(config)));

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/dev-login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "email": OWNER }).to_string()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_json(response).await;
    assert_eq!(body["error_code"], "UNAUTHORIZED_API");
}

#[tokio::test]
async fn same_email_different_provider_maps_to_same_user() {
    // AKA behavior (Impl Spec §2.9): identities with the same verified email
    // resolve to the same authorized user.
    let (state, _) = test_state();
    let first = state
        .users
        .resolve_identity("google", "sub-123", OWNER, "Zhi An");
    let second = state.users.resolve_identity("dev", OWNER, OWNER, "Zhi An");
    assert_eq!(first.user_id, second.user_id);
}
