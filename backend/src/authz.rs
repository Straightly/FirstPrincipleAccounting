//! Authorization framework (Impl Spec §5.1, §5.3, §6.5).
//!
//! Authentication proves who submitted a request; authorization decides
//! whether that identity may perform the requested action. In M1 — before
//! books, roles, and workflow deployments exist — the only durable authority
//! is the bootstrap owner from server config. Workflow-scoped authorization
//! (deployment `backend_api_calls`, execution context) completes in M5.

use crate::error::ApiError;
use crate::users::User;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    CreateAccountingBook,
    OpenBook,
    ListBooks,
    /// Every reference/ledger operation within an already-open book. v1 has
    /// no role system yet (that arrives in M5), so — like book creation and
    /// opening — the only durable authority is the bootstrap owner (Impl
    /// Spec §5.3). Workflow-scoped, per-book authorization replaces this
    /// blanket check once roles and workflow deployments exist.
    BookApi,
    /// Impl Plan M9: releases a book from the running process's in-memory
    /// open-books map. Bootstrap-owner-gated directly, like `OpenBook` —
    /// may need to act on a book that isn't necessarily open.
    CloseBook,
    /// Impl Plan M9: copies a book's already-encrypted files verbatim to an
    /// operator-chosen filesystem location — no decryption, so this needs
    /// no book-scoped context beyond the id itself.
    BackupBook,
    /// Impl Plan M9: copies a book's files back from a location, id read
    /// from the location's own plaintext `book.json`. Bootstrap-owner-gated
    /// directly, like `CreateAccountingBook`/`OpenBook` — the target book
    /// need not already exist, let alone be open.
    RestoreBook,
    AdminPing,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::CreateAccountingBook => "create_accounting_book",
            Action::OpenBook => "open_book",
            Action::ListBooks => "list_books",
            Action::BookApi => "book_api",
            Action::CloseBook => "close_book",
            Action::BackupBook => "backup_book",
            Action::RestoreBook => "restore_book",
            Action::AdminPing => "admin_ping",
        }
    }
}

const OWNER_ACTIONS: [Action; 8] = [
    Action::CreateAccountingBook,
    Action::OpenBook,
    Action::ListBooks,
    Action::BookApi,
    Action::CloseBook,
    Action::BackupBook,
    Action::RestoreBook,
    Action::AdminPing,
];

pub struct Authorizer {
    bootstrap_owner_email: String,
}

impl Authorizer {
    pub fn new(bootstrap_owner_email: &str) -> Self {
        Self {
            bootstrap_owner_email: bootstrap_owner_email.trim().to_lowercase(),
        }
    }

    pub fn is_bootstrap_owner(&self, user: &User) -> bool {
        user.email.trim().to_lowercase() == self.bootstrap_owner_email
    }

    /// Actions the user may currently perform, reported by /api/auth/me.
    pub fn allowed_actions(&self, user: &User) -> Vec<&'static str> {
        if self.is_bootstrap_owner(user) {
            OWNER_ACTIONS.iter().map(|a| a.name()).collect()
        } else {
            Vec::new()
        }
    }

    pub fn authorize(&self, user: &User, action: Action) -> Result<(), ApiError> {
        if self.is_bootstrap_owner(user) {
            Ok(())
        } else {
            Err(ApiError::unauthorized_api(format!(
                "user is not authorized for '{}' (fresh install: owner-gated)",
                action.name()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn user(email: &str) -> User {
        User {
            user_id: Uuid::new_v4(),
            display_name: "T".to_string(),
            email: email.to_string(),
        }
    }

    #[test]
    fn owner_matching_is_case_insensitive() {
        let authz = Authorizer::new("Zhian.Job@gmail.com");
        assert!(authz.is_bootstrap_owner(&user("zhian.job@gmail.com")));
        assert!(!authz.is_bootstrap_owner(&user("someone.else@gmail.com")));
    }

    #[test]
    fn non_owner_is_denied_with_structured_error() {
        let authz = Authorizer::new("owner@example.com");
        let err = authz
            .authorize(&user("other@example.com"), Action::AdminPing)
            .unwrap_err();
        assert_eq!(err.error_code, "UNAUTHORIZED_API");
        assert_eq!(err.status, 403);
    }

    #[test]
    fn owner_has_all_bootstrap_actions() {
        let authz = Authorizer::new("owner@example.com");
        let actions = authz.allowed_actions(&user("owner@example.com"));
        assert!(actions.contains(&"create_accounting_book"));
        assert!(actions.contains(&"open_book"));
    }
}
