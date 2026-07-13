//! Shared application state for the routing server.

use crate::audit::OpsAudit;
use crate::auth_provider::ProviderRegistry;
use crate::authz::Authorizer;
use crate::books::BooksRegistry;
use crate::config::ServerConfig;
use crate::sessions::SessionStore;
use crate::users::UserStore;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

pub struct AppState {
    pub config: ServerConfig,
    pub users: UserStore,
    pub sessions: SessionStore,
    pub authz: Authorizer,
    pub audit: OpsAudit,
    /// Authentication domains; runtime-mutable (Theorem T3).
    pub providers: ProviderRegistry,
    /// Outstanding OAuth CSRF `state` tokens: token -> (created, provider_id).
    pub oauth_states: RwLock<HashMap<String, (Instant, String)>>,
    /// Accounting book folders and the subset currently open (Impl Plan M4).
    pub books: BooksRegistry,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let sessions = SessionStore::new(config.session_ttl_seconds);
        let authz = Authorizer::new(&config.bootstrap_owner_email);
        let audit = OpsAudit::new(PathBuf::from(&config.ops_audit_log));
        let providers = ProviderRegistry::from_config(&config.auth_providers);
        let books = BooksRegistry::new(&config.books_dir);
        Self {
            config,
            users: UserStore::new(),
            sessions,
            authz,
            audit,
            providers,
            oauth_states: RwLock::new(HashMap::new()),
            books,
        }
    }
}
