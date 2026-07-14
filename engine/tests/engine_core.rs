//! M2 engine-core tests: lifecycle, every §4.4 error code the engine can
//! raise, reversals, idempotency, prices, and the expanded-equation check.

mod common;

use common::*;
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use ledgerzero_engine::{EngineState, ErrorCode};
use serde_json::Value;
use uuid::Uuid;

fn code(result: Result<Uuid, ledgerzero_engine::EngineError>) -> ErrorCode {
    result.expect_err("expected a rejection").error_code
}

#[test]
fn lifecycle_post_read_reverse() {
    let mut fx = fixture();
    // Owner funds the company: Dr Cash 1000 / Cr Owner Capital 1000.
    let funding = fx.entry(
        "2026-01-10",
        "Initial funding",
        vec![debit(fx.cash, "1000"), credit(fx.capital, "1000")],
    );
    let funding_id = fx.engine.post_entry(fx.actor, funding).unwrap();

    // A sale: Dr Cash 250 / Cr Sales 250.
    let sale = fx.entry(
        "2026-02-05",
        "Cash sale",
        vec![debit(fx.cash, "250"), credit(fx.sales, "250")],
    );
    fx.engine.post_entry(fx.actor, sale).unwrap();

    let cash = fx.engine.get_balance(fx.cash).unwrap();
    assert_eq!(cash.natural, amt("1250"));
    let capital = fx.engine.get_balance(fx.capital).unwrap();
    assert_eq!(capital.natural, amt("1000")); // credit-normal: positive
    let sales = fx.engine.get_balance(fx.sales).unwrap();
    assert_eq!(sales.natural, amt("250"));

    // The stored entry is complete and immutable.
    let stored = fx.engine.get_entry(funding_id).unwrap();
    assert_eq!(stored.event_type, EventType::Accounting);
    assert_eq!(stored.posted_by, fx.actor);
    assert_eq!(stored.lines.len(), 2);
    assert!(stored.reversal_of.is_none());

    // Reverse the funding; balances return to the pre-funding state.
    let reversal_id = fx
        .engine
        .reverse_entry(
            fx.actor,
            ReverseEntry {
                new_entry_id: id(),
                original_entry_id: funding_id,
                entry_date: date("2026-03-01"),
                description: None,
                metadata: Value::Null,
            },
        )
        .unwrap();
    let reversal = fx.engine.get_entry(reversal_id).unwrap();
    assert_eq!(reversal.reversal_of, Some(funding_id));
    assert_eq!(fx.engine.get_balance(fx.cash).unwrap().natural, amt("250"));
    assert_eq!(fx.engine.get_balance(fx.capital).unwrap().natural, amt("0"));

    // Ledger order is preserved.
    let listed = fx.engine.list_entries(fx.entity);
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].entry_id, funding_id);

    let report = fx.engine.equation_check(fx.chart).unwrap();
    assert!(report.holds, "expanded equation must hold: {report:?}");
}

#[test]
fn normal_balance_is_derived_never_stored() {
    let fx = fixture();
    let assert_nb = |account_id: Uuid, expected: NormalBalance| {
        assert_eq!(
            fx.engine.get_account(account_id).unwrap().normal_balance,
            expected
        );
    };
    assert_nb(fx.cash, NormalBalance::Debit);
    assert_nb(fx.rent, NormalBalance::Debit);
    assert_nb(fx.loan, NormalBalance::Credit);
    assert_nb(fx.capital, NormalBalance::Credit);
    assert_nb(fx.sales, NormalBalance::Credit);
}

#[test]
fn unbalanced_entry_rejected() {
    let mut fx = fixture();
    let e = fx.entry(
        "2026-01-10",
        "off by a cent",
        vec![debit(fx.cash, "100.00"), credit(fx.sales, "100.01")],
    );
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::UnbalancedEntry
    );
    // Rejected entries leave no trace (posted-or-rejected).
    assert!(fx.engine.list_entries(fx.entity).is_empty());
}

