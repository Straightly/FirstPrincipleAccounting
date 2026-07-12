//! M2 property and replay tests (Impl Plan M2, Impl Spec §7.5).
//!
//! Property: any generated entry either posts balanced or is rejected with a
//! structured error that leaves state untouched; no operation sequence
//! unbalances a chart. Replay: projections rebuilt from the event log equal
//! the incrementally maintained state.
//!
//! Uses a small deterministic xorshift PRNG instead of a property-testing
//! crate, keeping the engine's dependency set at serde/serde_json/uuid.

mod common;

use common::*;
use ledgerzero_engine::amount::{Amount, SCALE};
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use ledgerzero_engine::{EngineState, ErrorCode};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

/// xorshift64 — deterministic, dependency-free.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    fn uuid(&mut self) -> Uuid {
        Uuid::from_u128(((self.next_u64() as u128) << 64) | self.next_u64() as u128)
    }

    /// Whole-unit amount in 1..=max.
    fn whole(&mut self, max: u64) -> Amount {
        Amount::from_raw(((1 + self.below(max)) as i128) * SCALE).unwrap()
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

fn rng_line(
    rng: &mut Rng,
    account_id: Uuid,
    debit: Option<Amount>,
    credit: Option<Amount>,
) -> NewLine {
    NewLine {
        line_id: rng.uuid(),
        account_id,
        debit_amount: debit,
        credit_amount: credit,
        memo: None,
        metadata: Value::Null,
    }
}

const OPEN_DATES: [&str; 4] = ["2026-01-15", "2026-02-10", "2026-03-20", "2026-04-05"];

/// A balanced single-unit entry over USD accounts that carry no validation
/// rules and are never deactivated by this test.
fn gen_valid_single_unit(rng: &mut Rng, fx: &Fx) -> NewEntry {
    let pool = [fx.cash, fx.loan, fx.capital, fx.sales, fx.rent];
    let n_random = 1 + rng.below(3) as usize; // 1..=3 lines, plus balancer
    let mut lines = Vec::new();
    let mut net: i128 = 0; // debit − credit, raw
    for _ in 0..n_random {
        let account = *rng.pick(&pool);
        let amount = rng.whole(1_000);
        if rng.below(2) == 0 {
            net += amount.raw();
            lines.push(rng_line(rng, account, Some(amount), None));
        } else {
            net -= amount.raw();
            lines.push(rng_line(rng, account, None, Some(amount)));
        }
    }
    let balancer = *rng.pick(&pool);
    let closing = Amount::from_raw(net.abs()).unwrap();
    if net >= 0 {
        lines.push(rng_line(rng, balancer, None, Some(closing)));
    } else {
        lines.push(rng_line(rng, balancer, Some(closing), None));
    }
    NewEntry {
        entry_id: rng.uuid(),
        entity_id: fx.entity,
        entry_date: date(rng.pick(&OPEN_DATES)),
        description: "generated balanced entry".into(),
        lines,
        prices: Vec::new(),
        source: EntrySource::Manual,
        metadata: Value::Null,
    }
}

/// A balanced cross-unit entry: Dr Cash (r·k USD) / Cr Bank EUR (k EUR)
/// priced EUR→USD at the whole rate r, so it balances exactly.
fn gen_valid_cross_unit(rng: &mut Rng, fx: &Fx) -> NewEntry {
    let k = rng.whole(500);
    let r = 1 + rng.below(4) as i128;
    let usd_amount = Amount::from_raw(k.raw() * r).unwrap();
    let rate = Amount::from_raw(r * SCALE).unwrap();
    let entry_date = date(rng.pick(&OPEN_DATES));
    NewEntry {
        entry_id: rng.uuid(),
        entity_id: fx.entity,
        entry_date: entry_date.clone(),
        description: "generated cross-unit entry".into(),
        lines: vec![
            rng_line(rng, fx.cash, Some(usd_amount), None),
            rng_line(rng, fx.bank_eur, None, Some(k)),
        ],
        prices: vec![PriceFact {
            base_resource_type_id: fx.eur,
            quote_resource_type_id: fx.usd,
            rate,
            as_of: entry_date,
        }],
        source: EntrySource::Manual,
        metadata: Value::Null,
    }
}

fn assert_catalog_error(err: &ledgerzero_engine::EngineError) {
    // Every rejection must carry a code from the §4.4 catalog; in M2 the
    // engine itself can raise this subset.
    let m2_codes = [
        ErrorCode::UnbalancedEntry,
        ErrorCode::MissingPrice,
        ErrorCode::UnknownAccount,
        ErrorCode::InactiveAccount,
        ErrorCode::ChartMismatch,
        ErrorCode::PeriodClosed,
        ErrorCode::NoOpenPeriod,
        ErrorCode::IdempotencyConflict,
        ErrorCode::ValidationFailed,
        ErrorCode::InvalidInput,
    ];
    assert!(
        m2_codes.contains(&err.error_code),
        "unexpected error code: {err:?}"
    );
}

#[test]
fn property_posted_or_rejected_and_replay() {
    for seed in [7, 42, 20260711] {
        run_sequence(seed, 300);
    }
}

fn run_sequence(seed: u64, ops: usize) {
    let mut rng = Rng::new(seed);
    let mut fx = fixture();
    let mut posted: Vec<Uuid> = Vec::new();
    let mut replayable: Option<NewEntry> = None;
    let mut spare_active = true;

    for i in 0..ops {
        let before = fx.engine.state().clone();
        let log_before = fx.engine.audit_log().len();
        let roll = rng.below(100);

        if roll < 30 {
            // Valid balanced single-unit entry: must post.
            let e = gen_valid_single_unit(&mut rng, &fx);
            let entry_id = e.entry_id;
            fx.engine
                .post_entry(fx.actor, e.clone())
                .unwrap_or_else(|err| panic!("op {i}: valid entry rejected: {err:?}"));
            posted.push(entry_id);
            replayable = Some(e);
            assert_eq!(fx.engine.audit_log().len(), log_before + 1);
        } else if roll < 45 {
            // Valid balanced cross-unit entry: must post.
            let e = gen_valid_cross_unit(&mut rng, &fx);
            let entry_id = e.entry_id;
            fx.engine
                .post_entry(fx.actor, e)
                .unwrap_or_else(|err| panic!("op {i}: valid cross-unit rejected: {err:?}"));
            posted.push(entry_id);
        } else if roll < 55 {
            // Unbalanced: must reject and leave no trace.
            let amount = rng.whole(1_000);
            let off = Amount::from_raw(amount.raw() + 1).unwrap(); // off by 1e-8
            let e = NewEntry {
                entry_id: rng.uuid(),
                entity_id: fx.entity,
                entry_date: date(rng.pick(&OPEN_DATES)),
                description: "unbalanced".into(),
                lines: vec![
                    rng_line(&mut rng, fx.cash, Some(amount), None),
                    rng_line(&mut rng, fx.sales, None, Some(off)),
                ],
                prices: Vec::new(),
                source: EntrySource::Manual,
                metadata: Value::Null,
            };
            let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
            assert_eq!(err.error_code, ErrorCode::UnbalancedEntry);
            assert_eq!(
                fx.engine.state(),
                &before,
                "rejection must not mutate state"
            );
        } else if roll < 62 {
            // Cross-unit without a price: MISSING_PRICE.
            let k = rng.whole(100);
            let e = NewEntry {
                entry_id: rng.uuid(),
                entity_id: fx.entity,
                entry_date: date(rng.pick(&OPEN_DATES)),
                description: "no price".into(),
                lines: vec![
                    rng_line(&mut rng, fx.cash, Some(k), None),
                    rng_line(&mut rng, fx.bank_eur, None, Some(k)),
                ],
                prices: Vec::new(),
                source: EntrySource::Manual,
                metadata: Value::Null,
            };
            let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
            assert_eq!(err.error_code, ErrorCode::MissingPrice);
            assert_eq!(fx.engine.state(), &before);
        } else if roll < 69 {
            // Closed period / no period.
            let bad_date = if rng.below(2) == 0 {
                ("2026-05-15", ErrorCode::PeriodClosed)
            } else {
                ("2027-03-01", ErrorCode::NoOpenPeriod)
            };
            let amount = rng.whole(100);
            let e = NewEntry {
                entry_id: rng.uuid(),
                entity_id: fx.entity,
                entry_date: date(bad_date.0),
                description: "bad period".into(),
                lines: vec![
                    rng_line(&mut rng, fx.cash, Some(amount), None),
                    rng_line(&mut rng, fx.sales, None, Some(amount)),
                ],
                prices: Vec::new(),
                source: EntrySource::Manual,
                metadata: Value::Null,
            };
            let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
            assert_eq!(err.error_code, bad_date.1);
            assert_eq!(fx.engine.state(), &before);
        } else if roll < 76 {
            // Idempotency: identical replay returns the original outcome and
            // records nothing; a tampered payload conflicts.
            if let Some(e) = &replayable {
                let outcome = fx.engine.post_entry(fx.actor, e.clone()).unwrap();
                assert_eq!(outcome, e.entry_id);
                assert_eq!(
                    fx.engine.audit_log().len(),
                    log_before,
                    "replay must not append"
                );
                assert_eq!(fx.engine.state(), &before);

                let mut tampered = e.clone();
                tampered.description = "tampered".into();
                let err = fx.engine.post_entry(fx.actor, tampered).unwrap_err();
                assert_eq!(err.error_code, ErrorCode::IdempotencyConflict);
                assert_eq!(fx.engine.state(), &before);
            }
        } else if roll < 84 {
            // Reverse a random posted entry. May legitimately fail (e.g. an
            // entry touching the spare account while it is inactive); any
            // failure must be a catalog error that changes nothing.
            let picked = posted
                .get(rng.below(posted.len().max(1) as u64) as usize)
                .copied();
            if let Some(original) = picked {
                let result = fx.engine.reverse_entry(
                    fx.actor,
                    ReverseEntry {
                        new_entry_id: rng.uuid(),
                        original_entry_id: original,
                        entry_date: date(rng.pick(&OPEN_DATES)),
                        description: None,
                        metadata: Value::Null,
                    },
                );
                match result {
                    Ok(reversal_id) => {
                        posted.push(reversal_id);
                        assert_eq!(fx.engine.audit_log().len(), log_before + 1);
                    }
                    Err(err) => {
                        assert_catalog_error(&err);
                        assert_eq!(fx.engine.state(), &before);
                    }
                }
            }
        } else if roll < 90 {
            // Record a standalone price fact: always fine.
            let (base, quote) = if rng.below(2) == 0 {
                (fx.eur, fx.usd)
            } else {
                (fx.widget, fx.usd)
            };
            fx.engine
                .record_price(
                    rng.uuid(),
                    fx.actor,
                    PriceFact {
                        base_resource_type_id: base,
                        quote_resource_type_id: quote,
                        rate: rng.whole(10),
                        as_of: date(rng.pick(&OPEN_DATES)),
                    },
                )
                .unwrap();
        } else if roll < 95 {
            // Toggle the spare account, and sometimes hit it while inactive.
            if rng.below(2) == 0 {
                spare_active = !spare_active;
                fx.engine
                    .set_account_active(rng.uuid(), fx.actor, fx.spare, spare_active)
                    .unwrap();
            } else {
                let amount = rng.whole(50);
                let e = NewEntry {
                    entry_id: rng.uuid(),
                    entity_id: fx.entity,
                    entry_date: date(rng.pick(&OPEN_DATES)),
                    description: "spare usage".into(),
                    lines: vec![
                        rng_line(&mut rng, fx.spare, Some(amount), None),
                        rng_line(&mut rng, fx.cash, None, Some(amount)),
                    ],
                    prices: Vec::new(),
                    source: EntrySource::Manual,
                    metadata: Value::Null,
                };
                let result = fx.engine.post_entry(fx.actor, e);
                if spare_active {
                    posted.push(result.expect("active spare account must accept"));
                } else {
                    let err = result.unwrap_err();
                    assert_eq!(err.error_code, ErrorCode::InactiveAccount);
                    assert_eq!(fx.engine.state(), &before);
                }
            }
        } else {
            // Structural junk: duplicate line ids or a two-sided line.
            let amount = rng.whole(10);
            let mut l1 = rng_line(&mut rng, fx.cash, Some(amount), None);
            let mut l2 = rng_line(&mut rng, fx.sales, None, Some(amount));
            if rng.below(2) == 0 {
                l2.line_id = l1.line_id;
            } else {
                l1.credit_amount = Some(amount);
            }
            let e = NewEntry {
                entry_id: rng.uuid(),
                entity_id: fx.entity,
                entry_date: date(rng.pick(&OPEN_DATES)),
                description: "structural junk".into(),
                lines: vec![l1, l2],
                prices: Vec::new(),
                source: EntrySource::Manual,
                metadata: Value::Null,
            };
            let err = fx.engine.post_entry(fx.actor, e).unwrap_err();
            assert_eq!(err.error_code, ErrorCode::InvalidInput);
            assert_eq!(fx.engine.state(), &before);
        }
    }

    // --- Invariants after the whole sequence -------------------------------

    // Every posted entry still balances exactly at its own recorded prices.
    fx.engine
        .verify_all_entries()
        .unwrap_or_else(|err| panic!("seed {seed}: ledger corrupt: {err:?}"));

    // No operation sequence unbalances the chart: A = L + E + (R − X).
    let report = fx.engine.equation_check(fx.chart).unwrap();
    assert!(report.holds, "seed {seed}: equation violated: {report:?}");

    // Balances projection equals an independent fold over the event log.
    let mut recomputed: BTreeMap<Uuid, (i128, i128)> = BTreeMap::new();
    for record in fx.engine.audit_log() {
        if let EventPayload::EntryPosted { entry } = &record.payload {
            for line in &entry.lines {
                let slot = recomputed.entry(line.account_id).or_insert((0, 0));
                slot.0 += line.debit_amount.map(|a| a.raw()).unwrap_or(0);
                slot.1 += line.credit_amount.map(|a| a.raw()).unwrap_or(0);
            }
        }
    }
    for account_id in [
        fx.cash,
        fx.bank_eur,
        fx.inventory,
        fx.loan,
        fx.capital,
        fx.sales,
        fx.rent,
        fx.strict,
        fx.spare,
    ] {
        let view = fx.engine.get_balance(account_id).unwrap();
        let (debits, credits) = recomputed.get(&account_id).copied().unwrap_or((0, 0));
        assert_eq!(view.debit_total.raw(), debits, "seed {seed}: debit drift");
        assert_eq!(
            view.credit_total.raw(),
            credits,
            "seed {seed}: credit drift"
        );
    }

    // Replay: rebuilding every projection from the log yields identical state.
    let replayed = EngineState::replay(fx.book, fx.engine.audit_log()).unwrap();
    assert_eq!(&replayed, fx.engine.state(), "seed {seed}: replay drift");
}
