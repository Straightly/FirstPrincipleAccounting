//! M5 integration tests: workflow deployment, auto-roles, role assignment,
//! and workflow-scoped authorization on `post_entry`, driven over HTTP
//! (Impl Plan M5 exit criteria: "the workflow runs only via a valid
//! deployment and role assignment; authorization tests pass").

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
const EMPLOYEE: &str = "employee@example.com";

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

async fn dev_login(app: &Router, email: &str) -> (String, Uuid) {
    let response = call(
        app,
        Method::POST,
        "/api/auth/dev-login",
        None,
        Some(json!({ "email": email })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie(&response);
    let body = body_json(response).await;
    let user_id = Uuid::parse_str(body["user"]["user_id"].as_str().unwrap()).unwrap();
    (cookie, user_id)
}

async fn create_book(app: &Router, cookie: &str) -> Uuid {
    let response = post(
        app,
        "/api/books",
        cookie,
        json!({ "name": "Acme Books", "passphrase": "correct horse battery staple" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    Uuid::parse_str(body_json(response).await["book_id"].as_str().unwrap()).unwrap()
}

async fn id_field(response: axum::response::Response) -> Uuid {
    assert_eq!(response.status(), StatusCode::OK, "expected 200 OK");
    Uuid::parse_str(body_json(response).await["id"].as_str().unwrap()).unwrap()
}

/// Writes a minimal, valid dev artifact to disk so `hash_artifact` succeeds.
fn write_artifact(dev_artifacts_dir: &std::path::Path, deployment_id: Uuid) {
    let dir = dev_artifacts_dir
        .join("workflows")
        .join(deployment_id.to_string());
    let code_dir = dir.join("code");
    std::fs::create_dir_all(&code_dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(r#"{{"generator":"hand-written","workflow_deployment_id":"{deployment_id}"}}"#),
    )
    .unwrap();
    std::fs::write(
        code_dir.join("index.html"),
        "<!doctype html><div id=\"root\"></div>",
    )
    .unwrap();
    std::fs::write(code_dir.join("app.js"), "// hand-written workflow app").unwrap();
}

/// Sets up a book with one entity, a USD asset/expense account pair, and an
/// open period — everything `post_entry` needs — returning the ids used.
async fn setup_book(app: &Router, owner_cookie: &str) -> (Uuid, Uuid, Uuid, Uuid) {
    let book_id = create_book(app, owner_cookie).await;
    let entity_id = id_field(
        post(
            app,
            &format!("/api/books/{book_id}/entities"),
            owner_cookie,
            json!({ "op_id": Uuid::new_v4(), "name": "Acme LLC" }),
        )
        .await,
    )
    .await;
    let usd = id_field(
        post(
            app,
            &format!("/api/books/{book_id}/resource-types"),
            owner_cookie,
            json!({
                "op_id": Uuid::new_v4(), "name": "US Dollar", "kind": "CURRENCY",
                "code": "USD", "unit_of_measure": "USD", "precision": 2
            }),
        )
        .await,
    )
    .await;
    let chart_id = id_field(
        post(
            app,
            &format!("/api/books/{book_id}/charts"),
            owner_cookie,
            json!({ "op_id": Uuid::new_v4(), "entity_id": entity_id, "name": "Main", "description": null, "activate": true }),
        )
        .await,
    )
    .await;
    let make_account = |name: &'static str, account_type: &'static str| {
        let app = app.clone();
        let owner_cookie = owner_cookie.to_string();
        async move {
            id_field(
                post(
                    &app,
                    &format!("/api/books/{book_id}/accounts"),
                    &owner_cookie,
                    json!({
                        "op_id": Uuid::new_v4(), "chart_id": chart_id, "name": name, "code": null,
                        "account_type": account_type, "resource_type_id": usd,
                        "parent_account_id": null, "validation_rules": null, "metadata": null
                    }),
                )
                .await,
            )
            .await
        }
    };
    let cash = make_account("Cash", "ASSET").await;
    let rent = make_account("Rent Expense", "EXPENSE").await;
    let period_resp = post(
        app,
        &format!("/api/books/{book_id}/periods"),
        owner_cookie,
        json!({
            "op_id": Uuid::new_v4(), "entity_id": entity_id, "name": "2026-02",
            "start_date": "2026-02-01", "end_date": "2026-02-28"
        }),
    )
    .await;
    assert_eq!(period_resp.status(), StatusCode::OK);
    (book_id, entity_id, cash, rent)
}

#[tokio::test]
async fn full_workflow_lifecycle_over_http() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let (owner_cookie, _owner_id) = dev_login(&app, OWNER).await;
    let (book_id, entity_id, cash, rent) = setup_book(&app, &owner_cookie).await;

    let deployment_id = Uuid::new_v4();
    write_artifact(artifacts_dir.path(), deployment_id);

    let deploy_resp = post(
        &app,
        &format!("/api/books/{book_id}/workflows/deploy"),
        &owner_cookie,
        json!({
            "workflow_deployment_id": deployment_id,
            "workflow_id": Uuid::new_v4(),
            "entity_id": entity_id,
            "workflow_name": "Recording startup expense",
            "description": "Hand-built reference workflow",
            "backend_api_calls": ["post_entry"]
        }),
    )
    .await;
    assert_eq!(id_field(deploy_resp).await, deployment_id);

    // Admin view: the deployment and its auto-role both exist.
    let workflows = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/workflows?entity_id={entity_id}"),
            &owner_cookie,
        )
        .await,
    )
    .await;
    let workflows = workflows.as_array().unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["workflow_name"], "Recording startup expense");
    let workflow_id = Uuid::parse_str(workflows[0]["workflow_id"].as_str().unwrap()).unwrap();

    let roles = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/roles?entity_id={entity_id}"),
            &owner_cookie,
        )
        .await,
    )
    .await;
    let roles = roles.as_array().unwrap();
    assert_eq!(roles.len(), 1, "deploying a workflow auto-creates one role");
    let role_id = Uuid::parse_str(roles[0]["role_id"].as_str().unwrap()).unwrap();

    // Assign the auto-role to an employee — not the book owner.
    let (employee_cookie, employee_id) = dev_login(&app, EMPLOYEE).await;
    let assign_resp = post(
        &app,
        &format!("/api/books/{book_id}/roles/{role_id}/users"),
        &owner_cookie,
        json!({ "op_id": Uuid::new_v4(), "user_id": employee_id }),
    )
    .await;
    assert_eq!(assign_resp.status(), StatusCode::OK);

    // The employee's launcher menu now shows the workflow.
    let mine = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/workflows/mine?entity_id={entity_id}"),
            &employee_cookie,
        )
        .await,
    )
    .await;
    assert_eq!(mine.as_array().unwrap().len(), 1);

    // The employee runs the workflow: posts a balanced entry carrying full
    // execution context, authorized purely by role assignment (no
    // Action::BookApi / bootstrap-owner check on this path).
    let execution_id = Uuid::new_v4();
    let entry_resp = post(
        &app,
        &format!("/api/books/{book_id}/entries"),
        &employee_cookie,
        json!({
            "entry_id": Uuid::new_v4(), "entity_id": entity_id, "entry_date": "2026-02-10",
            "description": "startup laptop expense", "source": "WORKFLOW",
            "workflow": {
                "workflow_id": workflow_id,
                "workflow_deployment_id": deployment_id,
                "workflow_execution_id": execution_id
            },
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": rent, "debit_amount": "899.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": null, "credit_amount": "899.00", "memo": null }
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

    let balance = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/accounts/{rent}/balance"),
            &owner_cookie,
        )
        .await,
    )
    .await;
    assert_eq!(balance["debit_total"], "899.00000000");
}