#[test]
fn cross_unit_requires_and_uses_recorded_prices() {
    let mut fx = fixture();

    // No price recorded on the entry: MISSING_PRICE.
    let mut e = fx.entry(
        "2026-01-10",
        "buy EUR with USD",
        vec![credit(fx.cash, "220"), debit(fx.bank_eur, "200")],
    );
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e.clone())),
        ErrorCode::MissingPrice
    );

    // Price that does not balance the entry: UNBALANCED_ENTRY.
    e.entry_id = id();
    e.prices = vec![fx.price(fx.eur, fx.usd, "1.2", "2026-01-10")];
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e.clone())),
        ErrorCode::UnbalancedEntry
    );

    // Exact at the recorded price (200 EUR × 1.1 = 220 USD): posts.
    e.entry_id = id();
    e.prices = vec![fx.price(fx.eur, fx.usd, "1.1", "2026-01-10")];
    fx.engine.post_entry(fx.actor, e).unwrap();

    // The inverse price direction works too: 100 EUR = 110 USD priced as
    // USD→EUR at 1/1.1 is not exact, so use a clean 2:1 pair instead.
    let mut e2 = fx.entry(
        "2026-01-11",
        "inverse direction",
        vec![credit(fx.cash, "100"), debit(fx.bank_eur, "200")],
    );
    e2.prices = vec![fx.price(fx.usd, fx.eur, "2", "2026-01-11")];
    fx.engine.post_entry(fx.actor, e2).unwrap();

    // Entry-embedded prices feed the projection (both directions recorded
    // by the two entries above).
    let (fact, inverted) = fx.engine.lookup_price(fx.eur, fx.usd).unwrap();
    assert_eq!(fact.rate, amt("1.1"));
    assert!(!inverted);
    let (fact, inverted) = fx.engine.lookup_price(fx.usd, fx.eur).unwrap();
    assert_eq!(fact.rate, amt("2"));
    assert!(!inverted);

    let report = fx.engine.equation_check(fx.chart).unwrap();
    assert!(
        report.holds,
        "cross-unit book must satisfy the equation: {report:?}"
    );
}

#[test]
fn unknown_and_inactive_accounts_rejected() {
    let mut fx = fixture();
    let ghost = id();
    let e = fx.entry(
        "2026-01-10",
        "ghost account",
        vec![debit(ghost, "10"), credit(fx.sales, "10")],
    );
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::UnknownAccount
    );

    fx.engine
        .set_account_active(id(), fx.actor, fx.spare, false)
        .unwrap();
    let e = fx.entry(
        "2026-01-10",
        "inactive account",
        vec![debit(fx.spare, "10"), credit(fx.sales, "10")],
    );
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InactiveAccount
    );

    // Reactivation is possible and is itself a ledger event.
    fx.engine
        .set_account_active(id(), fx.actor, fx.spare, true)
        .unwrap();
    let e = fx.entry(
        "2026-01-10",
        "active again",
        vec![debit(fx.spare, "10"), credit(fx.sales, "10")],
    );
    fx.engine.post_entry(fx.actor, e).unwrap();
    let status_events = fx
        .engine
        .audit_log()
        .iter()
        .filter(|r| r.event_type == EventType::AccountStatus)
        .count();
    assert_eq!(status_events, 2);
}

#[test]
fn lines_must_stay_within_one_chart_and_entity() {
    let mut fx = fixture();
    // A second chart in the same entity, holding its own account.
    let chart2 = fx
        .engine
        .create_chart(
            id(),
            fx.actor,
            NewChart {
                entity_id: fx.entity,
                name: "Alternative".into(),
                description: None,
                activate: false,
            },
        )
        .unwrap();
    let other_cash = fx
        .engine
        .create_account(
            id(),
            fx.actor,
            NewAccount {
                chart_id: chart2,
                name: "Cash".into(),
                code: None,
                account_type: AccountType::Asset,
                resource_type_id: fx.usd,
                parent_account_id: None,
                validation_rules: Value::Null,
                metadata: Value::Null,
            },
        )
        .unwrap();
    let e = fx.entry(
        "2026-01-10",
        "spans charts",
        vec![debit(other_cash, "10"), credit(fx.sales, "10")],
    );
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::ChartMismatch
    );

    // Posting entirely inside the second (non-active) chart is fine:
    // active-chart selection is workflow policy, not an engine invariant.
    let e = fx.entry(
        "2026-01-10",
        "within chart2",
        vec![debit(other_cash, "10"), credit(other_cash, "10")],
    );
    fx.engine.post_entry(fx.actor, e).unwrap();
}

#[test]
fn period_enforcement() {
    let mut fx = fixture();
    let balanced = |fx: &Fx, d: &str| {
        fx.entry(
            d,
            "period test",
            vec![debit(fx.cash, "5"), credit(fx.sales, "5")],
        )
    };

    // May 2026 is closed.
    let e = balanced(&fx, "2026-05-15");
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::PeriodClosed
    );

    // Outside any period.
    let e = balanced(&fx, "2027-01-01");
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::NoOpenPeriod
    );

    // Reopen May: posting succeeds; close it again: rejected again.
    fx.engine.reopen_period(id(), fx.actor, fx.may).unwrap();
    let e = balanced(&fx, "2026-05-15");
    fx.engine.post_entry(fx.actor, e).unwrap();
    fx.engine.close_period(id(), fx.actor, fx.may).unwrap();
    let e = balanced(&fx, "2026-05-16");
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::PeriodClosed
    );
}

