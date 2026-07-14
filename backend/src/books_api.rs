//! Book lifecycle and core accounting APIs (Impl Spec §5.3, §5.4, §6.5;
//! Impl Plan M4). Every handler here re-authenticates from the session
//! cookie and re-authorizes against the bootstrap owner — no capability
//! tokens, the backend re-checks context against server-side state on every
//! call (Impl Spec §7.4/§6.5 carried over from workflow-originated calls).

use crate::authz::Action;
use crate::books::{mutate, BookMeta, OpenBook};
use crate::error::ApiError;
use crate::state::SharedState;
use crate::users::User;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn authenticated_owner(state: &SharedState, headers: &HeaderMap) -> Result<User, ApiError> {
    let user = crate::auth::current_user(state, headers)?;
    if let Err(err) = state.authz.authorize(&user, Action::BookApi) {
        state.audit.record(
            "authorization",
            &user.email,
            "denied",
            "book API requires bootstrap owner (v1: no role system until M5)",
        );
        return Err(err);
    }
    Ok(user)
}

/// Authenticate, authorize, and resolve the open book — the entry point
/// every reference/ledger handler below shares.
async fn book_context(
    state: &SharedState,
    headers: &HeaderMap,
    book_id: Uuid,
) -> Result<(User, Arc<OpenBook>), ApiError> {
    let user = authenticated_owner(state, headers).await?;
    let open_book = state.books.get_open(book_id).await?;
    Ok((user, open_book))
}

/// Authenticate and resolve the open book for *any* signed-in user, with no
/// `Action::BookApi` gate. Used only where the engine itself is the sole
/// authorization authority — workflow-originated `post_entry` calls and the
/// "my workflows" launcher menu — matching Impl Spec §6.5: "no capability
/// tokens... the backend re-checks context against server-side state."
async fn open_book_for_any_user(
    state: &SharedState,
    headers: &HeaderMap,
    book_id: Uuid,
) -> Result<(User, Arc<OpenBook>), ApiError> {
    let user = crate::auth::current_user(state, headers)?;
    let open_book = state.books.get_open(book_id).await?;
    Ok((user, open_book))
}

#[derive(Serialize)]
pub struct IdResponse {
    pub id: Uuid,
}

#[derive(Deserialize)]
pub struct EntityFilter {
    pub entity_id: Uuid,
}

#[derive(Deserialize)]
pub struct ChartFilter {
    pub chart_id: Uuid,
}

// ---------------------------------------------------------------------------
// Book lifecycle
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateBookRequest {
    pub name: String,
    pub passphrase: String,
}

#[derive(Serialize)]
pub struct BookResponse {
    pub book_id: Uuid,
    pub name: String,
}

/// POST /api/books — bootstrap-owner-gated (Impl Spec §5.3). Creating a book
/// opens it, so the caller can act immediately.
pub async fn create_book(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<CreateBookRequest>,
) -> Result<Json<BookResponse>, ApiError> {
    let user = crate::auth::current_user(&state, &headers)?;
    if let Err(err) = state.authz.authorize(&user, Action::CreateAccountingBook) {
        state.audit.record(
            "authorization",
            &user.email,
            "denied",
            "create_accounting_book requires bootstrap owner",
        );
        return Err(err);
    }
    if body.name.trim().is_empty() {
        return Err(ApiError::invalid_input("book name must not be empty"));
    }
    if body.passphrase.len() < 8 {
        return Err(ApiError::invalid_input(
            "passphrase must be at least 8 characters",
        ));
    }
    let meta = state
        .books
        .create(body.name, &body.passphrase, &user.email)
        .await?;
    Ok(Json(BookResponse {
        book_id: meta.book_id,
        name: meta.name,
    }))
}

#[derive(Deserialize)]
pub struct OpenBookRequest {
    pub passphrase: String,
}

/// POST /api/books/:book_id/open — owner passphrase → key into memory
/// (Impl Spec §5.4). A wrong passphrase returns 401 WRONG_PASSPHRASE.
pub async fn open_book(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<OpenBookRequest>,
) -> Result<Json<BookResponse>, ApiError> {
    let user = crate::auth::current_user(&state, &headers)?;
    if let Err(err) = state.authz.authorize(&user, Action::OpenBook) {
        state.audit.record(
            "authorization",
            &user.email,
            "denied",
            "open_book requires bootstrap owner",
        );
        return Err(err);
    }
    let meta = state.books.open(book_id, &body.passphrase).await?;
    Ok(Json(BookResponse {
        book_id: meta.book_id,
        name: meta.name,
    }))
}