#[tokio::test]
async fn workflow_scoped_post_entry_rejects_unassigned_users() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let (owner_cookie, _) = dev_login(&app, OWNER).await;
    let (book_id, entity_id, cash, rent) = setup_book(&app, &owner_cookie).await;

    let deployment_id = Uuid::new_v4();
    write_artifact(artifacts_dir.path(), deployment_id);
    post(
        &app,
        &format!("/api/books/{book_id}/workflows/deploy"),
        &owner_cookie,
        json!({
            "workflow_deployment_id": deployment_id, "workflow_id": Uuid::new_v4(),
            "entity_id": entity_id,
            "workflow_name": "Recording startup expense", "description": null,
            "backend_api_calls": ["post_entry"]
        }),
    )
    .await;
    let workflows = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/workflows?entity_id={entity_id}"),
            &owner_cookie,
        )
        .await,
    )
    .await;
    let workflow_id = Uuid::parse_str(workflows[0]["workflow_id"].as_str().unwrap()).unwrap();

    // A signed-in but unassigned user (not even the owner is assigned this
    // role) attempts to run the workflow directly.
    let (stranger_cookie, _) = dev_login(&app, "stranger@example.com").await;
    let entry_resp = post(
        &app,
        &format!("/api/books/{book_id}/entries"),
        &stranger_cookie,
        json!({
            "entry_id": Uuid::new_v4(), "entity_id": entity_id, "entry_date": "2026-02-10",
            "description": "unauthorized attempt", "source": "WORKFLOW",
            "workflow": {
                "workflow_id": workflow_id,
                "workflow_deployment_id": deployment_id,
                "workflow_execution_id": Uuid::new_v4()
            },
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": rent, "debit_amount": "10.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": null, "credit_amount": "10.00", "memo": null }
            ]
        }),
    )
    .await;
    assert_eq!(entry_resp.status(), StatusCode::FORBIDDEN);
    let err = body_json(entry_resp).await;
    assert_eq!(err["error_code"], "UNAUTHORIZED_WORKFLOW");
}

