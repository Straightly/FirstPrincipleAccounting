//! Shared test fixture: one book, one entity, USD/EUR/Widget resource
//! types, a chart with accounts of all five types, and periods Jan–Jun 2026
//! (May closed). Uses a fixed clock so runs are deterministic.
#![allow(dead_code)]

use ledgerzero_engine::amount::Amount;
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use ledgerzero_engine::types::{Date, FixedClock};
use ledgerzero_engine::AccountingEngine;
use serde_json::Value;
use uuid::Uuid;

pub fn amt(s: &str) -> Amount {
    s.parse().unwrap()
}

pub fn date(s: &str) -> Date {
    s.parse().unwrap()
}

pub fn id() -> Uuid {
    Uuid::new_v4()
}

pub struct Fx {
    pub engine: AccountingEngine,
    pub book: Uuid,
    pub actor: Uuid,
    pub entity: Uuid,
    pub usd: Uuid,
    pub eur: Uuid,
    pub widget: Uuid,
    pub chart: Uuid,
    /// Asset, USD.
    pub cash: Uuid,
    /// Asset, EUR.
    pub bank_eur: Uuid,
    /// Asset, Widget ("each").
    pub inventory: Uuid,
    /// Liability, USD.
    pub loan: Uuid,
    /// Equity, USD.
    pub capital: Uuid,
    /// Revenue, USD.
    pub sales: Uuid,
    /// Expense, USD.
    pub rent: Uuid,
    /// Expense, USD, with validation rules:
    /// require_memo, max_amount 100, debit_only.
    pub strict: Uuid,
    /// Asset, USD — reserved for activate/deactivate tests.
    pub spare: Uuid,
    /// Jan..Jun 2026 period ids; May is CLOSED.
    pub periods: Vec<Uuid>,
    pub may: Uuid,
}

pub fn fixture() -> Fx {
    let book = Uuid::new_v4();
    let actor = Uuid::new_v4();
    let mut engine = AccountingEngine::new(book, Box::new(FixedClock::new(1_752_000_000_000)));

    let entity = engine.create_entity(id(), actor, "Acme LLC").unwrap();

    let mut resource = |name: &str, kind: ResourceKind, code: &str, unit: &str, precision: u8| {
        engine
            .create_resource_type(
                id(),
                actor,
                NewResourceType {
                    name: name.into(),
                    kind,
                    code: code.into(),
                    unit_of_measure: unit.into(),
                    precision,
                    metadata: Value::Null,
                },
            )
            .unwrap()
    };
    let usd = resource("US Dollar", ResourceKind::Currency, "USD", "USD", 2);
    let eur = resource("Euro", ResourceKind::Currency, "EUR", "EUR", 2);
    let widget = resource("Widget-A", ResourceKind::Inventory, "WIDGET-A", "each", 0);

    let chart = engine
        .create_chart(
            id(),
            actor,
            NewChart {
                entity_id: entity,
                name: "Main".into(),
                description: None,
                activate: true,
            },
        )
        .unwrap();

    let mut account =
        |name: &str, account_type: AccountType, resource_type_id: Uuid, rules: Value| {
            engine
                .create_account(
                    id(),
                    actor,
                    NewAccount {
                        chart_id: chart,
                        name: name.into(),
                        code: None,
                        account_type,
                        resource_type_id,
                        parent_account_id: None,
                        validation_rules: rules,
                        metadata: Value::Null,
                    },
                )
                .unwrap()
        };
    let cash = account("Cash", AccountType::Asset, usd, Value::Null);
    let bank_eur = account("Bank EUR", AccountType::Asset, eur, Value::Null);
    let inventory = account("Inventory", AccountType::Asset, widget, Value::Null);
    let loan = account("Bank Loan", AccountType::Liability, usd, Value::Null);
    let capital = account("Owner Capital", AccountType::Equity, usd, Value::Null);
    let sales = account("Sales", AccountType::Revenue, usd, Value::Null);
    let rent = account("Rent Expense", AccountType::Expense, usd, Value::Null);
    let strict = account(
        "Strict Expense",
        AccountType::Expense,
        usd,
        serde_json::json!({
            "require_memo": true,
            "max_amount": "100.00",
            "side": "debit_only",
        }),
    );
    let spare = account("Spare Asset", AccountType::Asset, usd, Value::Null);

    let mut periods = Vec::new();
    let months = [
        ("2026-01", "2026-01-01", "2026-01-31"),
        ("2026-02", "2026-02-01", "2026-02-28"),
        ("2026-03", "2026-03-01", "2026-03-31"),
        ("2026-04", "2026-04-01", "2026-04-30"),
        ("2026-05", "2026-05-01", "2026-05-31"),
        ("2026-06", "2026-06-01", "2026-06-30"),
    ];
    for (name, start, end) in months {
        periods.push(
            engine
                .create_period(
                    id(),
                    actor,
                    NewPeriod {
                        entity_id: entity,
                        name: name.into(),
                        start_date: date(start),
                        end_date: date(end),
                    },
                )
                .unwrap(),
        );
    }
    let may = periods[4];
    engine.close_period(id(), actor, may).unwrap();

    Fx {
        engine,
        book,
        actor,
        entity,
        usd,
        eur,
        widget,
        chart,
        cash,
        bank_eur,
        inventory,
        loan,
        capital,
        sales,
        rent,
        strict,
        spare,
        periods,
        may,
    }
}

pub fn debit(account_id: Uuid, amount: &str) -> NewLine {
    NewLine {
        line_id: id(),
        account_id,
        debit_amount: Some(amt(amount)),
        credit_amount: None,
        memo: None,
        metadata: Value::Null,
    }
}

pub fn credit(account_id: Uuid, amount: &str) -> NewLine {
    NewLine {
        line_id: id(),
        account_id,
        debit_amount: None,
        credit_amount: Some(amt(amount)),
        memo: None,
        metadata: Value::Null,
    }
}

impl Fx {
    pub fn entry(&self, entry_date: &str, description: &str, lines: Vec<NewLine>) -> NewEntry {
        NewEntry {
            entry_id: id(),
            entity_id: self.entity,
            entry_date: date(entry_date),
            description: description.into(),
            lines,
            prices: Vec::new(),
            source: EntrySource::Manual,
            metadata: Value::Null,
        }
    }

    pub fn price(&self, base: Uuid, quote: Uuid, rate: &str, as_of: &str) -> PriceFact {
        PriceFact {
            base_resource_type_id: base,
            quote_resource_type_id: quote,
            rate: amt(rate),
            as_of: date(as_of),
        }
    }
}