/// GET /api/books — every book folder on disk, open or not.
pub async fn list_books(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<BookMeta>>, ApiError> {
    let user = crate::auth::current_user(&state, &headers)?;
    if let Err(err) = state.authz.authorize(&user, Action::ListBooks) {
        state.audit.record(
            "authorization",
            &user.email,
            "denied",
            "list_books requires bootstrap owner",
        );
        return Err(err);
    }
    Ok(Json(state.books.list().await?))
}

/// GET /api/books/mine — the launcher's book picker (Impl Plan M6). The
/// bootstrap owner sees every book, exactly as `list_books` does; any other
/// signed-in user sees only *currently open* books where they hold at least
/// one workflow-granting role in some entity — no `Action::BookApi` gate,
/// since the engine's own role assignments are the authority here.
pub async fn list_my_books(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<BookMeta>>, ApiError> {
    let user = crate::auth::current_user(&state, &headers)?;
    if state.authz.is_bootstrap_owner(&user) {
        return Ok(Json(state.books.list().await?));
    }
    let mut visible = Vec::new();
    for open_book in state.books.list_open().await {
        let engine = open_book.engine.read().await;
        if !engine
            .entities_with_workflows_for_user(user.user_id)
            .is_empty()
        {
            visible.push(open_book.meta.clone());
        }
    }
    Ok(Json(visible))
}

// ---------------------------------------------------------------------------
// Reference APIs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateEntityRequest {
    pub op_id: Uuid,
    pub name: String,
}

pub async fn create_entity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateEntityRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.create_entity(body.op_id, user.user_id, &body.name)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_entities(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<Entity>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(engine.list_entities().into_iter().cloned().collect()))
}

