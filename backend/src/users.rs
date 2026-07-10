//! User records and the AKA identity table (Impl Spec §2.9).
//!
//! M1 note: users are held in backend memory. Durable user records belong to
//! the AccountingBook and arrive with book storage in M3/M4. Before any book
//! exists, the only durable authority is the bootstrap owner (spec §5.3), so
//! in-memory user records are sufficient for the walking skeleton.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
}

#[derive(Default)]
pub struct UserStore {
    inner: RwLock<Inner>,
}

#[derive(Default)]
struct Inner {
    users: HashMap<Uuid, User>,
    /// AKA table: (auth_provider, subject_id) -> user_id (Impl Spec §2.9).
    aka: HashMap<(String, String), Uuid>,
    by_email: HashMap<String, Uuid>,
}

impl UserStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, user_id: Uuid) -> Option<User> {
        self.inner
            .read()
            .expect("user store lock poisoned")
            .users
            .get(&user_id)
            .cloned()
    }

    /// Find-or-create the authorized user for an authenticated identity.
    /// A known (provider, subject) pair resolves through the AKA table; an
    /// unknown pair with a known email attaches to the existing user (same
    /// trusted verified email = same person); otherwise a new user is created.
    pub fn resolve_identity(
        &self,
        provider: &str,
        subject: &str,
        email: &str,
        display_name: &str,
    ) -> User {
        let mut inner = self.inner.write().expect("user store lock poisoned");
        let key = (provider.to_string(), subject.to_string());
        if let Some(user_id) = inner.aka.get(&key).copied() {
            return inner
                .users
                .get(&user_id)
                .cloned()
                .expect("AKA entry points to existing user");
        }
        let email_key = email.to_lowercase();
        let user_id = match inner.by_email.get(&email_key).copied() {
            Some(existing) => existing,
            None => {
                let user_id = Uuid::new_v4();
                inner.users.insert(
                    user_id,
                    User {
                        user_id,
                        display_name: display_name.to_string(),
                        email: email.to_string(),
                    },
                );
                inner.by_email.insert(email_key, user_id);
                user_id
            }
        };
        inner.aka.insert(key, user_id);
        inner
            .users
            .get(&user_id)
            .cloned()
            .expect("user just ensured")
    }
}
