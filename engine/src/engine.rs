//! The `AccountingEngine` — sole writer, invariant enforcer (Impl Spec §4).
//!
//! M2 shape: a pure library over an in-memory event log. Every mutation is
//! validated (posted-or-rejected), then applied as an `EventRecord` through
//! the single `EngineState::apply` path — the same path `replay` uses to
//! rebuild all projections from the log, which is what makes the two
//! provably equal (Impl Plan M2 replay tests). The async storage boundary
//! arrives in M3 and wraps this state without changing it (Theorem T1).

use crate::amount::{Amount, Rational, SCALE};
use crate::domain::*;
use crate::error::{EngineError, ErrorCode};
use crate::types::{Clock, Date, SystemClock};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mutation inputs (client-supplied; canonicalized for idempotency)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewResourceType {
    pub name: String,
    pub kind: ResourceKind,
    pub code: String,
    pub unit_of_measure: String,
    pub precision: u8,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewChart {
    pub entity_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Make this the entity's active chart. The entity's first chart is
    /// always activated regardless (exactly-one-active, Impl Spec §2.2).
    pub activate: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CopyChart {
    pub source_chart_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub activate: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewAccount {
    pub chart_id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    pub resource_type_id: Uuid,
    pub parent_account_id: Option<Uuid>,
    #[serde(default)]
    pub validation_rules: Value,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateAccountMetadata {
    pub name: Option<String>,
    pub code: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewPeriod {
    pub entity_id: Uuid,
    pub name: String,
    pub start_date: Date,
    pub end_date: Date,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewLine {
    pub line_id: Uuid,
    pub account_id: Uuid,
    pub debit_amount: Option<Amount>,
    pub credit_amount: Option<Amount>,
    pub memo: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewEntry {
    /// Client-generated; the idempotency key (Impl Spec §2.4).
    pub entry_id: Uuid,
    pub entity_id: Uuid,
    pub entry_date: Date,
    pub description: String,
    pub lines: Vec<NewLine>,
    #[serde(default)]
    pub prices: Vec<PriceFact>,
    pub source: EntrySource,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReverseEntry {
    /// Client-generated id for the reversal entry itself.
    pub new_entry_id: Uuid,
    pub original_entry_id: Uuid,
    pub entry_date: Date,
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

// ---------------------------------------------------------------------------
// Read views
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BalanceView {
    pub account_id: Uuid,
    pub resource_type_id: Uuid,
    pub debit_total: Amount,
    pub credit_total: Amount,
    /// debit_total − credit_total.
    pub net: Amount,
    /// `net` signed by the account's normal balance (positive = normal).
    pub natural: Amount,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrialBalanceRow {
    pub account_id: Uuid,
    pub name: String,
    pub account_type: AccountType,
    pub resource_type_id: Uuid,
    pub debit_total: Amount,
    pub credit_total: Amount,
    pub natural: Amount,
}

/// Expanded-equation check A = L + E + (R − X), per resource type
/// (Impl Plan M2). Because amounts in different units never sum directly,
/// the equation is evaluated within each unit; cross-unit entries shift
/// value between units by exactly their recorded prices, and that shift is
/// `cross_unit_net`. The equation holds iff
/// `assets − liabilities − equity − (revenue − expenses) == cross_unit_net`
/// — which reduces to the textbook `== 0` for books without cross-unit
/// entries.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitEquation {
    pub resource_type_id: Uuid,
    pub assets: Amount,
    pub liabilities: Amount,
    pub equity: Amount,
    pub revenue: Amount,
    pub expenses: Amount,
    pub cross_unit_net: Amount,
    pub holds: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquationReport {
    pub chart_id: Uuid,
    pub units: Vec<UnitEquation>,
    pub holds: bool,
}

// ---------------------------------------------------------------------------
// Engine state (all projections; rebuilt from the log by `replay`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AccountBalance {
    pub debit_total: Amount,
    pub credit_total: Amount,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceProjectionEntry {
    pub price: PriceFact,
    /// Log index of the recording event — later wins on equal `as_of`.
    pub seq: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct IdempotencyRecord {
    request: Value,
    outcome_id: Uuid,
}

/// Everything the engine knows: the event log (source of truth) plus every
/// in-memory projection. `PartialEq` exists so tests can prove that replayed
/// state equals incrementally maintained state.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineState {
    book_id: Uuid,
    log: Vec<EventRecord>,
    entities: BTreeMap<Uuid, Entity>,
    resource_types: BTreeMap<Uuid, ResourceType>,
    charts: BTreeMap<Uuid, Chart>,
    accounts: BTreeMap<Uuid, Account>,
    periods: BTreeMap<Uuid, Period>,
    entries: BTreeMap<Uuid, JournalEntry>,
    balances: BTreeMap<Uuid, AccountBalance>,
    prices: BTreeMap<(Uuid, Uuid), PriceProjectionEntry>,
    idempotency: BTreeMap<Uuid, IdempotencyRecord>,
}

impl EngineState {
    pub fn new(book_id: Uuid) -> EngineState {
        EngineState {
            book_id,
            log: Vec::new(),
            entities: BTreeMap::new(),
            resource_types: BTreeMap::new(),
            charts: BTreeMap::new(),
            accounts: BTreeMap::new(),
            periods: BTreeMap::new(),
            entries: BTreeMap::new(),
            balances: BTreeMap::new(),
            prices: BTreeMap::new(),
            idempotency: BTreeMap::new(),
        }
    }

    /// Rebuild the complete state from an event log — the M2/M3 replay
    /// contract (Impl Spec §3.1): the log is the sole source of truth and
    /// every projection is derivable from it.
    pub fn replay(book_id: Uuid, events: &[EventRecord]) -> Result<EngineState, EngineError> {
        let mut state = EngineState::new(book_id);
        for record in events {
            if record.book_id != book_id {
                return Err(EngineError::invalid_input(format!(
                    "event {} belongs to book {}, not {}",
                    record.event_id, record.book_id, book_id
                )));
            }
            state.apply(record.clone());
        }
        Ok(state)
    }

    /// The single state-transition function. Mutations call this after
    /// validation; `replay` calls it for every recorded event. Nothing else
    /// writes projections.
    fn apply(&mut self, record: EventRecord) {
        let seq = self.log.len();
        self.idempotency.insert(
            record.event_id,
            IdempotencyRecord {
                request: record.request.clone(),
                outcome_id: record.payload.outcome_id(),
            },
        );
        match &record.payload {
            EventPayload::EntityCreated { entity } => {
                self.entities.insert(entity.entity_id, entity.clone());
            }
            EventPayload::ResourceTypeCreated { resource_type } => {
                self.resource_types
                    .insert(resource_type.resource_type_id, resource_type.clone());
            }
            EventPayload::ChartCreated {
                chart,
                deactivated_chart_id,
            } => {
                if let Some(old_id) = deactivated_chart_id {
                    if let Some(old) = self.charts.get_mut(old_id) {
                        old.is_active = false;
                    }
                }
                self.charts.insert(chart.chart_id, chart.clone());
            }
            EventPayload::AccountCreated { account } => {
                self.accounts.insert(account.account_id, account.clone());
                self.balances
                    .insert(account.account_id, AccountBalance::default());
            }
            EventPayload::AccountMetadataUpdated {
                account_id,
                name,
                code,
                metadata,
            } => {
                if let Some(account) = self.accounts.get_mut(account_id) {
                    if let Some(name) = name {
                        account.name = name.clone();
                    }
                    if let Some(code) = code {
                        account.code = Some(code.clone());
                    }
                    if let Some(metadata) = metadata {
                        account.metadata = metadata.clone();
                    }
                }
            }
            EventPayload::AccountStatusChanged {
                account_id,
                is_active,
            } => {
                if let Some(account) = self.accounts.get_mut(account_id) {
                    account.is_active = *is_active;
                }
            }
            EventPayload::PeriodCreated { period } => {
                self.periods.insert(period.period_id, period.clone());
            }
            EventPayload::PeriodStatusChanged { period_id, status } => {
                if let Some(period) = self.periods.get_mut(period_id) {
                    period.status = *status;
                }
            }
            EventPayload::PriceRecorded { price } => {
                Self::project_price(&mut self.prices, price, seq);
            }
            EventPayload::EntryPosted { entry } => {
                for line in &entry.lines {
                    let balance = self.balances.entry(line.account_id).or_default();
                    if let Some(debit) = line.debit_amount {
                        balance.debit_total = balance
                            .debit_total
                            .checked_add(debit)
                            .expect("balance overflow was pre-checked at validation");
                    }
                    if let Some(credit) = line.credit_amount {
                        balance.credit_total = balance
                            .credit_total
                            .checked_add(credit)
                            .expect("balance overflow was pre-checked at validation");
                    }
                }
                // Entry-embedded prices feed the price projection
                // (Impl Spec §2.6) but never re-check posted entries.
                for price in &entry.prices {
                    Self::project_price(&mut self.prices, price, seq);
                }
                self.entries.insert(entry.entry_id, entry.clone());
            }
        }
        self.log.push(record);
    }

    fn project_price(
        prices: &mut BTreeMap<(Uuid, Uuid), PriceProjectionEntry>,
        price: &PriceFact,
        seq: usize,
    ) {
        let key = (price.base_resource_type_id, price.quote_resource_type_id);
        let newer = match prices.get(&key) {
            Some(existing) => {
                (price.as_of.clone(), seq) >= (existing.price.as_of.clone(), existing.seq)
            }
            None => true,
        };
        if newer {
            prices.insert(
                key,
                PriceProjectionEntry {
                    price: price.clone(),
                    seq,
                },
            );
        }
    }

    pub fn book_id(&self) -> Uuid {
        self.book_id
    }

    pub fn log(&self) -> &[EventRecord] {
        &self.log
    }
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

pub struct AccountingEngine {
    clock: Box<dyn Clock>,
    state: EngineState,
}

impl AccountingEngine {
    pub fn new(book_id: Uuid, clock: Box<dyn Clock>) -> AccountingEngine {
        AccountingEngine {
            clock,
            state: EngineState::new(book_id),
        }
    }

    pub fn with_system_clock(book_id: Uuid) -> AccountingEngine {
        AccountingEngine::new(book_id, Box::new(SystemClock))
    }

    /// Resume from previously loaded state (M3 uses this after decrypt+replay).
    pub fn from_state(state: EngineState, clock: Box<dyn Clock>) -> AccountingEngine {
        AccountingEngine { clock, state }
    }

    pub fn state(&self) -> &EngineState {
        &self.state
    }

    pub fn book_id(&self) -> Uuid {
        self.state.book_id
    }

    /// The full event log — the operational + accounting audit trail.
    pub fn audit_log(&self) -> &[EventRecord] {
        self.state.log()
    }

    // -- idempotency ---------------------------------------------------------

    /// §4.1.6: unknown id proceeds (Ok(None)); known id with identical
    /// request returns the original outcome (Ok(Some(id))); known id with a
    /// different request is IDEMPOTENCY_CONFLICT.
    fn check_idempotency(&self, op_id: Uuid, request: &Value) -> Result<Option<Uuid>, EngineError> {
        if op_id.is_nil() {
            return Err(EngineError::invalid_input(
                "client-generated id must not be nil",
            ));
        }
        match self.state.idempotency.get(&op_id) {
            None => Ok(None),
            Some(existing) if existing.request == *request => Ok(Some(existing.outcome_id)),
            Some(_) => Err(EngineError::with_details(
                ErrorCode::IdempotencyConflict,
                "known client id with a different payload",
                json!({ "id": op_id }),
            )),
        }
    }

    fn record(
        &mut self,
        event_id: Uuid,
        actor: Uuid,
        request: Value,
        payload: EventPayload,
    ) -> Uuid {
        let outcome = payload.outcome_id();
        let record = EventRecord {
            event_id,
            book_id: self.state.book_id,
            event_type: payload.event_type(),
            occurred_at: self.clock.now_ms(),
            actor_user_id: actor,
            workflow: None,
            request,
            payload,
        };
        self.state.apply(record);
        outcome
    }

    // -- reference mutations --------------------------------------------------

    pub fn create_entity(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        name: &str,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "create_entity", "name": name });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        if name.trim().is_empty() {
            return Err(EngineError::invalid_input("entity name must not be empty"));
        }
        if self.state.entities.values().any(|e| e.name == name) {
            return Err(EngineError::invalid_input(format!(
                "entity name already exists: {name:?}"
            )));
        }
        let entity = Entity {
            entity_id: Uuid::new_v4(),
            book_id: self.state.book_id,
            name: name.to_string(),
            created_at: self.clock.now_ms(),
        };
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::EntityCreated { entity },
        ))
    }

    pub fn create_resource_type(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        spec: NewResourceType,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "create_resource_type", "spec": spec });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        if spec.name.trim().is_empty() || spec.unit_of_measure.trim().is_empty() {
            return Err(EngineError::invalid_input(
                "resource type needs a name and a unit_of_measure",
            ));
        }
        if spec.kind == ResourceKind::Currency && spec.code.trim().is_empty() {
            return Err(EngineError::invalid_input(
                "code is required for CURRENCY resource types",
            ));
        }
        if self
            .state
            .resource_types
            .values()
            .any(|r| r.name == spec.name)
        {
            return Err(EngineError::invalid_input(format!(
                "resource type name already exists: {:?}",
                spec.name
            )));
        }
        let resource_type = ResourceType {
            resource_type_id: Uuid::new_v4(),
            book_id: self.state.book_id,
            name: spec.name,
            kind: spec.kind,
            code: spec.code,
            unit_of_measure: spec.unit_of_measure,
            precision: spec.precision,
            metadata: spec.metadata,
        };
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::ResourceTypeCreated { resource_type },
        ))
    }

    pub fn create_chart(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        spec: NewChart,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "create_chart", "spec": spec });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        if !self.state.entities.contains_key(&spec.entity_id) {
            return Err(EngineError::invalid_input("unknown entity"));
        }
        if spec.name.trim().is_empty() {
            return Err(EngineError::invalid_input("chart name must not be empty"));
        }
        if self
            .state
            .charts
            .values()
            .any(|c| c.entity_id == spec.entity_id && c.name == spec.name)
        {
            return Err(EngineError::invalid_input(format!(
                "chart name already exists in entity: {:?}",
                spec.name
            )));
        }
        let current_active = self
            .state
            .charts
            .values()
            .find(|c| c.entity_id == spec.entity_id && c.is_active)
            .map(|c| c.chart_id);
        // The entity's first chart is always active (exactly-one-active rule).
        let is_active = spec.activate || current_active.is_none();
        let deactivated_chart_id = if spec.activate { current_active } else { None };
        let chart = Chart {
            chart_id: Uuid::new_v4(),
            entity_id: spec.entity_id,
            name: spec.name,
            description: spec.description,
            is_active,
            created_at: self.clock.now_ms(),
        };
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::ChartCreated {
                chart,
                deactivated_chart_id,
            },
        ))
    }

    /// Duplicates a chart's accounts under a new chart in the same entity
    /// (e.g. a proposed chart to review before activating). Parents are
    /// copied before children so `parent_account_id` remaps to the new
    /// chart's ids; balances and transaction history are never copied.
    /// Emits one `ChartCreated` event (the idempotency-tracked event, keyed
    /// by `op_id`) followed by one `AccountCreated` event per copied
    /// account.
    pub fn copy_chart(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        spec: CopyChart,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "copy_chart", "spec": spec });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        let source = self
            .state
            .charts
            .get(&spec.source_chart_id)
            .ok_or_else(|| EngineError::invalid_input("unknown source chart"))?
            .clone();
        if spec.name.trim().is_empty() {
            return Err(EngineError::invalid_input("chart name must not be empty"));
        }
        if self
            .state
            .charts
            .values()
            .any(|c| c.entity_id == source.entity_id && c.name == spec.name)
        {
            return Err(EngineError::invalid_input(format!(
                "chart name already exists in entity: {:?}",
                spec.name
            )));
        }
        let current_active = self
            .state
            .charts
            .values()
            .find(|c| c.entity_id == source.entity_id && c.is_active)
            .map(|c| c.chart_id);
        let is_active = spec.activate || current_active.is_none();
        let deactivated_chart_id = if spec.activate { current_active } else { None };
        let new_chart_id = Uuid::new_v4();
        let new_chart = Chart {
            chart_id: new_chart_id,
            entity_id: source.entity_id,
            name: spec.name,
            description: spec.description,
            is_active,
            created_at: self.clock.now_ms(),
        };

        // Copy accounts in dependency order (parents before children) so
        // `parent_account_id` can be remapped to the new chart's own ids.
        let mut id_map: BTreeMap<Uuid, Uuid> = BTreeMap::new();
        let mut remaining: Vec<&Account> = self
            .state
            .accounts
            .values()
            .filter(|a| a.chart_id == spec.source_chart_id)
            .collect();
        let mut new_accounts = Vec::new();
        while !remaining.is_empty() {
            let before = remaining.len();
            remaining.retain(|a| {
                let ready = match a.parent_account_id {
                    None => true,
                    Some(parent_id) => id_map.contains_key(&parent_id),
                };
                if !ready {
                    return true;
                }
                let new_account_id = Uuid::new_v4();
                id_map.insert(a.account_id, new_account_id);
                new_accounts.push(Account {
                    account_id: new_account_id,
                    chart_id: new_chart_id,
                    entity_id: source.entity_id,
                    name: a.name.clone(),
                    code: a.code.clone(),
                    account_type: a.account_type,
                    normal_balance: a.normal_balance,
                    resource_type_id: a.resource_type_id,
                    parent_account_id: a.parent_account_id.map(|parent_id| id_map[&parent_id]),
                    is_active: a.is_active,
                    validation_rules: a.validation_rules.clone(),
                    created_at: self.clock.now_ms(),
                    metadata: a.metadata.clone(),
                });
                false
            });
            if remaining.len() == before {
                // Unreachable in practice: parent_account_id is validated to
                // stay within one chart at account-creation time, so a cycle
                // can never form. Guarded here so a future relaxation of that
                // rule fails loudly instead of looping forever.
                return Err(EngineError::invalid_input(
                    "source chart has a cyclic parent_account_id chain",
                ));
            }
        }

        self.record(
            op_id,
            actor,
            request,
            EventPayload::ChartCreated {
                chart: new_chart,
                deactivated_chart_id,
            },
        );
        for account in new_accounts {
            let sub_request = json!({
                "op": "copy_chart_account", "chart_id": new_chart_id, "account_id": account.account_id
            });
            self.record(
                Uuid::new_v4(),
                actor,
                sub_request,
                EventPayload::AccountCreated { account },
            );
        }
        Ok(new_chart_id)
    }

    pub fn create_account(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        spec: NewAccount,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "create_account", "spec": spec });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        let chart = self
            .state
            .charts
            .get(&spec.chart_id)
            .ok_or_else(|| EngineError::invalid_input("unknown chart"))?;
        if spec.name.trim().is_empty() {
            return Err(EngineError::invalid_input("account name must not be empty"));
        }
        if self
            .state
            .accounts
            .values()
            .any(|a| a.chart_id == spec.chart_id && a.name == spec.name)
        {
            return Err(EngineError::invalid_input(format!(
                "account name already exists in chart: {:?}",
                spec.name
            )));
        }
        if !self
            .state
            .resource_types
            .contains_key(&spec.resource_type_id)
        {
            return Err(EngineError::invalid_input("unknown resource type"));
        }
        if let Some(parent_id) = spec.parent_account_id {
            match self.state.accounts.get(&parent_id) {
                Some(parent) if parent.chart_id == spec.chart_id => {}
                Some(_) => {
                    return Err(EngineError::invalid_input(
                        "parent account belongs to a different chart",
                    ))
                }
                None => return Err(EngineError::invalid_input("unknown parent account")),
            }
        }
        parse_validation_rules(&spec.validation_rules)?;
        let account = Account {
            account_id: Uuid::new_v4(),
            chart_id: spec.chart_id,
            entity_id: chart.entity_id,
            name: spec.name,
            code: spec.code,
            account_type: spec.account_type,
            normal_balance: spec.account_type.normal_balance(),
            resource_type_id: spec.resource_type_id,
            parent_account_id: spec.parent_account_id,
            is_active: true,
            validation_rules: spec.validation_rules,
            created_at: self.clock.now_ms(),
            metadata: spec.metadata,
        };
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::AccountCreated { account },
        ))
    }

    pub fn update_account_metadata(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        account_id: Uuid,
        update: UpdateAccountMetadata,
    ) -> Result<Uuid, EngineError> {
        let request =
            json!({ "op": "update_account_metadata", "account_id": account_id, "update": update });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        let account = self.state.accounts.get(&account_id).ok_or_else(|| {
            EngineError::with_details(
                ErrorCode::UnknownAccount,
                "account not found",
                json!({ "account_id": account_id }),
            )
        })?;
        if let Some(new_name) = &update.name {
            if new_name.trim().is_empty() {
                return Err(EngineError::invalid_input("account name must not be empty"));
            }
            if self.state.accounts.values().any(|a| {
                a.chart_id == account.chart_id && a.account_id != account_id && a.name == *new_name
            }) {
                return Err(EngineError::invalid_input(format!(
                    "account name already exists in chart: {new_name:?}"
                )));
            }
        }
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::AccountMetadataUpdated {
                account_id,
                name: update.name,
                code: update.code,
                metadata: update.metadata,
            },
        ))
    }

    /// Deactivation/reactivation is a ledger event (Impl Spec §2.3).
    pub fn set_account_active(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        account_id: Uuid,
        is_active: bool,
    ) -> Result<Uuid, EngineError> {
        let request = json!({
            "op": "set_account_active", "account_id": account_id, "is_active": is_active
        });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        let account = self.state.accounts.get(&account_id).ok_or_else(|| {
            EngineError::with_details(
                ErrorCode::UnknownAccount,
                "account not found",
                json!({ "account_id": account_id }),
            )
        })?;
        if account.is_active == is_active {
            return Err(EngineError::invalid_input(format!(
                "account is already {}",
                if is_active { "active" } else { "inactive" }
            )));
        }
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::AccountStatusChanged {
                account_id,
                is_active,
            },
        ))
    }

    pub fn create_period(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        spec: NewPeriod,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "create_period", "spec": spec });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        if !self.state.entities.contains_key(&spec.entity_id) {
            return Err(EngineError::invalid_input("unknown entity"));
        }
        if spec.end_date < spec.start_date {
            return Err(EngineError::invalid_input(
                "period end_date must be >= start_date",
            ));
        }
        let overlaps = self.state.periods.values().any(|p| {
            p.entity_id == spec.entity_id
                && spec.start_date <= p.end_date
                && p.start_date <= spec.end_date
        });
        if overlaps {
            return Err(EngineError::invalid_input(
                "periods must not overlap within an entity",
            ));
        }
        let period = Period {
            period_id: Uuid::new_v4(),
            entity_id: spec.entity_id,
            name: spec.name,
            start_date: spec.start_date,
            end_date: spec.end_date,
            status: PeriodStatus::Open,
        };
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::PeriodCreated { period },
        ))
    }

    pub fn close_period(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        period_id: Uuid,
    ) -> Result<Uuid, EngineError> {
        self.set_period_status(op_id, actor, period_id, PeriodStatus::Closed)
    }

    pub fn reopen_period(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        period_id: Uuid,
    ) -> Result<Uuid, EngineError> {
        self.set_period_status(op_id, actor, period_id, PeriodStatus::Open)
    }

    fn set_period_status(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        period_id: Uuid,
        status: PeriodStatus,
    ) -> Result<Uuid, EngineError> {
        let request = json!({
            "op": "set_period_status", "period_id": period_id, "status": status
        });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        let period = self
            .state
            .periods
            .get(&period_id)
            .ok_or_else(|| EngineError::invalid_input("unknown period"))?;
        if period.status == status {
            return Err(EngineError::invalid_input(format!(
                "period is already {status:?}"
            )));
        }
        Ok(self.record(
            op_id,
            actor,
            request,
            EventPayload::PeriodStatusChanged { period_id, status },
        ))
    }

    /// Standalone price change — a PRICE ledger event (Impl Spec §2.6).
    pub fn record_price(
        &mut self,
        op_id: Uuid,
        actor: Uuid,
        price: PriceFact,
    ) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "record_price", "price": price });
        if let Some(done) = self.check_idempotency(op_id, &request)? {
            return Ok(done);
        }
        self.validate_price_fact(&price)?;
        Ok(self.record(op_id, actor, request, EventPayload::PriceRecorded { price }))
    }

    // -- posting --------------------------------------------------------------

    /// Post-or-reject (Impl Spec §2.4, §4.1). On success the entry is an
    /// immutable ledger event; on failure the ledger is untouched.
    pub fn post_entry(&mut self, actor: Uuid, new: NewEntry) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "post_entry", "entry": new });
        if let Some(done) = self.check_idempotency(new.entry_id, &request)? {
            return Ok(done);
        }
        let entry = self.validate_entry(actor, &new)?;
        Ok(self.record(
            new.entry_id,
            actor,
            request,
            EventPayload::EntryPosted { entry },
        ))
    }

    /// Correction is a reversal entry referencing `reversal_of`
    /// (Impl Spec §2.4). The reversal is an ordinary entry: all invariants
    /// (open period, active accounts, balance) are re-checked. Whether an
    /// entry may be reversed twice is workflow policy, not an engine rule.
    pub fn reverse_entry(&mut self, actor: Uuid, spec: ReverseEntry) -> Result<Uuid, EngineError> {
        let request = json!({ "op": "reverse_entry", "spec": spec });
        if let Some(done) = self.check_idempotency(spec.new_entry_id, &request)? {
            return Ok(done);
        }
        let original = self
            .state
            .entries
            .get(&spec.original_entry_id)
            .ok_or_else(|| EngineError::invalid_input("unknown entry to reverse"))?;
        if original.event_type != EventType::Accounting {
            return Err(EngineError::invalid_input(
                "only ACCOUNTING entries can be reversed",
            ));
        }
        let description = spec
            .description
            .clone()
            .unwrap_or_else(|| format!("Reversal of: {}", original.description));
        let lines = original
            .lines
            .iter()
            .map(|line| NewLine {
                line_id: Uuid::new_v4(),
                account_id: line.account_id,
                debit_amount: line.credit_amount, // sides swapped
                credit_amount: line.debit_amount,
                memo: line.memo.clone(),
                metadata: line.metadata.clone(),
            })
            .collect();
        let new = NewEntry {
            entry_id: spec.new_entry_id,
            entity_id: original.entity_id,
            entry_date: spec.entry_date,
            description,
            lines,
            prices: original.prices.clone(),
            source: EntrySource::Manual,
            metadata: spec.metadata,
        };
        let mut entry = self.validate_entry(actor, &new)?;
        entry.reversal_of = Some(spec.original_entry_id);
        Ok(self.record(
            spec.new_entry_id,
            actor,
            request,
            EventPayload::EntryPosted { entry },
        ))
    }

    // -- validation ------------------------------------------------------------

    fn validate_price_fact(&self, price: &PriceFact) -> Result<(), EngineError> {
        if !self
            .state
            .resource_types
            .contains_key(&price.base_resource_type_id)
            || !self
                .state
                .resource_types
                .contains_key(&price.quote_resource_type_id)
        {
            return Err(EngineError::invalid_input(
                "price references unknown resource type",
            ));
        }
        if price.base_resource_type_id == price.quote_resource_type_id {
            return Err(EngineError::invalid_input(
                "price base and quote must differ",
            ));
        }
        if price.rate.raw() <= 0 {
            return Err(EngineError::invalid_input("price rate must be > 0"));
        }
        Ok(())
    }

    /// All §4.1 invariants that exist in M2. (Authorization and execution-
    /// context checks — §4.1.5 — arrive with workflow machinery in M5;
    /// idempotency — §4.1.6 — is checked by the caller before this runs.)
    fn validate_entry(&self, actor: Uuid, new: &NewEntry) -> Result<JournalEntry, EngineError> {
        // Structural (INVALID_INPUT).
        if new.description.trim().is_empty() {
            return Err(EngineError::invalid_input("description is required"));
        }
        if new.lines.is_empty() {
            return Err(EngineError::invalid_input(
                "entry must have at least one line",
            ));
        }
        let mut seen_line_ids = std::collections::BTreeSet::new();
        for line in &new.lines {
            if !seen_line_ids.insert(line.line_id) {
                return Err(EngineError::invalid_input("duplicate line_id within entry"));
            }
            match (line.debit_amount, line.credit_amount) {
                (Some(d), None) if !d.is_negative() => {}
                (None, Some(c)) if !c.is_negative() => {}
                (Some(_), Some(_)) | (None, None) => {
                    return Err(EngineError::invalid_input(
                        "each line must have exactly one of debit_amount/credit_amount",
                    ))
                }
                _ => {
                    return Err(EngineError::invalid_input("line amounts must be >= 0"));
                }
            }
        }
        if !self.state.entities.contains_key(&new.entity_id) {
            return Err(EngineError::invalid_input("unknown entity"));
        }
        // Entry-recorded prices: well-formed, no duplicate or two-way pairs.
        let mut price_dirs = std::collections::BTreeSet::new();
        for price in &new.prices {
            self.validate_price_fact(price)?;
            let key = (price.base_resource_type_id, price.quote_resource_type_id);
            if !price_dirs.insert(key) {
                return Err(EngineError::invalid_input("duplicate price for a pair"));
            }
            if price_dirs.contains(&(key.1, key.0)) {
                return Err(EngineError::invalid_input(
                    "a resource-type pair must be priced in one direction only",
                ));
            }
        }

        // Accounts: exist, active, one chart + one entity (§4.1.2).
        let mut chart_id: Option<Uuid> = None;
        for line in &new.lines {
            let account = self.state.accounts.get(&line.account_id).ok_or_else(|| {
                EngineError::with_details(
                    ErrorCode::UnknownAccount,
                    "account not found",
                    json!({ "account_id": line.account_id }),
                )
            })?;
            if !account.is_active {
                return Err(EngineError::with_details(
                    ErrorCode::InactiveAccount,
                    "account is deactivated",
                    json!({ "account_id": line.account_id }),
                ));
            }
            match chart_id {
                None => chart_id = Some(account.chart_id),
                Some(c) if c == account.chart_id => {}
                Some(_) => {
                    return Err(EngineError::new(
                        ErrorCode::ChartMismatch,
                        "lines span multiple charts",
                    ))
                }
            }
            if account.entity_id != new.entity_id {
                return Err(EngineError::new(
                    ErrorCode::ChartMismatch,
                    "line account belongs to a different entity than the entry",
                ));
            }
        }

        // Balance and unit coverage (§4.1.1, §4.1.3, §2.6).
        self.check_balance(new)?;

        // Period (§4.1.4).
        let period = self
            .state
            .periods
            .values()
            .find(|p| {
                p.entity_id == new.entity_id
                    && p.start_date <= new.entry_date
                    && new.entry_date <= p.end_date
            })
            .ok_or_else(|| {
                EngineError::with_details(
                    ErrorCode::NoOpenPeriod,
                    "entry_date falls in no period of the entity",
                    json!({ "entry_date": new.entry_date.as_str() }),
                )
            })?;
        if period.status == PeriodStatus::Closed {
            return Err(EngineError::with_details(
                ErrorCode::PeriodClosed,
                "entry_date falls in a closed period",
                json!({ "period_id": period.period_id, "entry_date": new.entry_date.as_str() }),
            ));
        }

        // Account-defined validation rules (§4.1.7).
        for line in &new.lines {
            let account = &self.state.accounts[&line.account_id];
            let rules = parse_validation_rules(&account.validation_rules)
                .expect("rules were validated at account creation");
            let amount = line
                .debit_amount
                .or(line.credit_amount)
                .unwrap_or(Amount::ZERO);
            if rules.require_memo && line.memo.as_deref().is_none_or(|m| m.trim().is_empty()) {
                return Err(rule_violation(line, "require_memo"));
            }
            if let Some(max) = rules.max_amount {
                if amount > max {
                    return Err(rule_violation(line, "max_amount"));
                }
            }
            match rules.side {
                Some(RuleSide::DebitOnly) if line.debit_amount.is_none() => {
                    return Err(rule_violation(line, "side"));
                }
                Some(RuleSide::CreditOnly) if line.credit_amount.is_none() => {
                    return Err(rule_violation(line, "side"));
                }
                _ => {}
            }
        }

        // Running-balance bound pre-check, so `apply` cannot fail.
        let mut deltas: BTreeMap<Uuid, (Amount, Amount)> = BTreeMap::new();
        for line in &new.lines {
            let delta = deltas.entry(line.account_id).or_default();
            if let Some(d) = line.debit_amount {
                delta.0 = delta.0.checked_add(d).map_err(EngineError::invalid_input)?;
            }
            if let Some(c) = line.credit_amount {
                delta.1 = delta.1.checked_add(c).map_err(EngineError::invalid_input)?;
            }
        }
        for (account_id, (debit, credit)) in &deltas {
            let balance = self
                .state
                .balances
                .get(account_id)
                .cloned()
                .unwrap_or_default();
            balance
                .debit_total
                .checked_add(*debit)
                .and_then(|_| balance.credit_total.checked_add(*credit))
                .map_err(|_| {
                    EngineError::invalid_input("account running totals would exceed Decimal(18,8)")
                })?;
        }

        Ok(JournalEntry {
            entry_id: new.entry_id,
            book_id: self.state.book_id,
            entity_id: new.entity_id,
            entry_date: new.entry_date.clone(),
            posted_at: self.clock.now_ms(),
            posted_by: actor,
            workflow: None,
            event_type: EventType::Accounting,
            description: new.description.clone(),
            reversal_of: None,
            period_id: period.period_id,
            prices: new.prices.clone(),
            source: new.source,
            metadata: new.metadata.clone(),
            lines: new
                .lines
                .iter()
                .map(|l| JournalLine {
                    line_id: l.line_id,
                    account_id: l.account_id,
                    debit_amount: l.debit_amount,
                    credit_amount: l.credit_amount,
                    memo: l.memo.clone(),
                    metadata: l.metadata.clone(),
                })
                .collect(),
        })
    }

    /// Exact balance at the entry's own recorded prices (Impl Spec §2.6):
    /// debits == credits exactly, no tolerance, no implicit rounding.
    fn check_balance(&self, new: &NewEntry) -> Result<(), EngineError> {
        // Net (debit − credit) per resource type.
        let mut nets: BTreeMap<Uuid, i128> = BTreeMap::new();
        for line in &new.lines {
            let account = &self.state.accounts[&line.account_id];
            let signed = line.debit_amount.map(|a| a.raw()).unwrap_or(0)
                - line.credit_amount.map(|a| a.raw()).unwrap_or(0);
            *nets.entry(account.resource_type_id).or_insert(0) += signed;
        }
        if nets.len() == 1 {
            let (_, net) = nets.iter().next().unwrap();
            if *net != 0 {
                return Err(unbalanced(*net));
            }
            return Ok(());
        }
        // Cross-unit: convert every unit into the first line's unit using
        // exactly the entry's recorded prices, as exact rationals.
        let valuation = self.state.accounts[&new.lines[0].account_id].resource_type_id;
        let mut total = Rational::ZERO;
        for (unit, net) in &nets {
            let (num, den) = if *unit == valuation {
                (1, 1)
            } else {
                let direct = new.prices.iter().find(|p| {
                    p.base_resource_type_id == *unit && p.quote_resource_type_id == valuation
                });
                let inverse = new.prices.iter().find(|p| {
                    p.base_resource_type_id == valuation && p.quote_resource_type_id == *unit
                });
                match (direct, inverse) {
                    (Some(p), _) => (p.rate.raw(), SCALE),
                    (None, Some(p)) => (SCALE, p.rate.raw()),
                    (None, None) => {
                        return Err(EngineError::with_details(
                            ErrorCode::MissingPrice,
                            "entry spans resource types without a recorded price",
                            json!({
                                "base_resource_type_id": unit,
                                "quote_resource_type_id": valuation,
                            }),
                        ))
                    }
                }
            };
            total = total
                .add_scaled(*net, num, den)
                .map_err(EngineError::invalid_input)?;
        }
        if !total.is_zero() {
            return Err(EngineError::new(
                ErrorCode::UnbalancedEntry,
                "debits != credits at the entry's recorded prices",
            ));
        }
        Ok(())
    }

    // -- reads ------------------------------------------------------------------

    pub fn list_entities(&self) -> Vec<&Entity> {
        self.state.entities.values().collect()
    }

    pub fn list_resource_types(&self) -> Vec<&ResourceType> {
        self.state.resource_types.values().collect()
    }

    pub fn list_charts(&self, entity_id: Uuid) -> Vec<&Chart> {
        self.state
            .charts
            .values()
            .filter(|c| c.entity_id == entity_id)
            .collect()
    }

    pub fn list_accounts(&self, chart_id: Uuid) -> Vec<&Account> {
        self.state
            .accounts
            .values()
            .filter(|a| a.chart_id == chart_id)
            .collect()
    }

    pub fn list_periods(&self, entity_id: Uuid) -> Vec<&Period> {
        self.state
            .periods
            .values()
            .filter(|p| p.entity_id == entity_id)
            .collect()
    }

    /// Latest projected rate per (base, quote) pair (Impl Spec §2.6).
    pub fn list_prices(&self) -> Vec<&PriceFact> {
        self.state
            .prices
            .values()
            .map(|entry| &entry.price)
            .collect()
    }

    pub fn get_entry(&self, entry_id: Uuid) -> Option<&JournalEntry> {
        self.state.entries.get(&entry_id)
    }

    /// Entries of an entity in ledger (log) order.
    pub fn list_entries(&self, entity_id: Uuid) -> Vec<&JournalEntry> {
        self.state
            .log
            .iter()
            .filter_map(|record| match &record.payload {
                EventPayload::EntryPosted { entry } if entry.entity_id == entity_id => Some(
                    self.state
                        .entries
                        .get(&entry.entry_id)
                        .expect("posted entry is in the entries projection"),
                ),
                _ => None,
            })
            .collect()
    }

    pub fn get_account(&self, account_id: Uuid) -> Option<&Account> {
        self.state.accounts.get(&account_id)
    }

    pub fn get_period(&self, period_id: Uuid) -> Option<&Period> {
        self.state.periods.get(&period_id)
    }

    /// Default/suggested rate from the price projection (never used to
    /// re-check posted entries). Returns the fact and whether it was found
    /// in the inverse direction.
    pub fn lookup_price(&self, base: Uuid, quote: Uuid) -> Option<(&PriceFact, bool)> {
        if let Some(entry) = self.state.prices.get(&(base, quote)) {
            return Some((&entry.price, false));
        }
        self.state
            .prices
            .get(&(quote, base))
            .map(|entry| (&entry.price, true))
    }

    /// Authoritative balance — always equal to summing journal lines from
    /// the event log (verified by the replay tests).
    pub fn get_balance(&self, account_id: Uuid) -> Result<BalanceView, EngineError> {
        let account = self.state.accounts.get(&account_id).ok_or_else(|| {
            EngineError::with_details(
                ErrorCode::UnknownAccount,
                "account not found",
                json!({ "account_id": account_id }),
            )
        })?;
        let balance = self
            .state
            .balances
            .get(&account_id)
            .cloned()
            .unwrap_or_default();
        let net = balance
            .debit_total
            .checked_sub(balance.credit_total)
            .map_err(EngineError::invalid_input)?;
        let natural = match account.normal_balance {
            NormalBalance::Debit => net,
            NormalBalance::Credit => net.checked_neg().map_err(EngineError::invalid_input)?,
        };
        Ok(BalanceView {
            account_id,
            resource_type_id: account.resource_type_id,
            debit_total: balance.debit_total,
            credit_total: balance.credit_total,
            net,
            natural,
        })
    }

    pub fn trial_balance(&self, chart_id: Uuid) -> Result<Vec<TrialBalanceRow>, EngineError> {
        if !self.state.charts.contains_key(&chart_id) {
            return Err(EngineError::invalid_input("unknown chart"));
        }
        let mut rows = Vec::new();
        for account in self.state.accounts.values() {
            if account.chart_id != chart_id {
                continue;
            }
            let view = self.get_balance(account.account_id)?;
            rows.push(TrialBalanceRow {
                account_id: account.account_id,
                name: account.name.clone(),
                account_type: account.account_type,
                resource_type_id: account.resource_type_id,
                debit_total: view.debit_total,
                credit_total: view.credit_total,
                natural: view.natural,
            });
        }
        Ok(rows)
    }

    /// Expanded-equation check A = L + E + (R − X) per resource type (see
    /// `UnitEquation`). Also re-verifies that no operation sequence has
    /// unbalanced the chart.
    pub fn equation_check(&self, chart_id: Uuid) -> Result<EquationReport, EngineError> {
        if !self.state.charts.contains_key(&chart_id) {
            return Err(EngineError::invalid_input("unknown chart"));
        }
        // Class nets (debit − credit, raw) per resource type.
        #[derive(Default, Clone)]
        struct Sums {
            asset: i128,
            liability: i128,
            equity: i128,
            revenue: i128,
            expense: i128,
        }
        let mut per_unit: BTreeMap<Uuid, Sums> = BTreeMap::new();
        for account in self.state.accounts.values() {
            if account.chart_id != chart_id {
                continue;
            }
            let balance = self
                .state
                .balances
                .get(&account.account_id)
                .cloned()
                .unwrap_or_default();
            let net = balance.debit_total.raw() - balance.credit_total.raw();
            let sums = per_unit.entry(account.resource_type_id).or_default();
            match account.account_type {
                AccountType::Asset => sums.asset += net,
                AccountType::Liability => sums.liability += net,
                AccountType::Equity => sums.equity += net,
                AccountType::Revenue => sums.revenue += net,
                AccountType::Expense => sums.expense += net,
            }
        }
        // Expected per-unit residue: the value cross-unit entries moved
        // between units (each such entry balances at its recorded prices).
        let mut cross_unit: BTreeMap<Uuid, i128> = BTreeMap::new();
        for entry in self.state.entries.values() {
            let entry_chart = entry
                .lines
                .first()
                .and_then(|l| self.state.accounts.get(&l.account_id))
                .map(|a| a.chart_id);
            if entry_chart != Some(chart_id) {
                continue;
            }
            let mut nets: BTreeMap<Uuid, i128> = BTreeMap::new();
            for line in &entry.lines {
                let account = &self.state.accounts[&line.account_id];
                let signed = line.debit_amount.map(|a| a.raw()).unwrap_or(0)
                    - line.credit_amount.map(|a| a.raw()).unwrap_or(0);
                *nets.entry(account.resource_type_id).or_insert(0) += signed;
            }
            if nets.len() > 1 {
                for (unit, net) in nets {
                    *cross_unit.entry(unit).or_insert(0) += net;
                }
            }
        }
        let natural = |account_type_net: i128, debit_normal: bool| -> i128 {
            if debit_normal {
                account_type_net
            } else {
                -account_type_net
            }
        };
        let mut units = Vec::new();
        let mut all_hold = true;
        for (unit, sums) in &per_unit {
            let expected = cross_unit.get(unit).copied().unwrap_or(0);
            let total_net = sums.asset + sums.liability + sums.equity + sums.revenue + sums.expense;
            let holds = total_net == expected;
            all_hold &= holds;
            let to_amount = |raw: i128| Amount::from_raw(raw).map_err(EngineError::invalid_input);
            units.push(UnitEquation {
                resource_type_id: *unit,
                assets: to_amount(natural(sums.asset, true))?,
                liabilities: to_amount(natural(sums.liability, false))?,
                equity: to_amount(natural(sums.equity, false))?,
                revenue: to_amount(natural(sums.revenue, false))?,
                expenses: to_amount(natural(sums.expense, true))?,
                cross_unit_net: to_amount(expected)?,
                holds,
            });
        }
        Ok(EquationReport {
            chart_id,
            units,
            holds: all_hold,
        })
    }

    /// Integrity sweep: re-verify every posted entry still balances exactly
    /// at its own recorded prices. Returns the number of entries checked.
    pub fn verify_all_entries(&self) -> Result<usize, EngineError> {
        for entry in self.state.entries.values() {
            let as_new = NewEntry {
                entry_id: entry.entry_id,
                entity_id: entry.entity_id,
                entry_date: entry.entry_date.clone(),
                description: entry.description.clone(),
                lines: entry
                    .lines
                    .iter()
                    .map(|l| NewLine {
                        line_id: l.line_id,
                        account_id: l.account_id,
                        debit_amount: l.debit_amount,
                        credit_amount: l.credit_amount,
                        memo: l.memo.clone(),
                        metadata: l.metadata.clone(),
                    })
                    .collect(),
                prices: entry.prices.clone(),
                source: entry.source,
                metadata: entry.metadata.clone(),
            };
            self.check_balance(&as_new)?;
        }
        Ok(self.state.entries.len())
    }
}

