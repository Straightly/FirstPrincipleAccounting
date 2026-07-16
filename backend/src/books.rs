//! Book registry: on-disk book folders under `books_dir`, and the set of
//! books currently open in backend memory (Impl Spec §5.3, §5.4; Impl Plan M4).
//!
//! `book.json` is plaintext metadata (book_id, name, owner) living beside
//! the engine's own `book.data.enc`/`book.keystore.json` — it is not
//! accounting data, so it does not go through the encrypted storage
//! boundary. `create_accounting_book`/`open_book` are the only ways a book
//! enters the open-books map; every other book API requires it already be
//! there (`BOOK_NOT_OPEN` otherwise, Impl Spec §4.4).

use crate::error::ApiError;
use axum::http::StatusCode;
use ledgerzero_engine::storage::{
    BookStorage, FileBookStore, PassphraseKeyProvider, StorageError, DATA_FILE, KEYSTORE_FILE,
};
use ledgerzero_engine::types::SystemClock;
use ledgerzero_engine::{AccountingEngine, EngineError, EngineState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

const META_FILE: &str = "book.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMeta {
    pub book_id: Uuid,
    pub name: String,
    pub owner_email: String,
    pub created_at_ms: i64,
    /// A book has exactly one entity, auto-created with the book (Impl Plan
    /// M7); carried here so callers never need a separate discovery round
    /// trip to learn it.
    pub entity_id: Uuid,
}

/// An open book: the live engine plus the storage handle that persists it.
pub struct OpenBook {
    pub meta: BookMeta,
    pub engine: RwLock<AccountingEngine>,
    store: FileBookStore,
}

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> ApiError {
        use ledgerzero_engine::ErrorCode::*;
        let status = match err.error_code {
            InvalidInput | InvalidExecutionContext => StatusCode::BAD_REQUEST,
            IdempotencyConflict | BookNotOpen => StatusCode::CONFLICT,
            UnauthorizedWorkflow | UnauthorizedApi => StatusCode::FORBIDDEN,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        ApiError {
            error_code: err.error_code.as_str().to_string(),
            message: err.message,
            details: if err.details.is_null() {
                None
            } else {
                Some(err.details)
            },
            status: status.as_u16(),
        }
    }
}

fn storage_err(err: StorageError) -> ApiError {
    match err {
        StorageError::Crypto(m) => ApiError::new(StatusCode::UNAUTHORIZED, "WRONG_PASSPHRASE", m),
        other => ApiError::internal(other.to_string()),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

async fn write_meta(dir: &Path, meta: &BookMeta) -> Result<(), ApiError> {
    let json = serde_json::to_vec_pretty(meta).map_err(|e| ApiError::internal(e.to_string()))?;
    tokio::fs::write(dir.join(META_FILE), json)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))
}

async fn read_meta(dir: &Path) -> Result<BookMeta, ApiError> {
    let bytes = tokio::fs::read(dir.join(META_FILE)).await.map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "UNKNOWN_BOOK",
            "no book with this id",
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|e| ApiError::internal(e.to_string()))
}

pub struct BooksRegistry {
    dir: PathBuf,
    open: RwLock<HashMap<Uuid, Arc<OpenBook>>>,
}

impl BooksRegistry {
    pub fn new(books_dir: &str) -> BooksRegistry {
        BooksRegistry {
            dir: PathBuf::from(books_dir),
            open: RwLock::new(HashMap::new()),
        }
    }

    fn book_dir(&self, book_id: Uuid) -> PathBuf {
        self.dir.join(book_id.to_string())
    }

    /// Bootstrap-owner-gated (Impl Spec §5.3): creates the folder and the
    /// encrypted event log, auto-creates the book's one entity (Impl Plan
    /// M7 — a book has exactly one, never created separately), and holds
    /// the book open in memory so the caller can act immediately.
    pub async fn create(
        &self,
        name: String,
        passphrase: &str,
        owner_email: &str,
        owner_user_id: Uuid,
    ) -> Result<BookMeta, ApiError> {
        let book_id = Uuid::new_v4();
        let dir = self.book_dir(book_id);
        let provider = PassphraseKeyProvider::new(passphrase);
        let store = FileBookStore::create(&dir, &provider)
            .await
            .map_err(storage_err)?;
        let mut engine = AccountingEngine::new(book_id, Box::new(SystemClock));
        let entity_id = engine.create_entity(Uuid::new_v4(), owner_user_id, &name)?;
        let new_ids: Vec<Uuid> = engine.audit_log().iter().map(|e| e.event_id).collect();
        store
            .persist(engine.audit_log(), &new_ids)
            .await
            .map_err(storage_err)?;
        let meta = BookMeta {
            book_id,
            name,
            owner_email: owner_email.to_string(),
            created_at_ms: now_ms(),
            entity_id,
        };
        write_meta(&dir, &meta).await?;
        let open_book = Arc::new(OpenBook {
            meta: meta.clone(),
            engine: RwLock::new(engine),
            store,
        });
        self.open.write().await.insert(book_id, open_book);
        Ok(meta)
    }

    /// Owner passphrase → key into memory (Impl Spec §5.4). Idempotent: an
    /// already-open book returns its metadata without touching disk again.
    pub async fn open(&self, book_id: Uuid, passphrase: &str) -> Result<BookMeta, ApiError> {
        if let Some(existing) = self.open.read().await.get(&book_id) {
            return Ok(existing.meta.clone());
        }
        let dir = self.book_dir(book_id);
        let meta = read_meta(&dir).await?;
        let provider = PassphraseKeyProvider::new(passphrase);
        let (store, events) = FileBookStore::open(&dir, &provider)
            .await
            .map_err(storage_err)?;
        let state = EngineState::replay(book_id, &events)
            .map_err(|e| ApiError::internal(format!("stored event log failed to replay: {e}")))?;
        let engine = AccountingEngine::from_state(state, Box::new(SystemClock));
        let open_book = Arc::new(OpenBook {
            meta: meta.clone(),
            engine: RwLock::new(engine),
            store,
        });
        self.open.write().await.insert(book_id, open_book);
        Ok(meta)
    }

