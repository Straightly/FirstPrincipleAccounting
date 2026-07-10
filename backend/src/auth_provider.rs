//! Pluggable authentication domains (Theorems T2/T3 in
//! docs/LedgerZero_Theorems.md).
//!
//! `IdentityProvider` is the interface a new authentication domain implements.
//! `OidcProvider` implements it generically from pure data, so any OIDC/OAuth2
//! domain (Google, Microsoft, an enterprise IdP) is a configuration record,
//! not code. `ProviderRegistry` is runtime-mutable: domains can be added while
//! the application is running.

use crate::config::AuthProviderConfig;
use crate::error::ApiError;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What every authentication domain must deliver: a verified external identity.
/// Everything downstream (AKA table, users, sessions, authorization) is
/// provider-blind (Theorem T5).
#[derive(Debug, Clone)]
pub struct AuthenticatedIdentity {
    pub provider_id: String,
    pub subject: String,
    pub email: String,
    pub email_verified: bool,
    pub display_name: String,
}

/// Interface for an authentication domain (Theorem T2).
///
/// OIDC/OAuth2 domains use `OidcProvider`. Other protocols (e.g. SAML) are
/// added as new implementations of this trait — never by modifying existing
/// authentication logic.
pub trait IdentityProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn display_name(&self) -> &str;
    /// Where to send the user's browser to authenticate.
    fn authorization_url(&self, csrf_state: &str) -> String;
    /// Exchange the callback code for a verified identity.
    fn exchange_code<'a>(
        &'a self,
        code: &'a str,
    ) -> BoxFuture<'a, Result<AuthenticatedIdentity, ApiError>>;
}

// ---------- generic OIDC/OAuth2 provider (data-driven) ----------

pub struct OidcProvider {
    config: AuthProviderConfig,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct UserInfo {
    sub: String,
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl OidcProvider {
    pub fn new(config: AuthProviderConfig) -> Self {
        Self { config }
    }
}

impl IdentityProvider for OidcProvider {
    fn provider_id(&self) -> &str {
        &self.config.id
    }

    fn display_name(&self) -> &str {
        &self.config.display_name
    }

    fn authorization_url(&self, csrf_state: &str) -> String {
        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.auth_url,
            urlencode(&self.config.client_id),
            urlencode(&self.config.redirect_url),
            urlencode(&self.config.scopes),
            urlencode(csrf_state),
        )
    }

    fn exchange_code<'a>(
        &'a self,
        code: &'a str,
    ) -> BoxFuture<'a, Result<AuthenticatedIdentity, ApiError>> {
        Box::pin(async move {
            let client = reqwest::Client::new();
            let token: TokenResponse = client
                .post(&self.config.token_url)
                .form(&[
                    ("code", code),
                    ("client_id", self.config.client_id.as_str()),
                    ("client_secret", self.config.client_secret.as_str()),
                    ("redirect_uri", self.config.redirect_url.as_str()),
                    ("grant_type", "authorization_code"),
                ])
                .send()
                .await
                .map_err(|e| ApiError::internal(format!("token exchange failed: {e}")))?
                .json()
                .await
                .map_err(|e| ApiError::internal(format!("token response invalid: {e}")))?;

            let info: UserInfo = client
                .get(&self.config.userinfo_url)
                .bearer_auth(&token.access_token)
                .send()
                .await
                .map_err(|e| ApiError::internal(format!("userinfo failed: {e}")))?
                .json()
                .await
                .map_err(|e| ApiError::internal(format!("userinfo response invalid: {e}")))?;

            let Some(email) = info.email else {
                return Err(ApiError::unauthenticated(
                    "identity has no email claim from the provider",
                ));
            };
            Ok(AuthenticatedIdentity {
                provider_id: self.config.id.clone(),
                subject: info.sub,
                display_name: info.name.unwrap_or_else(|| email.clone()),
                email,
                // Absent claim is treated as verified; explicit false is not.
                email_verified: info.email_verified != Some(false),
            })
        })
    }
}

// ---------- runtime-mutable registry (Theorem T3) ----------

#[derive(Default)]
pub struct ProviderRegistry {
    inner: RwLock<HashMap<String, Arc<dyn IdentityProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the initial registry from configuration. Providers without a
    /// client_id are skipped as unconfigured.
    pub fn from_config(configs: &[AuthProviderConfig]) -> Self {
        let registry = Self::new();
        for config in configs {
            if !config.client_id.is_empty() {
                registry.register(Arc::new(OidcProvider::new(config.clone())));
            }
        }
        registry
    }

    /// Add an authentication domain. Callable at any time while the
    /// application is running; the domain is immediately usable.
    pub fn register(&self, provider: Arc<dyn IdentityProvider>) {
        self.inner
            .write()
            .expect("provider registry lock poisoned")
            .insert(provider.provider_id().to_string(), provider);
    }

    /// Remove (disable) an authentication domain at runtime.
    pub fn deregister(&self, provider_id: &str) {
        self.inner
            .write()
            .expect("provider registry lock poisoned")
            .remove(provider_id);
    }

    pub fn get(&self, provider_id: &str) -> Option<Arc<dyn IdentityProvider>> {
        self.inner
            .read()
            .expect("provider registry lock poisoned")
            .get(provider_id)
            .cloned()
    }

    /// (id, display_name) pairs for the launcher's login buttons.
    pub fn list(&self) -> Vec<(String, String)> {
        let mut providers: Vec<(String, String)> = self
            .inner
            .read()
            .expect("provider registry lock poisoned")
            .values()
            .map(|p| (p.provider_id().to_string(), p.display_name().to_string()))
            .collect();
        providers.sort();
        providers
    }
}
