//! Server configuration (Impl Spec §5.3).
//!
//! Lives outside any book folder; contains OAuth client settings and the
//! bootstrap owner identity. Never contains book keys or passphrases.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthConfig {
    pub google: GoogleOAuthConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DevLoginConfig {
    /// Development-only login bypassing OAuth. NEVER enable on a
    /// network-reachable deployment.
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub books_dir: String,
    pub frontend_dist: String,
    pub ops_audit_log: String,
    /// On a fresh install, only this authenticated identity may create or open
    /// books and reach owner-gated endpoints (Impl Spec §5.3).
    pub bootstrap_owner_email: String,
    #[serde(default = "default_session_ttl")]
    pub session_ttl_seconds: u64,
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub dev_login: DevLoginConfig,
}

fn default_session_ttl() -> u64 {
    3600
}

impl ServerConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read config {}: {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| format!("cannot parse config {}: {e}", path.display()))
    }
}
