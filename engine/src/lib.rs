//! LedgerZero AccountingEngine.
//!
//! Owns the invariant-enforcing domain logic and posting transitions
//! (Impl Spec §4). M2 delivers the engine as a pure library over an
//! in-memory event log:
//!
//! - [`domain`] — the core data model (Impl Spec §2) and event envelope.
//! - [`amount`] — exact `Decimal(18,8)` fixed-point money; no floats.
//! - [`error`] — the complete structured error catalog (Impl Spec §4.4).
//! - [`engine`] — [`engine::AccountingEngine`]: posted-or-rejected mutations,
//!   §4.1 invariants, balances, price projection, replay.
//!
//! The async storage boundary (M3) will persist [`domain::EventRecord`]s and
//! reference state, and rebuild everything through [`engine::EngineState::replay`]
//! (Theorem T1: nothing above this crate may depend on the storage medium).

pub mod amount;
pub mod domain;
pub mod engine;
pub mod error;
pub mod types;

pub use amount::Amount;
pub use engine::{AccountingEngine, EngineState};
pub use error::{EngineError, ErrorCode};

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
