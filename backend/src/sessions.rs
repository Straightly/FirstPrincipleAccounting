//! Session tokens: short-lived with refresh rotation (Impl Spec §5.2).

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

struct Session {
    user_id: Uuid,
    expires_at: Instant,
}

pub struct SessionStore {
    ttl: Duration,
    inner: RwLock<HashMap<String, Session>>,
}

impl SessionStore {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_seconds),
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub fn create(&self, user_id: Uuid) -> String {
        let token = Uuid::new_v4().to_string();
        self.inner.write().expect("session lock poisoned").insert(
            token.clone(),
            Session {
                user_id,
                expires_at: Instant::now() + self.ttl,
            },
        );
        token
    }

    pub fn lookup(&self, token: &str) -> Option<Uuid> {
        let inner = self.inner.read().expect("session lock poisoned");
        let session = inner.get(token)?;
        if session.expires_at <= Instant::now() {
            None
        } else {
            Some(session.user_id)
        }
    }

    /// Refresh rotation: the presented token is invalidated and a new token
    /// with a fresh TTL is issued (Impl Spec §5.2).
    pub fn rotate(&self, token: &str) -> Option<String> {
        let mut inner = self.inner.write().expect("session lock poisoned");
        let session = inner.remove(token)?;
        if session.expires_at <= Instant::now() {
            return None;
        }
        let new_token = Uuid::new_v4().to_string();
        inner.insert(
            new_token.clone(),
            Session {
                user_id: session.user_id,
                expires_at: Instant::now() + self.ttl,
            },
        );
        Some(new_token)
    }

    pub fn revoke(&self, token: &str) {
        self.inner
            .write()
            .expect("session lock poisoned")
            .remove(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_lookup_rotate_revoke() {
        let store = SessionStore::new(3600);
        let uid = Uuid::new_v4();
        let token = store.create(uid);
        assert_eq!(store.lookup(&token), Some(uid));

        let new_token = store.rotate(&token).expect("rotate works");
        assert_eq!(store.lookup(&token), None, "old token invalidated");
        assert_eq!(store.lookup(&new_token), Some(uid));

        store.revoke(&new_token);
        assert_eq!(store.lookup(&new_token), None);
    }

    #[test]
    fn expired_session_is_rejected() {
        let store = SessionStore::new(0);
        let uid = Uuid::new_v4();
        let token = store.create(uid);
        assert_eq!(store.lookup(&token), None);
        assert!(store.rotate(&token).is_none());
    }
}
