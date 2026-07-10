//! LedgerZero AccountingEngine.
//!
//! Owns the invariant-enforcing domain logic, posting transitions, and the
//! storage writer boundary (Impl Spec §4). Domain model and invariants arrive
//! in milestone M2; this crate is scaffolded in M0 so the workspace shape is
//! fixed from the start.

/// Engine crate version, re-exported for diagnostics endpoints.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!ENGINE_VERSION.is_empty());
    }
}
