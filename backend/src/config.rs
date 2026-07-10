//! Server configuration (Impl Spec §5.3).
//!
//! Lives outside any book folder; contains authentication provider settings
//! and the bootstrap owner identity. Never contains book keys or passphrases.

use serde::Deserialize;
use std::path::Path;

/// One authentication domain as pure data (Theorem T2): any OIDC/OAuth2
/// provider — Google, Microsoft, an enterprise IdP — is a record of this
/// shape, not code.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthProviderConfig {
    /// Stable identifier used in routes (/api/auth/{id}/...) and the AKA table.
    pub id: String,
    pub display_name: String,
    /// OAuth2 authorization endpoint.
    pub auth_url: String,
    /// OAuth2 token endpoint.
    pub token_url: String,
    /// OIDC userinfo endpoint.
    pub userinfo_url: String,
    pub client_id: String,
    pub client_secret: String,
    /// Must match the redirect URI registered with the provider:
    /// {base}/api/auth/{id}/callback
    pub redirect_url: String,
    #[serde(default = "default_scopes")]
    pub scopes: String,
}

fn default_scopes() -> String {
    "openid email profile".to_string()
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
    /// Authentication domains registered at startup. More can be added at
    /// runtime through the provider registry (Theorem T3).
    #[serde(default)]
    pub auth_providers: Vec<AuthProviderConfig>,
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
