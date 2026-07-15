//! M9 integration tests: export/restore driven entirely over HTTP (Impl
//! Plan M9 exit criteria — "a book moves to a new folder/deployment and
//! keeps operating"), plus the artifact-availability marking a restore can
//! surface (Impl Spec §8.2).

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
const OTHER: &str = "someone.else@example.com";

fn test_config(books_dir: &std::path::Path, dev_artifacts_dir: &std::path::Path) -> ServerConfig {
    let audit_path = std::env::temp_dir().join(format!("lz_test_audit_{}.jsonl", Uuid::new_v4()));
    ServerConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        books_dir: books_dir.to_string_lossy().to_string(),
        frontend_dist: "./nonexistent-dist".to_string(),
        dev_artifacts_dir: dev_artifacts_dir.to_string_lossy().to_string(),
        ops_audit_log: audit_path.to_string_lossy().to_string(),
        bootstrap_owner_email: OWNER.to_string(),
        session_ttl_seconds: 3600,
        auth_providers: vec![],
        dev_login: DevLoginConfig { enabled: true },
    }
}

fn app_over(books_dir: &std::path::Path, dev_artifacts_dir: &std::path::Path) -> Router {
    build_router(Arc::new(AppState::new(test_config(
        books_dir,
        dev_artifacts_dir,
    ))))
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

async fn create_book(app: &Router, cookie: &str, name: &str) -> (Uuid, Uuid) {
    let response = post(
        app,
        "/api/books",
        cookie,
        json!({ "name": name, "passphrase": "correct horse battery staple" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    (
        Uuid::parse_str(body["book_id"].as_str().unwrap()).unwrap(),
        Uuid::parse_str(body["entity_id"].as_str().unwrap()).unwrap(),
    )
}

/// Seeds a book with a USD chart, Cash/Capital accounts, an open period,
/// and a $1000 opening entry — returns the ids the caller needs.
async fn seed_book(app: &Router, cookie: &str) -> (Uuid, Uuid, String, String) {
    let (book_id, entity_id) = create_book(app, cookie, "Acme Books").await;
    let entity_id_s = entity_id.to_string();

    let usd = body_json(
        post(
            app,
            &format!("/api/books/{book_id}/resource-types"),
            cookie,
            json!({
                "op_id": Uuid::new_v4(), "name": "US Dollar", "kind": "CURRENCY",
                "code": "USD", "unit_of_measure": "USD", "precision": 2
            }),
        )
        .await,
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let chart_id = body_json(
        post(
            app,
            &format!("/api/books/{book_id}/charts"),
            cookie,
            json!({ "op_id": Uuid::new_v4(), "entity_id": entity_id_s, "name": "Main", "description": null, "activate": true }),
        )
        .await,
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let account = |name: &'static str, account_type: &'static str| {
        let app = app.clone();
        let cookie = cookie.to_string();
        let chart_id = chart_id.clone();
        let usd = usd.clone();
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
            assert_eq!(resp.status(), StatusCode::OK);
            body_json(resp).await["id"].as_str().unwrap().to_string()
        }
    };
    let cash = account("Cash", "ASSET").await;
    let capital = account("Owner Capital", "EQUITY").await;

    let period_resp = post(
        app,
        &format!("/api/books/{book_id}/periods"),
        cookie,
        json!({
            "op_id": Uuid::new_v4(), "entity_id": entity_id_s, "name": "2026-01",
            "start_date": "2026-01-01", "end_date": "2026-01-31"
        }),
    )
    .await;
    assert_eq!(period_resp.status(), StatusCode::OK);

    let entry_resp = post(
        app,
        &format!("/api/books/{book_id}/entries"),
        cookie,
        json!({
            "entry_id": Uuid::new_v4(), "entity_id": entity_id_s, "entry_date": "2026-01-15",
            "description": "opening balance", "source": "MANUAL",
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": "1000.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": capital, "debit_amount": null, "credit_amount": "1000.00", "memo": null }
            ]
        }),
    )
    .await;
    assert_eq!(entry_resp.status(), StatusCode::OK);

    (book_id, entity_id, entity_id_s, cash)
}

async fn balance(app: &Router, cookie: &str, book_id: Uuid, account_id: &str) -> Value {
    let resp = get(
        app,
        &format!("/api/books/{book_id}/accounts/{account_id}/balance"),
        cookie,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    body_json(resp).await
}

/// The Impl Plan M9 exit criteria, driven end to end: export, restore (to
/// the same location — restore always targets the bundle's own book_id),
/// balances survive, and posting continues to work afterward.
#[tokio::test]
async fn export_then_restore_preserves_book_id_balances_and_keeps_operating() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let cookie = dev_login(&app, OWNER).await;
    let (book_id, entity_id, entity_id_s, cash) = seed_book(&app, &cookie).await;

    let before = balance(&app, &cookie, book_id, &cash).await;
    assert_eq!(before["net"], "1000.00000000");

    let export_resp = post(
        &app,
        &format!("/api/books/{book_id}/export"),
        &cookie,
        json!({ "reader_passphrase": "reader passphrase, quite long" }),
    )
    .await;
    assert_eq!(export_resp.status(), StatusCode::OK);
    let bundle = body_json(export_resp).await;
    assert_eq!(bundle["book_id"], book_id.to_string());
    // At least: entity created, resource type, chart, 2 accounts, period,
    // opening entry.
    assert!(bundle["event_count"].as_u64().unwrap() >= 6);

    let restore_resp = post(
        &app,
        "/api/books/restore",
        &cookie,
        json!({
            "op_id": Uuid::new_v4(),
            "bundle": bundle,
            "reader_passphrase": "reader passphrase, quite long",
            "storage_passphrase": "a brand new storage passphrase",
        }),
    )
    .await;
    assert_eq!(
        restore_resp.status(),
        StatusCode::OK,
        "{:?}",
        body_json(restore_resp).await
    );
    let restored = body_json(restore_resp).await;
    assert_eq!(restored["book_id"], book_id.to_string());
    assert_eq!(restored["entity_id"], entity_id.to_string());
    assert_eq!(restored["name"], "Acme Books");

    // Same balance survives the restore.
    let after = balance(&app, &cookie, book_id, &cash).await;
    assert_eq!(after["net"], "1000.00000000");

    // The restored book is a live operational starting point, not a dead
    // snapshot — posting continues.
    let second_entry = post(
        &app,
        &format!("/api/books/{book_id}/entries"),
        &cookie,
        json!({
            "entry_id": Uuid::new_v4(), "entity_id": entity_id_s, "entry_date": "2026-01-20",
            "description": "post-restore entry", "source": "MANUAL",
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": "50.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": null, "credit_amount": "50.00", "memo": null }
            ]
        }),
    )
    .await;
    assert_eq!(
        second_entry.status(),
        StatusCode::OK,
        "posting after restore must succeed: {:?}",
        body_json(second_entry).await
    );

    // The old storage passphrase no longer opens this book — restore
    // rotated the key, it did not reuse the export's source key.
    let stale_open = post(
        &app,
        &format!("/api/books/{book_id}/open"),
        &cookie,
        json!({ "passphrase": "correct horse battery staple" }),
    )
    .await;
    // Already open in this process, so `open` short-circuits before ever
    // touching the passphrase (Impl Plan M4's documented idempotent-open
    // behavior) — this call succeeding here is expected, not a signal the
    // old passphrase still works. The real proof the key rotated is in the
    // engine-level `export_restore.rs` test, which reopens from a fresh
    // process state.
    assert_eq!(stale_open.status(), StatusCode::OK);
}

#[tokio::test]
async fn export_requires_the_correct_reader_passphrase_to_restore() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let cookie = dev_login(&app, OWNER).await;
    let (book_id, ..) = seed_book(&app, &cookie).await;

    let bundle = body_json(
        post(
            &app,
            &format!("/api/books/{book_id}/export"),
            &cookie,
            json!({ "reader_passphrase": "the right passphrase" }),
        )
        .await,
    )
    .await;

    let restore_resp = post(
        &app,
        "/api/books/restore",
        &cookie,
        json!({
            "op_id": Uuid::new_v4(),
            "bundle": bundle,
            "reader_passphrase": "the wrong passphrase",
            "storage_passphrase": "a brand new storage passphrase",
        }),
    )
    .await;
    assert_eq!(restore_resp.status(), StatusCode::UNAUTHORIZED);
    let err = body_json(restore_resp).await;
    assert_eq!(err["error_code"], "WRONG_PASSPHRASE");
}

#[tokio::test]
async fn export_and_restore_are_owner_gated() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let owner_cookie = dev_login(&app, OWNER).await;
    let (book_id, ..) = seed_book(&app, &owner_cookie).await;
    let other_cookie = dev_login(&app, OTHER).await;

    let export_resp = post(
        &app,
        &format!("/api/books/{book_id}/export"),
        &other_cookie,
        json!({ "reader_passphrase": "whatever" }),
    )
    .await;
    assert_eq!(export_resp.status(), StatusCode::FORBIDDEN);

    let restore_resp = post(
        &app,
        "/api/books/restore",
        &other_cookie,
        json!({
            "op_id": Uuid::new_v4(),
            "bundle": { "version": 1, "book_id": book_id, "exported_at": 0, "event_count": 0,
                        "kdf": "argon2id", "m_cost_kib": 8, "t_cost": 1, "p_cost": 1,
                        "salt_hex": "00", "payload_hex": "00" },
            "reader_passphrase": "whatever",
            "storage_passphrase": "whatever-storage-pass",
        }),
    )
    .await;
    assert_eq!(restore_resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn restore_marks_workflows_unavailable_when_the_artifact_is_missing() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let cookie = dev_login(&app, OWNER).await;
    let (book_id, entity_id, entity_id_s, _cash) = seed_book(&app, &cookie).await;

    let deployment_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let code_dir = artifacts_dir
        .path()
        .join("workflows")
        .join(deployment_id.to_string())
        .join("code");
    std::fs::create_dir_all(&code_dir).unwrap();
    std::fs::write(
        artifacts_dir
            .path()
            .join("workflows")
            .join(deployment_id.to_string())
            .join("manifest.json"),
        "{}",
    )
    .unwrap();
    std::fs::write(code_dir.join("app.js"), "// app").unwrap();

    let deploy_resp = post(
        &app,
        &format!("/api/books/{book_id}/workflows/deploy"),
        &cookie,
        json!({
            "workflow_deployment_id": deployment_id, "workflow_id": workflow_id,
            "entity_id": entity_id, "workflow_name": "Test workflow", "description": null,
            "backend_api_calls": ["post_entry"]
        }),
    )
    .await;
    assert_eq!(deploy_resp.status(), StatusCode::OK);

    let list_before = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/workflows?entity_id={entity_id_s}"),
            &cookie,
        )
        .await,
    )
    .await;
    assert_eq!(list_before[0]["artifact_available"], true);

    // Delete the dev artifact — simulating a book restored somewhere the
    // dev artifact store wasn't also restored (Impl Spec §8.2).
    std::fs::remove_dir_all(
        artifacts_dir
            .path()
            .join("workflows")
            .join(deployment_id.to_string()),
    )
    .unwrap();

    let list_after = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/workflows?entity_id={entity_id_s}"),
            &cookie,
        )
        .await,
    )
    .await;
    assert_eq!(list_after[0]["artifact_available"], false);
    // Still discoverable by an admin (not silently hidden) — just flagged.
    assert_eq!(
        list_after[0]["workflow_deployment_id"],
        deployment_id.to_string()
    );
}
