//! M4 engine additions: copy_chart and the list_* read methods.

mod common;

use common::*;
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use ledgerzero_engine::ErrorCode;
use serde_json::Value;
use uuid::Uuid;

#[test]
fn list_methods_reflect_the_fixture() {
    let fx = fixture();
    assert_eq!(fx.engine.list_entities().len(), 1);
    assert_eq!(fx.engine.list_resource_types().len(), 3);
    assert_eq!(fx.engine.list_charts(fx.entity).len(), 1);
    // Cash, Bank EUR, Inventory, Loan, Capital, Sales, Rent, Strict, Spare.
    assert_eq!(fx.engine.list_accounts(fx.chart).len(), 9);
    assert_eq!(fx.engine.list_periods(fx.entity).len(), 6);
    assert!(fx.engine.list_prices().is_empty());
}

#[test]
fn list_prices_reflects_the_latest_projection() {
    let mut fx = fixture();
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.eur, fx.usd, "1.10", "2026-01-05"),
        )
        .unwrap();
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.eur, fx.usd, "1.12", "2026-02-10"),
        )
        .unwrap();
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.widget, fx.usd, "9.00", "2026-01-01"),
        )
        .unwrap();

    let prices = fx.engine.list_prices();
    assert_eq!(prices.len(), 2);
    let eur_usd = prices
        .iter()
        .find(|p| p.base_resource_type_id == fx.eur)
        .unwrap();
    assert_eq!(eur_usd.rate, amt("1.12"), "latest as_of wins");
}

#[test]
fn copy_chart_duplicates_accounts_with_remapped_parents() {
    let mut fx = fixture();
    // Give one account in the fixture chart a parent so the remap path runs.
    let parent = fx.cash;
    let child = fx
        .engine
        .create_account(
            id(),
            fx.actor,
            NewAccount {
                chart_id: fx.chart,
                name: "Petty Cash".into(),
                code: None,
                account_type: AccountType::Asset,
                resource_type_id: fx.usd,
                parent_account_id: Some(parent),
                validation_rules: Value::Null,
                metadata: Value::Null,
            },
        )
        .unwrap();

    let source_count = fx.engine.list_accounts(fx.chart).len();

    let new_chart = fx
        .engine
        .copy_chart(
            id(),
            fx.actor,
            CopyChart {
                source_chart_id: fx.chart,
                name: "Main (copy)".into(),
                description: Some("proposed chart".into()),
                activate: false,
            },
        )
        .unwrap();

    assert_ne!(new_chart, fx.chart);
    let copied = fx.engine.list_accounts(new_chart);
    assert_eq!(copied.len(), source_count);

    let copied_child = copied.iter().find(|a| a.name == "Petty Cash").unwrap();
    let copied_parent = copied.iter().find(|a| a.name == "Cash").unwrap();
    assert_eq!(
        copied_child.parent_account_id,
        Some(copied_parent.account_id),
        "parent_account_id must remap into the new chart, not point back at the source"
    );
    assert_ne!(copied_child.account_id, child, "copies get fresh ids");

    // Source chart is untouched.
    assert_eq!(fx.engine.list_accounts(fx.chart).len(), source_count);

    // The original chart stays active since `activate: false` and it was
    // already the entity's active chart.
    let source_chart = fx
        .engine
        .list_charts(fx.entity)
        .into_iter()
        .find(|c| c.chart_id == fx.chart)
        .unwrap();
    assert!(source_chart.is_active);

    // Replay equivalence: the multi-event batch (ChartCreated + N x
    // AccountCreated) must be indistinguishable from incremental state.
    let replayed = EngineState::replay(fx.book, fx.engine.audit_log()).unwrap();
    assert_eq!(&replayed, fx.engine.state());
}

#[test]
fn copy_chart_is_idempotent_and_conflicts_on_tamper() {
    let mut fx = fixture();
    let op_id = id();
    let spec = CopyChart {
        source_chart_id: fx.chart,
        name: "Main (copy)".into(),
        description: None,
        activate: false,
    };
    let first = fx.engine.copy_chart(op_id, fx.actor, spec.clone()).unwrap();
    let log_len = fx.engine.audit_log().len();

    let replay = fx.engine.copy_chart(op_id, fx.actor, spec).unwrap();
    assert_eq!(
        replay, first,
        "identical replay returns the original chart id"
    );
    assert_eq!(
        fx.engine.audit_log().len(),
        log_len,
        "replay appends nothing"
    );

    let tampered = CopyChart {
        source_chart_id: fx.chart,
        name: "Different name".into(),
        description: None,
        activate: false,
    };
    let err = fx.engine.copy_chart(op_id, fx.actor, tampered).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::IdempotencyConflict);
}

#[test]
fn copy_chart_rejects_unknown_source() {
    let mut fx = fixture();
    let err = fx
        .engine
        .copy_chart(
            id(),
            fx.actor,
            CopyChart {
                source_chart_id: Uuid::new_v4(),
                name: "x".into(),
                description: None,
                activate: false,
            },
        )
        .unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);
}
