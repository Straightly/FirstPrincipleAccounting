//! Shared application state for the routing server.

use crate::audit::OpsAudit;
use crate::authz::Authorizer;
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
    /// Outstanding OAuth CSRF `state` tokens and their creation time.
    pub oauth_states: RwLock<HashMap<String, Instant>>,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let sessions = SessionStore::new(config.session_ttl_seconds);
        let authz = Authorizer::new(&config.bootstrap_owner_email);
        let audit = OpsAudit::new(PathBuf::from(&config.ops_audit_log));
        Self {
            config,
            users: UserStore::new(),
            sessions,
            authz,
            audit,
            oauth_states: RwLock::new(HashMap::new()),
        }
    }
}