#[test]
fn overlapping_periods_rejected() {
    let mut fx = fixture();
    let result = fx.engine.create_period(
        id(),
        fx.actor,
        NewPeriod {
            entity_id: fx.entity,
            name: "overlaps June".into(),
            start_date: date("2026-06-15"),
            end_date: date("2026-07-15"),
        },
    );
    assert_eq!(code(result), ErrorCode::InvalidInput);
}

#[test]
fn idempotency_replay_and_conflict() {
    let mut fx = fixture();
    let e = fx.entry(
        "2026-01-10",
        "posted once",
        vec![debit(fx.cash, "42"), credit(fx.sales, "42")],
    );
    let first = fx.engine.post_entry(fx.actor, e.clone()).unwrap();
    let log_len = fx.engine.audit_log().len();

    // Identical replay: same outcome, no new event, balances unchanged.
    let replayed = fx.engine.post_entry(fx.actor, e.clone()).unwrap();
    assert_eq!(first, replayed);
    assert_eq!(fx.engine.audit_log().len(), log_len);
    assert_eq!(fx.engine.get_balance(fx.cash).unwrap().natural, amt("42"));

    // Same id, different payload: IDEMPOTENCY_CONFLICT.
    let mut tampered = e.clone();
    tampered.description = "posted twice?".into();
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, tampered)),
        ErrorCode::IdempotencyConflict
    );

    // Idempotency also covers reference mutations.
    let op = id();
    let p1 = fx
        .engine
        .create_period(
            op,
            fx.actor,
            NewPeriod {
                entity_id: fx.entity,
                name: "2026-07".into(),
                start_date: date("2026-07-01"),
                end_date: date("2026-07-31"),
            },
        )
        .unwrap();
    let p2 = fx
        .engine
        .create_period(
            op,
            fx.actor,
            NewPeriod {
                entity_id: fx.entity,
                name: "2026-07".into(),
                start_date: date("2026-07-01"),
                end_date: date("2026-07-31"),
            },
        )
        .unwrap();
    assert_eq!(p1, p2);
}

#[test]
fn account_validation_rules_enforced() {
    let mut fx = fixture();

    // Missing memo.
    let e = fx.entry(
        "2026-01-10",
        "no memo",
        vec![debit(fx.strict, "50"), credit(fx.cash, "50")],
    );
    let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::ValidationFailed);
    assert_eq!(err.details["rule"], "require_memo");

    // Over max_amount.
    let mut line = debit(fx.strict, "150");
    line.memo = Some("big spend".into());
    let e = fx.entry("2026-01-10", "too big", vec![line, credit(fx.cash, "150")]);
    let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
    assert_eq!(err.details["rule"], "max_amount");

    // Wrong side (credit on a debit_only account).
    let mut line = credit(fx.strict, "10");
    line.memo = Some("refund".into());
    let e = fx.entry("2026-01-10", "wrong side", vec![line, debit(fx.cash, "10")]);
    let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
    assert_eq!(err.details["rule"], "side");

    // Compliant line posts.
    let mut line = debit(fx.strict, "99.99");
    line.memo = Some("office chair".into());
    let e = fx.entry("2026-01-10", "ok", vec![line, credit(fx.cash, "99.99")]);
    fx.engine.post_entry(fx.actor, e).unwrap();

    // Unknown rule keys are rejected at account creation.
    let result = fx.engine.create_account(
        id(),
        fx.actor,
        NewAccount {
            chart_id: fx.chart,
            name: "Bad Rules".into(),
            code: None,
            account_type: AccountType::Expense,
            resource_type_id: fx.usd,
            parent_account_id: None,
            validation_rules: serde_json::json!({ "no_such_rule": true }),
            metadata: Value::Null,
        },
    );
    assert_eq!(code(result), ErrorCode::InvalidInput);
}