#[tokio::test]
async fn workflow_scoped_post_entry_rejects_disallowed_api_and_bad_context() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let (owner_cookie, _) = dev_login(&app, OWNER).await;
    let (book_id, entity_id, cash, rent) = setup_book(&app, &owner_cookie).await;

    // Deploy a workflow whose allow-list does NOT include post_entry.
    let deployment_id = Uuid::new_v4();
    write_artifact(artifacts_dir.path(), deployment_id);
    post(
        &app,
        &format!("/api/books/{book_id}/workflows/deploy"),
        &owner_cookie,
        json!({
            "workflow_deployment_id": deployment_id, "workflow_id": Uuid::new_v4(),
            "entity_id": entity_id,
            "workflow_name": "Read-only report", "description": null,
            "backend_api_calls": ["get_balance"]
        }),
    )
    .await;
    let workflows = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/workflows?entity_id={entity_id}"),
            &owner_cookie,
        )
        .await,
    )
    .await;
    let workflow_id = Uuid::parse_str(workflows[0]["workflow_id"].as_str().unwrap()).unwrap();
    let roles = body_json(
        get(
            &app,
            &format!("/api/books/{book_id}/roles?entity_id={entity_id}"),
            &owner_cookie,
        )
        .await,
    )
    .await;
    let role_id = Uuid::parse_str(roles[0]["role_id"].as_str().unwrap()).unwrap();
    let (employee_cookie, employee_id) = dev_login(&app, EMPLOYEE).await;
    post(
        &app,
        &format!("/api/books/{book_id}/roles/{role_id}/users"),
        &owner_cookie,
        json!({ "op_id": Uuid::new_v4(), "user_id": employee_id }),
    )
    .await;

    let disallowed_resp = post(
        &app,
        &format!("/api/books/{book_id}/entries"),
        &employee_cookie,
        json!({
            "entry_id": Uuid::new_v4(), "entity_id": entity_id, "entry_date": "2026-02-10",
            "description": "not permitted", "source": "WORKFLOW",
            "workflow": {
                "workflow_id": workflow_id,
                "workflow_deployment_id": deployment_id,
                "workflow_execution_id": Uuid::new_v4()
            },
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": rent, "debit_amount": "5.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": null, "credit_amount": "5.00", "memo": null }
            ]
        }),
    )
    .await;
    assert_eq!(disallowed_resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        body_json(disallowed_resp).await["error_code"],
        "UNAUTHORIZED_API"
    );

    // Unknown workflow_deployment_id: INVALID_EXECUTION_CONTEXT.
    let bad_context_resp = post(
        &app,
        &format!("/api/books/{book_id}/entries"),
        &employee_cookie,
        json!({
            "entry_id": Uuid::new_v4(), "entity_id": entity_id, "entry_date": "2026-02-10",
            "description": "bad context", "source": "WORKFLOW",
            "workflow": {
                "workflow_id": workflow_id,
                "workflow_deployment_id": Uuid::new_v4(),
                "workflow_execution_id": Uuid::new_v4()
            },
            "lines": [
                { "line_id": Uuid::new_v4(), "account_id": rent, "debit_amount": "5.00", "credit_amount": null, "memo": null },
                { "line_id": Uuid::new_v4(), "account_id": cash, "debit_amount": null, "credit_amount": "5.00", "memo": null }
            ]
        }),
    )
    .await;
    assert_eq!(bad_context_resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(bad_context_resp).await["error_code"],
        "INVALID_EXECUTION_CONTEXT"
    );
}

#[tokio::test]
async fn deploy_workflow_requires_bootstrap_owner() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let (owner_cookie, _) = dev_login(&app, OWNER).await;
    let (book_id, entity_id, _, _) = setup_book(&app, &owner_cookie).await;

    let deployment_id = Uuid::new_v4();
    write_artifact(artifacts_dir.path(), deployment_id);
    let (other_cookie, _) = dev_login(&app, "someone.else@example.com").await;
    let response = post(
        &app,
        &format!("/api/books/{book_id}/workflows/deploy"),
        &other_cookie,
        json!({
            "workflow_deployment_id": deployment_id, "workflow_id": Uuid::new_v4(),
            "entity_id": entity_id,
            "workflow_name": "Recording startup expense", "description": null,
            "backend_api_calls": ["post_entry"]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deploy_workflow_fails_when_artifact_is_missing_from_disk() {
    let books_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();
    let app = app_over(books_dir.path(), artifacts_dir.path());
    let (owner_cookie, _) = dev_login(&app, OWNER).await;
    let (book_id, entity_id, _, _) = setup_book(&app, &owner_cookie).await;

    // No write_artifact() call — nothing on disk for this deployment id.
    let response = post(
        &app,
        &format!("/api/books/{book_id}/workflows/deploy"),
        &owner_cookie,
        json!({
            "workflow_deployment_id": Uuid::new_v4(), "workflow_id": Uuid::new_v4(),
            "entity_id": entity_id,
            "workflow_name": "Recording startup expense", "description": null,
            "backend_api_calls": ["post_entry"]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
