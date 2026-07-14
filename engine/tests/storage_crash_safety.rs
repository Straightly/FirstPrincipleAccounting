//! M3 storage tests (Impl Plan M3, Impl Spec §3, §7.5).
//!
//! Round-trip, wrong-passphrase, crash-safety (atomic replacement leaves the
//! pre-mutation file untouched), idempotency surviving a reload, and a
//! property-lite run that interleaves persist/reopen cycles so the M2
//! invariants are also proven against the file driver.

mod common;

use common::*;
use ledgerzero_engine::amount::Amount;
use ledgerzero_engine::domain::*;
use ledgerzero_engine::engine::*;
use ledgerzero_engine::storage::{
    Argon2Profile, BookStorage, FileBookStore, PassphraseKeyProvider, StorageError,
};
use ledgerzero_engine::types::FixedClock;
use ledgerzero_engine::{AccountingEngine, EngineState, ErrorCode};
use serde_json::Value;
use uuid::Uuid;

fn provider(passphrase: &str) -> PassphraseKeyProvider {
    PassphraseKeyProvider::with_profile(passphrase, Argon2Profile::TEST_FAST)
}

async fn reopen(dir: &std::path::Path, passphrase: &str, book: Uuid) -> AccountingEngine {
    let (_, events) = FileBookStore::open(dir, &provider(passphrase))
        .await
        .expect("open should succeed with the correct passphrase");
    let state = EngineState::replay(book, &events).expect("stored log must replay cleanly");
    AccountingEngine::from_state(state, Box::new(FixedClock::new(1_752_100_000_000)))
}

#[tokio::test]
async fn round_trip_create_persist_reopen_replay_matches() {
    let mut fx = fixture();
    fx.engine
        .post_entry(
            fx.actor,
            fx.entry(
                "2026-02-10",
                "rent",
                vec![debit(fx.rent, "500.00"), credit(fx.cash, "500.00")],
            ),
        )
        .unwrap();
    fx.engine
        .post_entry(
            fx.actor,
            fx.entry(
                "2026-03-05",
                "sale",
                vec![debit(fx.cash, "250.00"), credit(fx.sales, "250.00")],
            ),
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let store = FileBookStore::create(dir.path(), &provider("correct horse battery staple"))
        .await
        .unwrap();
    let all_ids: Vec<Uuid> = fx.engine.audit_log().iter().map(|e| e.event_id).collect();
    store
        .persist(fx.engine.audit_log(), &all_ids)
        .await
        .unwrap();

    let reopened = reopen(dir.path(), "correct horse battery staple", fx.book).await;
    assert_eq!(reopened.state(), fx.engine.state());
    assert_eq!(reopened.audit_log().len(), fx.engine.audit_log().len());

    // §3.3: the book folder is a git repo, committed to after each batch —
    // one commit for `create` (0 events) and one for the persist above.
    assert!(
        dir.path().join(".git").exists(),
        "book folder must be a git repo"
    );
    let log = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["log", "--oneline"])
        .output()
        .await
        .unwrap();
    let commit_count = String::from_utf8_lossy(&log.stdout).lines().count();
    assert_eq!(
        commit_count, 2,
        "expected one commit for create + one for persist"
    );
}

#[tokio::test]
async fn wrong_passphrase_is_rejected() {
    let fx = fixture();
    let dir = tempfile::tempdir().unwrap();
    let store = FileBookStore::create(dir.path(), &provider("right-passphrase"))
        .await
        .unwrap();
    store.persist(fx.engine.audit_log(), &[]).await.unwrap();

    let result = FileBookStore::open(dir.path(), &provider("wrong-passphrase")).await;
    match result {
        Err(StorageError::Crypto(_)) => {}
        Err(other) => panic!("expected a Crypto error for the wrong passphrase, got {other:?}"),
        Ok(_) => panic!("the wrong passphrase must not open the book"),
    }
}

