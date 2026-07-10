//! Router assembly: the routing server (Impl Spec §7.1).
//!
//! One Axum router serves the API, the launcher assets, and (from M5 on)
//! deployed workflow artifacts.

use crate::auth;
use crate::state::SharedState;
use axum::routing::{get, post};
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
        .route("/admin/ping", get(auth::admin_ping));

    let dist = state.config.frontend_dist.clone();
    let index = Path::new(&dist).join("index.html");
    let static_service = ServeDir::new(&dist).not_found_service(ServeFile::new(index));

    Router::new()
        .nest("/api", api)
        .with_state(state)
        .fallback_service(static_service)
}