    /// All books that exist on disk, open or not.
    pub async fn list(&self) -> Result<Vec<BookMeta>, ApiError> {
        let mut metas = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(metas),
            Err(e) => return Err(ApiError::internal(e.to_string())),
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
        {
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if let Ok(meta) = read_meta(&entry.path()).await {
                    metas.push(meta);
                }
            }
        }
        metas.sort_by_key(|a| a.created_at_ms);
        Ok(metas)
    }

    pub async fn get_open(&self, book_id: Uuid) -> Result<Arc<OpenBook>, ApiError> {
        self.open
            .read()
            .await
            .get(&book_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::CONFLICT,
                    "BOOK_NOT_OPEN",
                    "book is not open; call open_book first",
                )
            })
    }

    /// Every currently open book (Impl Plan M6): the picker's discovery
    /// scope for non-owner users is intentionally limited to books already
    /// open in this process, not every book folder on disk — a book a
    /// non-owner is assigned into is only reachable once its owner has
    /// opened it.
    pub async fn list_open(&self) -> Vec<Arc<OpenBook>> {
        self.open.read().await.values().cloned().collect()
    }

    /// Bootstrap-owner-gated (Impl Spec §7.3, Impl Plan M9, resolution R3):
    /// copies a book's three portable files — `book.json`, `book.data.enc`,
    /// `book.keystore.json` — to `location` verbatim. No decryption, so
    /// this needs neither an open book nor any passphrase, and works
    /// whether or not the book is currently open. Safe against a book
    /// being actively mutated concurrently: `book.data.enc` is only ever
    /// replaced via atomic rename (engine `storage.rs`), so a read from
    /// outside any lock always sees a complete file, never a torn one.
    pub async fn backup(&self, book_id: Uuid, location: &Path) -> Result<(), ApiError> {
        let source = self.book_dir(book_id);
        read_meta(&source).await?; // confirms the book actually exists
        tokio::fs::create_dir_all(location)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        for name in [META_FILE, DATA_FILE, KEYSTORE_FILE] {
            tokio::fs::copy(source.join(name), location.join(name))
                .await
                .map_err(|e| ApiError::internal(format!("failed to copy {name}: {e}")))?;
        }
        Ok(())
    }

    /// Bootstrap-owner-gated (Impl Plan M9): releases `book_id` from the
    /// in-memory open-books map. Idempotent — a no-op if it isn't open.
    /// Exists specifically so `restore` (below) has a way to act on a book
    /// this process currently holds open.
    pub async fn close(&self, book_id: Uuid) {
        self.open.write().await.remove(&book_id);
    }

    /// Bootstrap-owner-gated (Impl Spec §7.3, Impl Plan M9, resolution R3):
    /// wipe-and-replace restore from `location`. `book_id` comes from
    /// `location`'s own plaintext `book.json` — never a caller-supplied
    /// one, so restore always preserves the logical book_id. Refuses if
    /// that book_id is currently open in this process (`close` first);
    /// otherwise allowed whether or not a book already exists on disk at
    /// that id — a damaged location being intentionally replaced is the
    /// point. Never decrypts anything, so the restored book is left
    /// closed: the owner calls `open` afterward with the same passphrase
    /// as always, unchanged by any of this.
    pub async fn restore(&self, location: &Path) -> Result<BookMeta, ApiError> {
        let meta = read_meta(location).await.map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_INPUT",
                "location does not contain a book.json — not a valid backup",
            )
        })?;
        if self.open.read().await.contains_key(&meta.book_id) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "BOOK_STILL_OPEN",
                "book is currently open in this process; close it before restoring over it",
            ));
        }
        for name in [DATA_FILE, KEYSTORE_FILE] {
            if !tokio::fs::try_exists(location.join(name))
                .await
                .unwrap_or(false)
            {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "INVALID_INPUT",
                    format!("location is missing {name} — not a complete backup"),
                ));
            }
        }
        let target = self.book_dir(meta.book_id);
        tokio::fs::create_dir_all(&target)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        for name in [META_FILE, DATA_FILE, KEYSTORE_FILE] {
            tokio::fs::copy(location.join(name), target.join(name))
                .await
                .map_err(|e| ApiError::internal(format!("failed to copy {name}: {e}")))?;
        }
        Ok(meta)
    }
}

/// Runs a mutation against an open book's engine and, only if it appended
/// new events, durably persists the whole log and best-effort commits the
/// backup git repo (Impl Spec §3.1, §3.3). Idempotent replays that append
/// nothing skip the O(N) rewrite entirely.
pub async fn mutate<T>(
    open_book: &OpenBook,
    f: impl FnOnce(&mut AccountingEngine) -> Result<T, EngineError>,
) -> Result<T, ApiError> {
    let mut engine = open_book.engine.write().await;
    let before = engine.audit_log().len();
    let value = f(&mut engine)?;
    let new_ids: Vec<Uuid> = engine.audit_log()[before..]
        .iter()
        .map(|e| e.event_id)
        .collect();
    if !new_ids.is_empty() {
        open_book
            .store
            .persist(engine.audit_log(), &new_ids)
            .await
            .map_err(storage_err)?;
    }
    Ok(value)
}