/// GET /api/books/:book_id/entities/mine — the launcher's entity picker
/// (Impl Plan M6), the second step after `list_my_books`. The owner sees
/// every entity in the book, exactly as `list_entities` does; any other
/// user sees only entities where they hold at least one workflow-granting
/// role.
pub async fn list_my_entities(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<Entity>>, ApiError> {
    let (user, open_book) = open_book_for_any_user(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    if state.authz.is_bootstrap_owner(&user) {
        return Ok(Json(engine.list_entities().into_iter().cloned().collect()));
    }
    let entity_ids = engine.entities_with_workflows_for_user(user.user_id);
    Ok(Json(
        engine
            .list_entities()
            .into_iter()
            .filter(|e| entity_ids.contains(&e.entity_id))
            .cloned()
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateResourceTypeRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub spec: NewResourceType,
}

pub async fn create_resource_type(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateResourceTypeRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.create_resource_type(body.op_id, user.user_id, body.spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_resource_types(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<ResourceType>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine.list_resource_types().into_iter().cloned().collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateChartRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub spec: NewChart,
}

pub async fn create_chart(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateChartRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.create_chart(body.op_id, user.user_id, body.spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

#[derive(Deserialize)]
pub struct CopyChartRequest {
    pub op_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub activate: bool,
}

pub async fn copy_chart(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, chart_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CopyChartRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let spec = CopyChart {
        source_chart_id: chart_id,
        name: body.name,
        description: body.description,
        activate: body.activate,
    };
    let id = mutate(&open_book, |engine| {
        engine.copy_chart(body.op_id, user.user_id, spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_charts(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Vec<Chart>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .list_charts(filter.entity_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateAccountRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub spec: NewAccount,
}

pub async fn create_account(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateAccountRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.create_account(body.op_id, user.user_id, body.spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_accounts(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<ChartFilter>,
) -> Result<Json<Vec<Account>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .list_accounts(filter.chart_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct UpdateAccountRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub update: UpdateAccountMetadata,
}

pub async fn update_account(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, account_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateAccountRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.update_account_metadata(body.op_id, user.user_id, account_id, body.update)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

#[derive(Deserialize)]
pub struct SetAccountActiveRequest {
    pub op_id: Uuid,
    pub is_active: bool,
}

/// PUT /api/books/:book_id/accounts/:account_id/active — covers both
/// `deactivate_account` (is_active: false) and reactivation.
pub async fn set_account_active(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, account_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SetAccountActiveRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.set_account_active(body.op_id, user.user_id, account_id, body.is_active)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn get_balance(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, account_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<BalanceView>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(engine.get_balance(account_id)?))
}

#[derive(Deserialize)]
pub struct CreatePeriodRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub spec: NewPeriod,
}

pub async fn create_period(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreatePeriodRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.create_period(body.op_id, user.user_id, body.spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

#[derive(Deserialize)]
pub struct PeriodStatusRequest {
    pub op_id: Uuid,
}

pub async fn close_period(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, period_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<PeriodStatusRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.close_period(body.op_id, user.user_id, period_id)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn reopen_period(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, period_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<PeriodStatusRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.reopen_period(body.op_id, user.user_id, period_id)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_periods(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Vec<Period>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .list_periods(filter.entity_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

// ---------------------------------------------------------------------------
// Ledger APIs
// ---------------------------------------------------------------------------

/// POST /api/books/:book_id/entries. When `body.workflow` is present, the
/// caller need not be the bootstrap owner — the engine's own workflow-scoped
/// authorization (Impl Spec §6.5) is the sole gate. Otherwise this is the
/// direct/manual path from M4, unchanged: bootstrap-owner-gated.
pub async fn post_entry(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<NewEntry>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = if body.workflow.is_some() {
        open_book_for_any_user(&state, &headers, book_id).await?
    } else {
        book_context(&state, &headers, book_id).await?
    };
    let id = mutate(&open_book, |engine| engine.post_entry(user.user_id, body)).await?;
    Ok(Json(IdResponse { id }))
}

pub async fn reverse_entry(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<ReverseEntry>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.reverse_entry(user.user_id, body)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_entries(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Vec<JournalEntry>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .list_entries(filter.entity_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

pub async fn get_audit_log(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<EventRecord>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(engine.audit_log().to_vec()))
}

#[derive(Deserialize)]
pub struct RecordPriceRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub price: PriceFact,
}

pub async fn record_price(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<RecordPriceRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.record_price(body.op_id, user.user_id, body.price)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_prices(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Vec<PriceFact>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(engine.list_prices().into_iter().cloned().collect()))
}

// ---------------------------------------------------------------------------
// Workflows and roles (Impl Plan M5)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DeployWorkflowRequest {
    pub workflow_deployment_id: Uuid,
    /// Caller-supplied (Impl Spec §2.9): the artifact itself must embed its
    /// own `workflow_id` before it is deployed, so the engine cannot be the
    /// one to generate it.
    pub workflow_id: Uuid,
    pub entity_id: Uuid,
    pub workflow_name: String,
    pub description: Option<String>,
    pub backend_api_calls: Vec<String>,
    #[serde(default)]
    pub required_inputs: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// POST /api/books/:book_id/workflows/deploy — bootstrap-owner-gated (the
/// developer is the sole deploy authority in v1, Impl Spec §6.2). Reads the
/// artifact the caller already wrote to `dev_artifacts_dir` under
/// `workflow_deployment_id`, hashes it (Impl Spec §7.4 — hashes are the
/// identity authority), and registers the deployment plus its auto-role.
pub async fn deploy_workflow(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<DeployWorkflowRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let dir = crate::dev_artifacts::workflow_dir(
        &state.config.dev_artifacts_dir,
        body.workflow_deployment_id,
    );
    let hashed = crate::dev_artifacts::hash_artifact(&dir)
        .await
        .map_err(ApiError::invalid_input)?;
    let spec = NewWorkflowDeployment {
        workflow_deployment_id: body.workflow_deployment_id,
        workflow_id: body.workflow_id,
        entity_id: body.entity_id,
        workflow_name: body.workflow_name,
        description: body.description,
        artifact_id: body.workflow_deployment_id,
        dev_artifact_path: dir.to_string_lossy().into_owned(),
        manifest_hash: hashed.manifest_hash,
        code_hash: hashed.code_hash,
        frontend_route: format!("/workflows/{}/code/index.html", body.workflow_deployment_id),
        backend_api_calls: body.backend_api_calls,
        required_inputs: body.required_inputs,
        metadata: body.metadata,
    };
    let id = mutate(&open_book, |engine| {
        engine.deploy_workflow(user.user_id, spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

/// GET /api/books/:book_id/workflows — every deployed workflow in the
/// entity (admin view; owner-gated).
pub async fn list_workflows(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Vec<WorkflowDefinition>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .list_workflows(filter.entity_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

/// GET /api/books/:book_id/workflows/mine — the launcher's workflow menu
/// (Impl Spec §7.1): every workflow in the entity the current user actually
/// holds a role for. Any signed-in user, not just the owner — this is the
/// read-side counterpart of the workflow-scoped `post_entry` path.
pub async fn my_workflows(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Vec<WorkflowDefinition>>, ApiError> {
    let (user, open_book) = open_book_for_any_user(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .workflows_authorized_for_user(user.user_id, filter.entity_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateRoleRequest {
    pub op_id: Uuid,
    #[serde(flatten)]
    pub spec: NewRole,
}

pub async fn create_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Json(body): Json<CreateRoleRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.create_role(body.op_id, user.user_id, body.spec)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

pub async fn list_roles(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(book_id): Path<Uuid>,
    Query(filter): Query<EntityFilter>,
) -> Result<Json<Vec<Role>>, ApiError> {
    let (_, open_book) = book_context(&state, &headers, book_id).await?;
    let engine = open_book.engine.read().await;
    Ok(Json(
        engine
            .list_roles(filter.entity_id)
            .into_iter()
            .cloned()
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct AssignWorkflowToRoleRequest {
    pub op_id: Uuid,
    pub workflow_id: Uuid,
}

pub async fn assign_workflow_to_role(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, role_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AssignWorkflowToRoleRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.assign_workflow_to_role(body.op_id, user.user_id, role_id, body.workflow_id)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}

#[derive(Deserialize)]
pub struct AssignRoleToUserRequest {
    pub op_id: Uuid,
    pub user_id: Uuid,
}

pub async fn assign_role_to_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((book_id, role_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AssignRoleToUserRequest>,
) -> Result<Json<IdResponse>, ApiError> {
    let (user, open_book) = book_context(&state, &headers, book_id).await?;
    let id = mutate(&open_book, |engine| {
        engine.assign_role_to_user(body.op_id, user.user_id, role_id, body.user_id)
    })
    .await?;
    Ok(Json(IdResponse { id }))
}
