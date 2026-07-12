//! Structured error catalog — Impl Spec §4.4, complete.
//!
//! Every rejection is `{ error_code, message, details }`. Codes for
//! authorization and book lifecycle (`UNAUTHORIZED_*`, `BOOK_NOT_OPEN`,
//! `INVALID_EXECUTION_CONTEXT`) are defined here so the catalog is complete
//! in M2; they are raised by the backend/authorization layers in later
//! milestones.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Debits ≠ credits at the entry's recorded prices.
    UnbalancedEntry,
    /// Entry spans resource types without a recorded price.
    MissingPrice,
    /// Account missing.
    UnknownAccount,
    /// Account deactivated.
    InactiveAccount,
    /// Lines span charts or entities.
    ChartMismatch,
    /// entry_date falls in a CLOSED period.
    PeriodClosed,
    /// entry_date falls in no period of the entity.
    NoOpenPeriod,
    /// Authorization re-check failed: workflow not permitted (M6+).
    UnauthorizedWorkflow,
    /// Authorization re-check failed: API not in deployment's set (M6+).
    UnauthorizedApi,
    /// workflow_execution_id inconsistent with its context (M6+).
    InvalidExecutionContext,
    /// Known client ID with a different payload.
    IdempotencyConflict,
    /// Book key not loaded in backend memory (M3+).
    BookNotOpen,
    /// Account-defined validation rule failed (details name the rule).
    ValidationFailed,
    /// Structural/schema failure.
    InvalidInput,
}

impl ErrorCode {
    /// The wire string, e.g. `UNBALANCED_ENTRY`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::UnbalancedEntry => "UNBALANCED_ENTRY",
            ErrorCode::MissingPrice => "MISSING_PRICE",
            ErrorCode::UnknownAccount => "UNKNOWN_ACCOUNT",
            ErrorCode::InactiveAccount => "INACTIVE_ACCOUNT",
            ErrorCode::ChartMismatch => "CHART_MISMATCH",
            ErrorCode::PeriodClosed => "PERIOD_CLOSED",
            ErrorCode::NoOpenPeriod => "NO_OPEN_PERIOD",
            ErrorCode::UnauthorizedWorkflow => "UNAUTHORIZED_WORKFLOW",
            ErrorCode::UnauthorizedApi => "UNAUTHORIZED_API",
            ErrorCode::InvalidExecutionContext => "INVALID_EXECUTION_CONTEXT",
            ErrorCode::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            ErrorCode::BookNotOpen => "BOOK_NOT_OPEN",
            ErrorCode::ValidationFailed => "VALIDATION_FAILED",
            ErrorCode::InvalidInput => "INVALID_INPUT",
        }
    }
}

/// A structured engine rejection. The entry/operation it rejects leaves no
/// trace in the ledger (posted-or-rejected, Impl Spec §2.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineError {
    pub error_code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

impl EngineError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> EngineError {
        EngineError {
            error_code: code,
            message: message.into(),
            details: serde_json::Value::Null,
        }
    }

    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> EngineError {
        EngineError {
            error_code: code,
            message: message.into(),
            details,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> EngineError {
        EngineError::new(ErrorCode::InvalidInput, message)
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error_code.as_str(), self.message)
    }
}

impl std::error::Error for EngineError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_serialize_screaming_snake() {
        let e = EngineError::new(ErrorCode::UnbalancedEntry, "x");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["error_code"], "UNBALANCED_ENTRY");
        assert_eq!(
            serde_json::to_value(ErrorCode::IdempotencyConflict).unwrap(),
            "IDEMPOTENCY_CONFLICT"
        );
    }
}
