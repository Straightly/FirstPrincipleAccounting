//! M9 storage/engine tests (Impl Spec §8.2, Impl Plan M9): export-bundle
//! crypto round-trip independent of the book's own key, `record_restore`'s
//! marker event, and `FileBookStore::restore`'s wipe-and-replace behavior
//! against both a fresh and an already-occupied location.

mod common;

use common::*;
use ledgerzero_engine::domain::EventPayload;
use ledgerzero_engine::storage::{
    create_export_bundle, open_export_bundle, Argon2Profile, BookStorage, FileBookStore,
    PassphraseKeyProvider, StorageError,
};
use ledgerzero_engine::types::FixedClock;
use ledgerzero_engine::{AccountingEngine, EngineState};
use uuid::Uuid;

fn provider(passphrase: &str) -> PassphraseKeyProvider {
    PassphraseKeyProvider::with_profile(passphrase, Argon2Profile::TEST_FAST)
}

#[test]
fn export_bundle_round_trip_is_key_independent_of_the_book_key() {
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

    let bundle = create_export_bundle(
        fx.book,
        fx.engine.audit_log(),
        1_752_200_000_000,
        "reader pass",
    )
    .unwrap();
    assert_eq!(bundle.book_id, fx.book);
    assert_eq!(bundle.event_count, fx.engine.audit_log().len());
    // The bundle is its own self-contained crypto envelope — no reference
    // to any book-key material.
    assert_eq!(bundle.kdf, "argon2id");

    let recovered = open_export_bundle(&bundle, "reader pass").unwrap();
    assert_eq!(recovered, fx.engine.audit_log());
}

#[test]
fn wrong_reader_passphrase_is_rejected() {
    let fx = fixture();
    let bundle = create_export_bundle(
        fx.book,
        fx.engine.audit_log(),
        1_752_200_000_000,
        "right pass",
    )
    .unwrap();
    match open_export_bundle(&bundle, "wrong pass") {
        Err(StorageError::Crypto(_)) => {}
        other => panic!("expected a Crypto error for the wrong reader passphrase, got {other:?}"),
    }
}

#[test]
fn export_never_mutates_the_source_log() {
    let fx = fixture();
    let before = fx.engine.audit_log().len();
    let _ = create_export_bundle(fx.book, fx.engine.audit_log(), 1_752_200_000_000, "p").unwrap();
    assert_eq!(fx.engine.audit_log().len(), before);
}

#[test]
fn record_restore_appends_one_marker_and_is_idempotent() {
    let mut fx = fixture();
    let before = fx.engine.audit_log().len();
    let op_id = id();
    let source_book_id = Uuid::new_v4();

    let first = fx
        .engine
        .record_restore(op_id, fx.actor, source_book_id, 1_752_200_000_000, before)
        .unwrap();
    assert_eq!(fx.engine.audit_log().len(), before + 1);
    let last = fx.engine.audit_log().last().unwrap();
    assert_eq!(last.event_id, op_id);
    // `Restored` has no single created object, like `PriceRecorded` —
    // outcome_id is nil, not a meaningful id to compare against.
    assert!(first.is_nil());
    match &last.payload {
        EventPayload::Restored {
            source_book_id: s,
            restored_event_count,
            ..
        } => {
            assert_eq!(*s, source_book_id);
            assert_eq!(*restored_event_count, before);
        }
        other => panic!("expected a Restored payload, got {other:?}"),
    }

    // Identical replay appends nothing (Impl Spec §4.1.6).
    let replay = fx
        .engine
        .record_restore(op_id, fx.actor, source_book_id, 1_752_200_000_000, before)
        .unwrap();
    assert_eq!(replay, first);
    assert_eq!(fx.engine.audit_log().len(), before + 1);
}

#[tokio::test]
async fn restore_into_a_fresh_location_is_loadable() {
    let fx = fixture();
    let dir = tempfile::tempdir().unwrap();
    let store =
        FileBookStore::restore(dir.path(), &provider("restore pass"), fx.engine.audit_log())
            .await
            .unwrap();
    let loaded = store.load().await.unwrap();
    assert_eq!(loaded, fx.engine.audit_log());

    let (_, reopened_events) = FileBookStore::open(dir.path(), &provider("restore pass"))
        .await
        .unwrap();
    let state = EngineState::replay(fx.book, &reopened_events).unwrap();
    assert_eq!(&state, fx.engine.state());
}

#[tokio::test]
async fn restore_wipes_and_replaces_an_existing_book() {
    let fx_a = fixture();
    let dir = tempfile::tempdir().unwrap();

    // Original book occupying the location, under its own passphrase.
    let store_a = FileBookStore::create(dir.path(), &provider("original pass"))
        .await
        .unwrap();
    store_a.persist(fx_a.engine.audit_log(), &[]).await.unwrap();

    // A different book's export, restored on top under a *different*
    // passphrase — restore never reuses the exported book's old key.
    let fx_b = fixture();
    let mut engine_b = AccountingEngine::from_state(
        fx_b.engine.state().clone(),
        Box::new(FixedClock::new(1_752_300_000_000)),
    );
    engine_b
        .record_restore(
            id(),
            fx_b.actor,
            fx_b.book,
            1_752_299_000_000,
            engine_b.audit_log().len(),
        )
        .unwrap();

    let store_b = FileBookStore::restore(dir.path(), &provider("new pass"), engine_b.audit_log())
        .await
        .unwrap();
    let loaded = store_b.load().await.unwrap();
    assert_eq!(loaded, engine_b.audit_log());
    assert_ne!(loaded, fx_a.engine.audit_log());

    // The old passphrase no longer opens anything at this location.
    match FileBookStore::open(dir.path(), &provider("original pass")).await {
        Err(StorageError::Crypto(_)) => {}
        Err(other) => panic!("expected a Crypto error, got {other:?}"),
        Ok(_) => panic!("expected the old passphrase to be rejected"),
    }
}

#[tokio::test]
async fn create_still_refuses_an_existing_book_unlike_restore() {
    let fx = fixture();
    let dir = tempfile::tempdir().unwrap();
    FileBookStore::create(dir.path(), &provider("p"))
        .await
        .unwrap();

    match FileBookStore::create(dir.path(), &provider("p")).await {
        Err(StorageError::Corrupt(_)) => {}
        Err(other) => panic!("expected a Corrupt error, got {other:?}"),
        Ok(_) => panic!("expected create to refuse an existing book"),
    }

    // restore, by contrast, is fine with it.
    FileBookStore::restore(dir.path(), &provider("p"), fx.engine.audit_log())
        .await
        .unwrap();
}