// ---------------------------------------------------------------------------
// Account validation rules (v1 vocabulary, Impl Spec §4.1.7)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum RuleSide {
    DebitOnly,
    CreditOnly,
}

#[derive(Debug, Clone, Default)]
struct ValidationRules {
    require_memo: bool,
    max_amount: Option<Amount>,
    side: Option<RuleSide>,
}

fn parse_validation_rules(value: &Value) -> Result<ValidationRules, EngineError> {
    let mut rules = ValidationRules::default();
    let map = match value {
        Value::Null => return Ok(rules),
        Value::Object(map) => map,
        _ => {
            return Err(EngineError::invalid_input(
                "validation_rules must be an object or null",
            ))
        }
    };
    for (key, val) in map {
        match key.as_str() {
            "require_memo" => {
                rules.require_memo = val
                    .as_bool()
                    .ok_or_else(|| EngineError::invalid_input("require_memo must be a boolean"))?;
            }
            "max_amount" => {
                let s = val.as_str().ok_or_else(|| {
                    EngineError::invalid_input("max_amount must be a decimal string")
                })?;
                let amount: Amount = s
                    .parse()
                    .map_err(|e: String| EngineError::invalid_input(e))?;
                if amount.is_negative() {
                    return Err(EngineError::invalid_input("max_amount must be >= 0"));
                }
                rules.max_amount = Some(amount);
            }
            "side" => {
                rules.side = Some(match val.as_str() {
                    Some("debit_only") => RuleSide::DebitOnly,
                    Some("credit_only") => RuleSide::CreditOnly,
                    _ => {
                        return Err(EngineError::invalid_input(
                            "side must be \"debit_only\" or \"credit_only\"",
                        ))
                    }
                });
            }
            other => {
                return Err(EngineError::invalid_input(format!(
                    "unknown validation rule: {other:?}"
                )))
            }
        }
    }
    Ok(rules)
}

fn rule_violation(line: &NewLine, rule: &str) -> EngineError {
    EngineError::with_details(
        ErrorCode::ValidationFailed,
        format!("account validation rule {rule:?} failed"),
        json!({ "account_id": line.account_id, "line_id": line.line_id, "rule": rule }),
    )
}

fn unbalanced(net_raw: i128) -> EngineError {
    EngineError::with_details(
        ErrorCode::UnbalancedEntry,
        "debits != credits",
        json!({ "net_raw_1e8": net_raw.to_string() }),
    )
}
