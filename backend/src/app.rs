//! Router assembly: the routing server (Impl Spec §7.1).
//!
//! One Axum router serves the API, the launcher assets, and (from M5 on)
//! deployed workflow artifacts.

use crate::auth;
use crate::books_api;
use crate::state::SharedState;
use axum::routing::{get, patch, post, put};
use axum::Router;
use std::path::Path;
use tower_http::services::{ServeDir, ServeFile};

pub fn build_router(state: SharedState) -> Router {
    let api = Router::new()
        .route("/health", get(auth::health))
        .route("/auth/config", get(auth::auth_config))
        .route("/auth/me", get(auth::me))
        .route("/auth/:provider/login", get(auth::provider_login))
        .route("/auth/:provider/callback", get(auth::provider_callback))
        .route("/auth/dev-login", post(auth::dev_login))
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .route("/admin/ping", get(auth::admin_ping))
        .route(
            "/books",
            get(books_api::list_books).post(books_api::create_book),
        )
        .route("/books/mine", get(books_api::list_my_books))
        .route("/books/restore", post(books_api::restore_book))
        .route("/books/:book_id/open", post(books_api::open_book))
        .route("/books/:book_id/export", post(books_api::export_book))
        .route("/books/:book_id/entities", get(books_api::list_entities))
        .route(
            "/books/:book_id/resource-types",
            get(books_api::list_resource_types).post(books_api::create_resource_type),
        )
        .route(
            "/books/:book_id/charts",
            get(books_api::list_charts).post(books_api::create_chart),
        )
        .route(
            "/books/:book_id/charts/:chart_id/copy",
            post(books_api::copy_chart),
        )
        .route(
            "/books/:book_id/accounts",
            get(books_api::list_accounts).post(books_api::create_account),
        )
        .route(
            "/books/:book_id/accounts/:account_id",
            patch(books_api::update_account),
        )
        .route(
            "/books/:book_id/accounts/:account_id/active",
            put(books_api::set_account_active),
        )
        .route(
            "/books/:book_id/accounts/:account_id/balance",
            get(books_api::get_balance),
        )
        .route(
            "/books/:book_id/periods",
            get(books_api::list_periods).post(books_api::create_period),
        )
        .route(
            "/books/:book_id/periods/:period_id/close",
            post(books_api::close_period),
        )
        .route(
            "/books/:book_id/periods/:period_id/reopen",
            post(books_api::reopen_period),
        )
        .route(
            "/books/:book_id/entries",
            get(books_api::list_entries).post(books_api::post_entry),
        )
        .route(
            "/books/:book_id/entries/reverse",
            post(books_api::reverse_entry),
        )
        .route("/books/:book_id/audit-log", get(books_api::get_audit_log))
        .route(
            "/books/:book_id/prices",
            get(books_api::list_prices).post(books_api::record_price),
        )
        .route("/books/:book_id/workflows", get(books_api::list_workflows))
        .route(
            "/books/:book_id/workflows/mine",
            get(books_api::my_workflows),
        )
        .route(
            "/books/:book_id/workflows/deploy",
            post(books_api::deploy_workflow),
        )
        .route(
            "/books/:book_id/roles",
            get(books_api::list_roles).post(books_api::create_role),
        )
        .route(
            "/books/:book_id/roles/:role_id/workflows",
            post(books_api::assign_workflow_to_role),
        )
        .route(
            "/books/:book_id/roles/:role_id/users",
            post(books_api::assign_role_to_user),
        );

    let dist = state.config.frontend_dist.clone();
    let index = Path::new(&dist).join("index.html");
    let static_service = ServeDir::new(&dist).not_found_service(ServeFile::new(index));

    // Deployed workflow artifacts (Impl Spec §7.1, §7.4): static assets only
    // — authorization happens at the backend API calls the artifact makes,
    // not at the point of fetching its own HTML/JS.
    let workflows_dir = Path::new(&state.config.dev_artifacts_dir).join("workflows");
    let workflows_service = ServeDir::new(&workflows_dir);

    Router::new()
        .nest("/api", api)
        .nest_service("/workflows", workflows_service)
        .with_state(state)
        .fallback_service(static_service)
}