#[test]
fn structural_failures_are_invalid_input() {
    let mut fx = fixture();

    // No lines.
    let e = fx.entry("2026-01-10", "empty", vec![]);
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InvalidInput
    );

    // Empty description.
    let e = fx.entry(
        "2026-01-10",
        "  ",
        vec![debit(fx.cash, "1"), credit(fx.sales, "1")],
    );
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InvalidInput
    );

    // Both sides set on one line.
    let mut bad = debit(fx.cash, "1");
    bad.credit_amount = Some(amt("1"));
    let e = fx.entry("2026-01-10", "both sides", vec![bad, credit(fx.sales, "1")]);
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InvalidInput
    );

    // Neither side set.
    let mut bad = debit(fx.cash, "1");
    bad.debit_amount = None;
    let e = fx.entry("2026-01-10", "no sides", vec![bad, credit(fx.sales, "0")]);
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InvalidInput
    );

    // Duplicate line ids.
    let l1 = debit(fx.cash, "1");
    let mut l2 = credit(fx.sales, "1");
    l2.line_id = l1.line_id;
    let e = fx.entry("2026-01-10", "dup line ids", vec![l1, l2]);
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InvalidInput
    );

    // Unknown entity.
    let mut e = fx.entry(
        "2026-01-10",
        "ghost entity",
        vec![debit(fx.cash, "1"), credit(fx.sales, "1")],
    );
    e.entity_id = id();
    assert_eq!(
        code(fx.engine.post_entry(fx.actor, e)),
        ErrorCode::InvalidInput
    );
}

#[test]
fn reversal_reuses_invariants() {
    let mut fx = fixture();

    // Unknown original.
    let result = fx.engine.reverse_entry(
        fx.actor,
        ReverseEntry {
            new_entry_id: id(),
            original_entry_id: id(),
            entry_date: date("2026-01-10"),
            description: None,
            metadata: Value::Null,
        },
    );
    assert_eq!(code(result), ErrorCode::InvalidInput);

    // A reversal dated into a closed period is rejected like any entry.
    let e = fx.entry(
        "2026-01-10",
        "to reverse",
        vec![debit(fx.cash, "10"), credit(fx.sales, "10")],
    );
    let original = fx.engine.post_entry(fx.actor, e).unwrap();
    let result = fx.engine.reverse_entry(
        fx.actor,
        ReverseEntry {
            new_entry_id: id(),
            original_entry_id: original,
            entry_date: date("2026-05-15"), // May is closed
            description: None,
            metadata: Value::Null,
        },
    );
    assert_eq!(code(result), ErrorCode::PeriodClosed);
}

#[test]
fn price_projection_latest_as_of_wins() {
    let mut fx = fixture();
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.eur, fx.usd, "1.05", "2026-01-01"),
        )
        .unwrap();
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.eur, fx.usd, "1.20", "2026-03-01"),
        )
        .unwrap();
    // An older fact never overwrites a newer one.
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.eur, fx.usd, "1.00", "2026-02-01"),
        )
        .unwrap();
    let (fact, inverted) = fx.engine.lookup_price(fx.eur, fx.usd).unwrap();
    assert!(!inverted);
    assert_eq!(fact.rate, amt("1.20"));
    // Inverse lookup finds the same fact.
    let (fact, inverted) = fx.engine.lookup_price(fx.usd, fx.eur).unwrap();
    assert!(inverted);
    assert_eq!(fact.rate, amt("1.20"));
}

#[test]
fn replay_rebuilds_identical_state() {
    let mut fx = fixture();
    let e = fx.entry(
        "2026-01-10",
        "funding",
        vec![debit(fx.cash, "500"), credit(fx.capital, "500")],
    );
    let posted = fx.engine.post_entry(fx.actor, e).unwrap();
    fx.engine
        .record_price(
            id(),
            fx.actor,
            fx.price(fx.eur, fx.usd, "1.1", "2026-01-10"),
        )
        .unwrap();
    fx.engine
        .reverse_entry(
            fx.actor,
            ReverseEntry {
                new_entry_id: id(),
                original_entry_id: posted,
                entry_date: date("2026-02-01"),
                description: None,
                metadata: Value::Null,
            },
        )
        .unwrap();

    let replayed = EngineState::replay(fx.book, fx.engine.audit_log()).unwrap();
    assert_eq!(&replayed, fx.engine.state());
}

#[test]
fn a_book_has_exactly_one_entity() {
    // Impl Plan M7: the fixture's own entity already exists; a second
    // create_entity call must be rejected structurally, not merely by
    // removing the client-facing route.
    let mut fx = fixture();
    let before = fx.engine.state().clone();
    let err = fx
        .engine
        .create_entity(id(), fx.actor, "Second Entity")
        .unwrap_err();
    assert_eq!(err.error_code, ErrorCode::InvalidInput);
    assert_eq!(
        fx.engine.state(),
        &before,
        "a rejected create_entity must not mutate state"
    );
}
