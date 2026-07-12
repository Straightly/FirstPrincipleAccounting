//! Core data model — Impl Spec §2.
//!
//! Every type here is a plain serializable value. System-derived fields
//! (`normal_balance`, `posted_at`, `period_id`) are set by the engine, never
//! taken from input.

use crate::amount::Amount;
use crate::types::{Date, TimestampMs};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Reference model
// ---------------------------------------------------------------------------

/// Managed or external party (Impl Spec §2.9). "Counterparty" is a contextual
/// label, not a type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub entity_id: Uuid,
    pub book_id: Uuid,
    pub name: String,
    pub created_at: TimestampMs,
}

/// Impl Spec §2.1 — replaces the former resource-type enum + currency code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceKind {
    Currency,
    Inventory,
    Commodity,
    DigitalAsset,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceType {
    pub resource_type_id: Uuid,
    pub book_id: Uuid,
    pub name: String,
    pub kind: ResourceKind,
    /// e.g. ISO 4217; required for CURRENCY.
    pub code: String,
    /// Exactly one per resource type (e.g. "USD", "each", "kg").
    pub unit_of_measure: String,
    /// Decimal places meaningful for this unit.
    pub precision: u8,
    pub metadata: serde_json::Value,
}

/// Impl Spec §2.2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    pub chart_id: Uuid,
    pub entity_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: TimestampMs,
}

/// Impl Spec §2.3. ASSET and EXPENSE are DEBIT-normal; LIABILITY, EQUITY,
/// REVENUE are CREDIT-normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NormalBalance {
    Debit,
    Credit,
}

impl AccountType {
    /// `normal_balance` is always derived, never stored from input.
    pub fn normal_balance(&self) -> NormalBalance {
        match self {
            AccountType::Asset | AccountType::Expense => NormalBalance::Debit,
            AccountType::Liability | AccountType::Equity | AccountType::Revenue => {
                NormalBalance::Credit
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub account_id: Uuid,
    pub chart_id: Uuid,
    pub entity_id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    /// Derived from `account_type`; recorded for readability.
    pub normal_balance: NormalBalance,
    pub resource_type_id: Uuid,
    pub parent_account_id: Option<Uuid>,
    pub is_active: bool,
    /// Account-defined validation invoked by the engine (Impl Spec §4.1.7).
    /// v1 vocabulary: `require_memo: bool`, `max_amount: "decimal"`,
    /// `side: "debit_only" | "credit_only"`. Unknown keys are rejected at
    /// account creation.
    pub validation_rules: serde_json::Value,
    pub created_at: TimestampMs,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PeriodStatus {
    Open,
    Closed,
}

/// Impl Spec §2.7. Periods must not overlap within an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Period {
    pub period_id: Uuid,
    pub entity_id: Uuid,
    pub name: String,
    pub start_date: Date,
    pub end_date: Date,
    pub status: PeriodStatus,
}

/// A price is a fact (Impl Spec §2.6): 1 unit of `base` = `rate` units of
/// `quote`, as of `as_of`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceFact {
    pub base_resource_type_id: Uuid,
    pub quote_resource_type_id: Uuid,
    pub rate: Amount,
    pub as_of: Date,
}

// ---------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------

/// Impl Spec §2.4 event catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Accounting,
    Administrative,
    WorkflowDeployment,
    RoleAssignment,
    SubBookLink,
    ConsolidationRule,
    Consolidation,
    Price,
    PeriodStatus,
    AccountStatus,
    SystemDerived,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntrySource {
    Manual,
    Workflow,
    Derived,
    Admin,
    Restore,
    System,
}

/// Workflow execution context (populated from M5/M6 onward; None = manual).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowContext {
    pub workflow_id: Uuid,
    pub workflow_deployment_id: Uuid,
    pub workflow_execution_id: Uuid,
}

/// Impl Spec §2.5. Amounts are always denominated in the account's own
/// resource unit; exactly one of debit/credit is set, each `>= 0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalLine {
    pub line_id: Uuid,
    pub account_id: Uuid,
    pub debit_amount: Option<Amount>,
    pub credit_amount: Option<Amount>,
    pub memo: Option<String>,
    pub metadata: serde_json::Value,
}