#[tokio::test]
async fn crash_during_atomic_replace_preserves_pre_mutation_state() {
    let mut fx = fixture();
    fx.engine
        .post_entry(
            fx.actor,
            fx.entry(
                "2026-01-10",
                "opening",
                vec![debit(fx.cash, "1000.00"), credit(fx.capital, "1000.00")],
            ),
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let store = FileBookStore::create(dir.path(), &provider("a-passphrase"))
        .await
        .unwrap();
    store.persist(fx.engine.audit_log(), &[]).await.unwrap();
    let pre_crash_events = store.load().await.unwrap();
    assert_eq!(pre_crash_events.len(), fx.engine.audit_log().len());

    // Simulate a process death after the temp file was opened but before
    // fsync+rename completed: leave a corrupt/partial temp file next to the
    // real data file, exactly as `atomic_write` would while writing.
    let tmp_path = dir.path().join("book.data.enc.tmp");
    tokio::fs::write(&tmp_path, b"garbage from an interrupted write")
        .await
        .unwrap();
    assert!(tmp_path.exists());

    // A fresh load must ignore the stray temp file entirely and return
    // exactly the pre-crash state — `book.data.enc` was never touched.
    let after_crash = store.load().await.unwrap();
    assert_eq!(after_crash, pre_crash_events);

    let (_, reopened_events) = FileBookStore::open(dir.path(), &provider("a-passphrase"))
        .await
        .unwrap();
    assert_eq!(reopened_events, pre_crash_events);

    // A subsequent legitimate mutation still succeeds and fully replaces
    // both the stray temp file and the data file.
    fx.engine
        .post_entry(
            fx.actor,
            fx.entry(
                "2026-01-11",
                "second",
                vec![debit(fx.rent, "50.00"), credit(fx.cash, "50.00")],
            ),
        )
        .unwrap();
    store.persist(fx.engine.audit_log(), &[]).await.unwrap();
    let (_, final_events) = FileBookStore::open(dir.path(), &provider("a-passphrase"))
        .await
        .unwrap();
    assert_eq!(final_events.len(), fx.engine.audit_log().len());
}

#[tokio::test]
async fn idempotent_replay_and_conflict_survive_reload() {
    let mut fx = fixture();
    let new_entry = fx.entry(
        "2026-04-01",
        "reloaded idempotency",
        vec![debit(fx.cash, "75.00"), credit(fx.sales, "75.00")],
    );
    let entry_id = new_entry.entry_id;
    fx.engine.post_entry(fx.actor, new_entry.clone()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let store = FileBookStore::create(dir.path(), &provider("reload-idempotency"))
        .await
        .unwrap();
    store.persist(fx.engine.audit_log(), &[]).await.unwrap();

    let mut reloaded = reopen(dir.path(), "reload-idempotency", fx.book).await;
    let log_len_before = reloaded.audit_log().len();

    // Identical replay after reload returns the original outcome and
    // appends nothing — the idempotency index was fully rebuilt by replay.
    let outcome = reloaded.post_entry(fx.actor, new_entry.clone()).unwrap();
    assert_eq!(outcome, entry_id);
    assert_eq!(reloaded.audit_log().len(), log_len_before);

    // A mutated payload under the same client id conflicts, post-reload.
    let mut tampered = new_entry;
    tampered.description = "tampered after reload".into();
    let err = reloaded.post_entry(fx.actor, tampered).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::IdempotencyConflict);
}

/// A cut-down version of the M2 property harness (engine/tests/property_replay.rs)
/// that interleaves persist/reopen cycles, proving the file driver preserves
/// every M2 invariant, not just the in-memory one.
#[tokio::test]
async fn property_lite_persist_reopen_cycles() {
    struct Rng(u64);
    impl Rng {
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
        fn whole(&mut self, max: u64) -> Amount {
            format!("{}.00", 1 + self.below(max)).parse().unwrap()
        }
    }

    let mut rng = Rng(20260711);
    let mut fx = fixture();
    let pool = [fx.cash, fx.loan, fx.capital, fx.sales, fx.rent];

    let dir = tempfile::tempdir().unwrap();
    let store = FileBookStore::create(dir.path(), &provider("property-lite"))
        .await
        .unwrap();

    for cycle in 0..4 {
        for _ in 0..15 {
            let a = pool[rng.below(pool.len() as u64) as usize];
            let mut b = pool[rng.below(pool.len() as u64) as usize];
            if b == a {
                b = pool[(rng.below(pool.len() as u64) as usize + 1) % pool.len()];
            }
            let amount = rng.whole(200);
            let entry = NewEntry {
                entry_id: Uuid::new_v4(),
                entity_id: fx.entity,
                entry_date: "2026-02-15".parse().unwrap(),
                description: format!("cycle {cycle}"),
                lines: vec![
                    debit(a, &amount.to_string()),
                    credit(b, &amount.to_string()),
                ],
                prices: Vec::new(),
                source: EntrySource::Manual,
                metadata: Value::Null,
                workflow: None,
            };
            fx.engine.post_entry(fx.actor, entry).unwrap();
        }

        store.persist(fx.engine.audit_log(), &[]).await.unwrap();
        let (_, events) = FileBookStore::open(dir.path(), &provider("property-lite"))
            .await
            .unwrap();
        let replayed = EngineState::replay(fx.book, &events).unwrap();
        assert_eq!(
            &replayed,
            fx.engine.state(),
            "cycle {cycle}: drift after reopen"
        );

        let report = fx.engine.equation_check(fx.chart).unwrap();
        assert!(report.holds, "cycle {cycle}: equation violated: {report:?}");
    }
}
