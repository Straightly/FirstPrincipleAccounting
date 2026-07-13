//! M4 integration tests: book lifecycle and core accounting APIs, driven
//! entirely over HTTP (Impl Plan M4 exit criteria).

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use ledgerzero_backend::app::build_router;
use ledgerzero_backend::config::{DevLoginConfig, ServerConfig};
use ledgerzero_backend::state::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

const OWNER: &str = "zhian.job@gmail.com";

fn test_config(books_dir: &std::path::Path) -> ServerConfig {
    let audit_path = std::env::temp_dir().join(format!("lz_test_audit_{}.jsonl", Uuid::new_v4()));
    ServerConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        books_dir: books_dir.to_string_lossy().to_string(),
        frontend_dist: "./nonexistent-dist".to_string(),
        ops_audit_log: audit_path.to_string_lossy().to_string(),
        bootstrap_owner_email: OWNER.to_string(),
        session_ttl_seconds: 3600,
        auth_providers: vec![],
        dev_login: DevLoginConfig { enabled: true },
    }
}

fn app_over(books_dir: &std::path::Path) -> Router {
    build_router(Arc::new(AppState::new(test_config(books_dir))))
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn session_cookie(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie present")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn dev_login(app: &Router, email: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/dev-login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "email": email }).to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    session_cookie(&response)
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let request = if let Some(body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    app.clone().oneshot(request).await.unwrap()
}

async fn get(app: &Router, uri: &str, cookie: &str) -> axum::response::Response {
    call(app, Method::GET, uri, Some(cookie), None).await
}

async fn post(app: &Router, uri: &str, cookie: &str, body: Value) -> axum::response::Response {
    call(app, Method::POST, uri, Some(cookie), Some(body)).await
}

async fn create_book(app: &Router, cookie: &str, name: &str) -> Uuid {
    let response = post(
        app,
        "/api/books",
        cookie,
        json!({ "name": name, "passphrase": "correct horse battery staple" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    Uuid::parse_str(body["book_id"].as_str().unwrap()).unwrap()
}

/// Golden path: dev-login as the bootstrap owner, create a book, set up an
/// entity/resource type/chart/accounts/period, post a balanced entry, and
/// read it back — every call authenticated and authorized (M4 exit criteria).
#[tokio::test]
async fn full_book_lifecycle_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_over(dir.path());
    let cookie = dev_login(&app, OWNER).await;

    let book_id = create_book(&app, &cookie, "Acme Books").await;

    let entity_resp = post(
        &app,
        &format!("/api/books/{book_id}/entities"),
        &cookie,
        json!({ "op_id": Uuid::new_v4(), "name": "Acme LLC" }),
    )
    .await;
    assert_eq!(entity_resp.status(), StatusCode::OK);
    let entity_id = body_json(entity_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let usd_resp = post(
        &app,
        &format!("/api/books/{book_id}/resource-types"),
        &cookie,
        json!({
            "op_id": Uuid::new_v4(), "name": "US Dollar", "kind": "CURRENCY",
            "code": "USD", "unit_of_measure": "USD", "precision": 2
        }),
    )
    .await;
    assert_eq!(usd_resp.status(), StatusCode::OK);
    let usd = body_json(usd_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let chart_resp = post(
        &app,
        &format!("/api/books/{book_id}/charts"),
        &cookie,
        json!({ "op_id": Uuid::new_v4(), "entity_id": entity_id, "name": "Main", "description": null, "activate": true }),
    )
    .await;
    assert_eq!(chart_resp.status(), StatusCode::OK);
    let chart_id = body_json(chart_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let account = |name: &str, account_type: &str| {
        let app = app.clone();
        let cookie = cookie.clone();
        let chart_id = chart_id.clone();
        let usd = usd.clone();
        let name = name.to_string();
        let account_type = account_type.to_string();
        async move {
            let resp = post(
                &app,
                &format!("/api/books/{book_id}/accounts"),
                &cookie,
                json!({
                    "op_id": Uuid::new_v4(), "chart_id": chart_id, "name": name, "code": null,
                    "account_type": account_type, "resource_type_id": usd,
                    "parent_account_id": null, "validation_rules": null, "metadata": null
                }),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::OK, "creating account {name}");
            body_json(resp).await["id"].as_str().unwrap().to_string()
        }
    };
    let cash = account("Cash", "ASSET").await;
    let capital = account("Owner Capital", "EQUITY").await;

    let period_resp = post(
        &app,
        &format!("/api/books/{book_id}/periods"),
        &cookie,
        json!({
            "op_id": Uuid::new_v4(), "entity_id": entity_id, "name": "2026-01",
            "start_date": "2026-01-01", "end_date": "2026-01-31"
        }),
    )
    .await;
    assert_eq!(period_resp.status(), StatusCode::OK);

    let entry_id = Uuid::new_v4();
    let entry_resp = post(
        &app,
        &format!("/api/books/{book_id}/entries"),
        &cookie,
        json!({
            "entry_id": entry_id, "entity_id": entity_id, "entry_date": "2026-01-15",
            "description": "opening balance", "source": "MANUAL",
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": "1000.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": capital, "debit_amount": null, "credit_amount": "1000.00", "memo": null }
            ]
        }),
    )
    .await;
    assert_eq!(
        entry_resp.status(),
        StatusCode::OK,
        "{:?}",
        body_json(entry_resp).await
    );

    let balance_resp = get(
        &app,
        &format!("/api/books/{book_id}/accounts/{cash}/balance"),
        &cookie,
    )
    .await;
    assert_eq!(balance_resp.status(), StatusCode::OK);
    let balance = body_json(balance_resp).await;
    assert_eq!(balance["debit_total"], "1000.00000000");
    assert_eq!(balance["net"], "1000.00000000");

    let entries_resp = get(
        &app,
        &format!("/api/books/{book_id}/entries?entity_id={entity_id}"),
        &cookie,
    )
    .await;
    assert_eq!(entries_resp.status(), StatusCode::OK);
    let entries = body_json(entries_resp).await;
    assert_eq!(entries.as_array().unwrap().len(), 1);

    let audit_resp = get(&app, &format!("/api/books/{book_id}/audit-log"), &cookie).await;
    assert_eq!(audit_resp.status(), StatusCode::OK);
    let audit = body_json(audit_resp).await;
    // entity + resource type + chart + 2 accounts + period + entry = 7 events.
    assert_eq!(audit.as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn create_book_requires_authentication() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_over(dir.path());
    let response = call(
        &app,
        Method::POST,
        "/api/books",
        None,
        Some(json!({ "name": "x", "passphrase": "correct horse battery staple" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_book_rejects_non_owner() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_over(dir.path());
    let cookie = dev_login(&app, "someone.else@example.com").await;
    let response = post(
        &app,
        "/api/books",
        &cookie,
        json!({ "name": "x", "passphrase": "correct horse battery staple" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_json(response).await;
    assert_eq!(body["error_code"], "UNAUTHORIZED_API");
}

#[tokio::test]
async fn book_api_on_unopened_book_is_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_over(dir.path());
    let cookie = dev_login(&app, OWNER).await;
    let random_book_id = Uuid::new_v4();
    let response = post(
        &app,
        &format!("/api/books/{random_book_id}/entities"),
        &cookie,
        json!({ "op_id": Uuid::new_v4(), "name": "Acme LLC" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = body_json(response).await;
    assert_eq!(body["error_code"], "BOOK_NOT_OPEN");
}

#[tokio::test]
async fn wrong_passphrase_on_open_is_rejected() {
    let dir = tempfile::tempdir().unwrap();

    // App A creates the book (which also opens it in App A's memory).
    let app_a = app_over(dir.path());
    let cookie_a = dev_login(&app_a, OWNER).await;
    let book_id = create_book(&app_a, &cookie_a, "Acme Books").await;

    // App B is a fresh server process pointed at the same books_dir — the
    // book exists on disk but is not open in App B's memory yet.
    let app_b = app_over(dir.path());
    let cookie_b = dev_login(&app_b, OWNER).await;
    let response = post(
        &app_b,
        &format!("/api/books/{book_id}/open"),
        &cookie_b,
        json!({ "passphrase": "wrong passphrase entirely" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["error_code"], "WRONG_PASSPHRASE");

    // The correct passphrase opens it.
    let response = post(
        &app_b,
        &format!("/api/books/{book_id}/open"),
        &cookie_b,
        json!({ "passphrase": "correct horse battery staple" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn idempotent_replay_and_conflict_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_over(dir.path());
    let cookie = dev_login(&app, OWNER).await;
    let book_id = create_book(&app, &cookie, "Acme Books").await;

    let op_id = Uuid::new_v4();
    let body = json!({ "op_id": op_id, "name": "Acme LLC" });
    let first = post(
        &app,
        &format!("/api/books/{book_id}/entities"),
        &cookie,
        body.clone(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_id = body_json(first).await["id"].clone();

    let replay = post(
        &app,
        &format!("/api/books/{book_id}/entities"),
        &cookie,
        body,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::OK);
    let replay_id = body_json(replay).await["id"].clone();
    assert_eq!(
        first_id, replay_id,
        "identical replay returns the original outcome"
    );

    let tampered = post(
        &app,
        &format!("/api/books/{book_id}/entities"),
        &cookie,
        json!({ "op_id": op_id, "name": "Different Name LLC" }),
    )
    .await;
    assert_eq!(tampered.status(), StatusCode::CONFLICT);
    let err = body_json(tampered).await;
    assert_eq!(err["error_code"], "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn copy_chart_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_over(dir.path());
    let cookie = dev_login(&app, OWNER).await;
    let book_id = create_book(&app, &cookie, "Acme Books").await;

    let entity_id = body_json(
        post(
            &app,
            &format!("/api/books/{book_id}/entities"),
            &cookie,
            json!({ "op_id": Uuid::new_v4(), "name": "Acme LLC" }),
        )
        .await,
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let chart_id = body_json(
        post(
            &app,
            &format!("/api/books/{book_id}/charts"),
            &cookie,
            json!({ "op_id": Uuid::new_v4(), "entity_id": entity_id, "name": "Main", "description": null, "activate": true }),
        )
        .await,
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let copy_resp = post(
        &app,
        &format!("/api/books/{book_id}/charts/{chart_id}/copy"),
        &cookie,
        json!({ "op_id": Uuid::new_v4(), "name": "Main (copy)", "description": null, "activate": false }),
    )
    .await;
    assert_eq!(copy_resp.status(), StatusCode::OK);
    let new_chart_id = body_json(copy_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(new_chart_id, chart_id);

    let charts_resp = get(
        &app,
        &format!("/api/books/{book_id}/charts?entity_id={entity_id}"),
        &cookie,
    )
    .await;
    assert_eq!(charts_resp.status(), StatusCode::OK);
    assert_eq!(body_json(charts_resp).await.as_array().unwrap().len(), 2);
}