/// Impl Spec §2.4 — an immutable POSTED ledger entry. There is no status
/// field: an entry that exists was posted; a rejected entry never exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Client-generated; the idempotency key; equals the event_id.
    pub entry_id: Uuid,
    pub book_id: Uuid,
    pub entity_id: Uuid,
    /// Economic date — may differ from posted_at.
    pub entry_date: Date,
    /// System-set, immutable.
    pub posted_at: TimestampMs,
    pub posted_by: Uuid,
    pub workflow: Option<WorkflowContext>,
    pub event_type: EventType,
    pub description: String,
    pub reversal_of: Option<Uuid>,
    /// Resolved by the engine from entry_date; enforced open on post.
    pub period_id: Uuid,
    /// Required when lines span resource types (Impl Spec §2.6). These
    /// recorded prices are authoritative for this entry forever.
    pub prices: Vec<PriceFact>,
    pub source: EntrySource,
    pub metadata: serde_json::Value,
    pub lines: Vec<JournalLine>,
}

// ---------------------------------------------------------------------------
// Event envelope
// ---------------------------------------------------------------------------

/// What an event did to the book. The event log plus these payloads is the
/// sole source of truth; every projection is rebuilt from it (Impl Spec §3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventPayload {
    EntityCreated {
        entity: Entity,
    },
    ResourceTypeCreated {
        resource_type: ResourceType,
    },
    /// `deactivated_chart_id`: the previously active chart, if this creation
    /// took over the active slot.
    ChartCreated {
        chart: Chart,
        deactivated_chart_id: Option<Uuid>,
    },
    AccountCreated {
        account: Account,
    },
    AccountMetadataUpdated {
        account_id: Uuid,
        name: Option<String>,
        code: Option<String>,
        metadata: Option<serde_json::Value>,
    },
    AccountStatusChanged {
        account_id: Uuid,
        is_active: bool,
    },
    PeriodCreated {
        period: Period,
    },
    PeriodStatusChanged {
        period_id: Uuid,
        status: PeriodStatus,
    },
    PriceRecorded {
        price: PriceFact,
    },
    EntryPosted {
        entry: JournalEntry,
    },
}

impl EventPayload {
    pub fn event_type(&self) -> EventType {
        match self {
            EventPayload::EntityCreated { .. }
            | EventPayload::ResourceTypeCreated { .. }
            | EventPayload::ChartCreated { .. }
            | EventPayload::AccountCreated { .. }
            | EventPayload::AccountMetadataUpdated { .. }
            | EventPayload::PeriodCreated { .. } => EventType::Administrative,
            EventPayload::AccountStatusChanged { .. } => EventType::AccountStatus,
            EventPayload::PeriodStatusChanged { .. } => EventType::PeriodStatus,
            EventPayload::PriceRecorded { .. } => EventType::Price,
            EventPayload::EntryPosted { entry } => entry.event_type,
        }
    }

    /// The id of the object this event created or affected — the outcome an
    /// idempotent replay returns.
    pub fn outcome_id(&self) -> Uuid {
        match self {
            EventPayload::EntityCreated { entity } => entity.entity_id,
            EventPayload::ResourceTypeCreated { resource_type } => resource_type.resource_type_id,
            EventPayload::ChartCreated { chart, .. } => chart.chart_id,
            EventPayload::AccountCreated { account } => account.account_id,
            EventPayload::AccountMetadataUpdated { account_id, .. } => *account_id,
            EventPayload::AccountStatusChanged { account_id, .. } => *account_id,
            EventPayload::PeriodCreated { period } => period.period_id,
            EventPayload::PeriodStatusChanged { period_id, .. } => *period_id,
            EventPayload::PriceRecorded { .. } => Uuid::nil(),
            EventPayload::EntryPosted { entry } => entry.entry_id,
        }
    }
}

/// The logical event record serialized inside the encrypted book file
/// (Impl Spec §2.4, §3.1). `request` is the canonical client request, kept
/// for idempotency comparison (§4.1.6): a replay with the same `event_id`
/// and identical request returns the original outcome; a different request
/// is IDEMPOTENCY_CONFLICT.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    /// Client-generated idempotency key; equals entry_id for posted entries.
    pub event_id: Uuid,
    pub book_id: Uuid,
    pub event_type: EventType,
    /// System-set, immutable.
    pub occurred_at: TimestampMs,
    pub actor_user_id: Uuid,
    pub workflow: Option<WorkflowContext>,
    /// Canonical client request (JSON), for idempotency comparison.
    pub request: serde_json::Value,
    pub payload: EventPayload,
}
