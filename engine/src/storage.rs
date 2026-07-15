//! Encrypted single-file storage boundary — Impl Spec §3.
//!
//! `book.data.enc` holds the complete serialized event log (Impl Spec §3.1);
//! there is no separate reference-state blob because [`crate::engine::EngineState::replay`]
//! rebuilds every projection from the log alone (Theorem T1: nothing above
//! this module depends on the storage medium). `book.keystore.json` holds the
//! book key wrapped for the owner's passphrase (Impl Spec §5.4). Every
//! mutation batch rewrites the whole file via atomic replacement (temp +
//! fsync + rename) under a writer lock, then commits the book folder to a
//! local git repository used for backup/point-in-time recovery only —
//! `book.data.enc` remains the sole source of truth (§3.1), so a git failure
//! (e.g. `git` not installed) is logged, not fatal to durability.

use crate::domain::EventRecord;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use async_trait::async_trait;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const DATA_FILE: &str = "book.data.enc";
const KEYSTORE_FILE: &str = "book.keystore.json";
const LOCK_FILE: &str = "book.lock";
const TMP_SUFFIX: &str = ".tmp";
const BOOK_KEY_LEN: usize = 32;
const GCM_NONCE_LEN: usize = 12;

#[derive(Debug)]
pub enum StorageError {
    Io(String),
    /// Wrong passphrase, corrupt ciphertext, or an AEAD auth-tag mismatch.
    Crypto(String),
    /// Malformed JSON, unsupported keystore version/kdf, or a truncated file.
    Corrupt(String),
    /// Another writer currently holds `book.lock`.
    LockHeld,
    Git(String),
    /// The engine rejected an event during replay after load.
    Replay(crate::error::EngineError),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Io(m) => write!(f, "storage I/O error: {m}"),
            StorageError::Crypto(m) => write!(f, "storage crypto error: {m}"),
            StorageError::Corrupt(m) => write!(f, "storage corrupt: {m}"),
            StorageError::LockHeld => write!(f, "writer lock is already held"),
            StorageError::Git(m) => write!(f, "git backup error: {m}"),
            StorageError::Replay(e) => write!(f, "replay error: {e}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> StorageError {
        StorageError::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Hex encoding (avoids pulling in a base64 dependency for a few short fields)
// ---------------------------------------------------------------------------

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Result<Vec<u8>, StorageError> {
    if !s.len().is_multiple_of(2) {
        return Err(StorageError::Corrupt("odd-length hex string".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| StorageError::Corrupt("invalid hex string".into()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Book key wrapping — Impl Spec §5.4
// ---------------------------------------------------------------------------

/// Argon2id cost parameters. `PRODUCTION` follows the OWASP interactive
/// minimum; `TEST_FAST` trades security for speed so tests can open/close a
/// book many times without spending real wall-clock time on key derivation.
#[derive(Debug, Clone, Copy)]
pub struct Argon2Profile {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Argon2Profile {
    pub const PRODUCTION: Argon2Profile = Argon2Profile {
        m_cost_kib: 19_456,
        t_cost: 2,
        p_cost: 1,
    };

    pub const TEST_FAST: Argon2Profile = Argon2Profile {
        m_cost_kib: 8,
        t_cost: 1,
        p_cost: 1,
    };
}

/// `book.keystore.json` — the book key wrapped by a passphrase-derived key.
/// Never contains a plaintext key or passphrase (Impl Spec §5.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystoreFile {
    pub version: u32,
    pub kdf: String,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt_hex: String,
    pub wrap_nonce_hex: String,
    pub wrapped_key_hex: String,
}

/// Impl Spec §5.4: later providers (OS keystore, KMS, HSM) implement this
/// trait without changing the engine or the on-disk format.
pub trait BookKeyProvider: Send + Sync {
    fn wrap(&self, book_key: &[u8; BOOK_KEY_LEN]) -> Result<KeystoreFile, StorageError>;
    fn unwrap(&self, keystore: &KeystoreFile) -> Result<[u8; BOOK_KEY_LEN], StorageError>;
}

/// v1's only provider: Argon2id-derived wrapping key over the owner's
/// passphrase, AES-256-GCM for the wrap itself.
pub struct PassphraseKeyProvider {
    passphrase: String,
    profile: Argon2Profile,
}

impl PassphraseKeyProvider {
    pub fn new(passphrase: impl Into<String>) -> PassphraseKeyProvider {
        PassphraseKeyProvider {
            passphrase: passphrase.into(),
            profile: Argon2Profile::PRODUCTION,
        }
    }

    pub fn with_profile(
        passphrase: impl Into<String>,
        profile: Argon2Profile,
    ) -> PassphraseKeyProvider {
        PassphraseKeyProvider {
            passphrase: passphrase.into(),
            profile,
        }
    }

    fn derive_wrap_key(
        &self,
        salt: &[u8],
        profile: Argon2Profile,
    ) -> Result<[u8; 32], StorageError> {
        derive_key(&self.passphrase, salt, profile)
    }
}

/// Argon2id key derivation shared by book-key wrapping and export-bundle
/// encryption (Impl Plan M9) — same KDF, different purposes.
fn derive_key(
    passphrase: &str,
    salt: &[u8],
    profile: Argon2Profile,
) -> Result<[u8; 32], StorageError> {
    let params = Params::new(profile.m_cost_kib, profile.t_cost, profile.p_cost, Some(32))
        .map_err(|e| StorageError::Crypto(format!("bad argon2 params: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| StorageError::Crypto(format!("argon2 derivation failed: {e}")))?;
    Ok(out)
}

impl BookKeyProvider for PassphraseKeyProvider {
    fn wrap(&self, book_key: &[u8; BOOK_KEY_LEN]) -> Result<KeystoreFile, StorageError> {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let wrap_key = self.derive_wrap_key(&salt, self.profile)?;
        let cipher = Aes256Gcm::new_from_slice(&wrap_key)
            .map_err(|e| StorageError::Crypto(format!("bad wrap key: {e}")))?;
        let mut nonce_bytes = [0u8; GCM_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, book_key.as_slice())
            .map_err(|_| StorageError::Crypto("book key wrap failed".into()))?;
        Ok(KeystoreFile {
            version: 1,
            kdf: "argon2id".into(),
            m_cost_kib: self.profile.m_cost_kib,
            t_cost: self.profile.t_cost,
            p_cost: self.profile.p_cost,
            salt_hex: to_hex(&salt),
            wrap_nonce_hex: to_hex(&nonce_bytes),
            wrapped_key_hex: to_hex(&ciphertext),
        })
    }

    fn unwrap(&self, keystore: &KeystoreFile) -> Result<[u8; BOOK_KEY_LEN], StorageError> {
        if keystore.kdf != "argon2id" {
            return Err(StorageError::Corrupt(format!(
                "unsupported kdf {}",
                keystore.kdf
            )));
        }
        let salt = from_hex(&keystore.salt_hex)?;
        let profile = Argon2Profile {
            m_cost_kib: keystore.m_cost_kib,
            t_cost: keystore.t_cost,
            p_cost: keystore.p_cost,
        };
        let wrap_key = self.derive_wrap_key(&salt, profile)?;
        let cipher = Aes256Gcm::new_from_slice(&wrap_key)
            .map_err(|e| StorageError::Crypto(format!("bad wrap key: {e}")))?;
        let nonce_bytes = from_hex(&keystore.wrap_nonce_hex)?;
        if nonce_bytes.len() != GCM_NONCE_LEN {
            return Err(StorageError::Corrupt("wrap nonce has wrong length".into()));
        }
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = from_hex(&keystore.wrapped_key_hex)?;
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| StorageError::Crypto("wrong passphrase or corrupt keystore".into()))?;
        plaintext
            .try_into()
            .map_err(|_| StorageError::Corrupt("unwrapped key has wrong length".into()))
    }
}

// ---------------------------------------------------------------------------
// Event log encryption
// ---------------------------------------------------------------------------

fn encrypt_events(
    book_key: &[u8; BOOK_KEY_LEN],
    events: &[EventRecord],
) -> Result<Vec<u8>, StorageError> {
    let plaintext = serde_json::to_vec(events).map_err(|e| StorageError::Corrupt(e.to_string()))?;
    let cipher = Aes256Gcm::new_from_slice(book_key)
        .map_err(|e| StorageError::Crypto(format!("bad book key: {e}")))?;
    let mut nonce_bytes = [0u8; GCM_NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_slice())
        .map_err(|_| StorageError::Crypto("book encryption failed".into()))?;
    let mut out = Vec::with_capacity(GCM_NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_events(
    book_key: &[u8; BOOK_KEY_LEN],
    data: &[u8],
) -> Result<Vec<EventRecord>, StorageError> {
    if data.len() < GCM_NONCE_LEN {
        return Err(StorageError::Corrupt("book file too short".into()));
    }
    let (nonce_bytes, ciphertext) = data.split_at(GCM_NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(book_key)
        .map_err(|e| StorageError::Crypto(format!("bad book key: {e}")))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        StorageError::Crypto("book decryption failed (wrong key or corrupt file)".into())
    })?;
    serde_json::from_slice(&plaintext).map_err(|e| StorageError::Corrupt(e.to_string()))
}

// ---------------------------------------------------------------------------
// Export and restore — Impl Spec §7.3/§8.2, Impl Plan M9
// ---------------------------------------------------------------------------

pub const EXPORT_BUNDLE_VERSION: u32 = 1;

/// A portable, encrypted export (Impl Spec §8.2): the whole event log,
/// re-encrypted for a reader passphrase independent of the book's own
/// storage key — leaking a bundle never leaks the operational passphrase,
/// and the export is fully self-describing (own KDF params/salt) so it can
/// be decrypted anywhere, not just against the source book's keystore.
/// `book_id`/`exported_at`/`event_count` are plaintext — the "ledger
/// marker" proving what was captured, inspectable without the passphrase;
/// the payload's own AEAD tag is the integrity guarantee, so no separate
/// checksum is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub version: u32,
    pub book_id: Uuid,
    pub exported_at: crate::types::TimestampMs,
    pub event_count: usize,
    pub kdf: String,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt_hex: String,
    pub payload_hex: String,
}

/// Encrypts `events` for `reader_passphrase` into a self-contained bundle.
/// Read-only over the source book — appends nothing to its log (Impl Spec
/// §8.2: "reconciled before export" is a no-op since the engine never
/// accepts an unbalanced state, so there is nothing to check or mutate
/// here beyond taking a consistent snapshot, which is the caller's job via
/// whatever lock already serializes against concurrent mutation).
pub fn create_export_bundle(
    book_id: Uuid,
    events: &[EventRecord],
    exported_at: crate::types::TimestampMs,
    reader_passphrase: &str,
) -> Result<ExportBundle, StorageError> {
    let profile = Argon2Profile::PRODUCTION;
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let export_key = derive_key(reader_passphrase, &salt, profile)?;
    let payload = encrypt_events(&export_key, events)?;
    Ok(ExportBundle {
        version: EXPORT_BUNDLE_VERSION,
        book_id,
        exported_at,
        event_count: events.len(),
        kdf: "argon2id".into(),
        m_cost_kib: profile.m_cost_kib,
        t_cost: profile.t_cost,
        p_cost: profile.p_cost,
        salt_hex: to_hex(&salt),
        payload_hex: to_hex(&payload),
    })
}

/// Decrypts a bundle back into its event log. A wrong `reader_passphrase`
/// surfaces as `StorageError::Crypto`, exactly like a wrong book
/// passphrase does for `FileBookStore::open`.
pub fn open_export_bundle(
    bundle: &ExportBundle,
    reader_passphrase: &str,
) -> Result<Vec<EventRecord>, StorageError> {
    if bundle.kdf != "argon2id" {
        return Err(StorageError::Corrupt(format!(
            "unsupported export kdf {}",
            bundle.kdf
        )));
    }
    let salt = from_hex(&bundle.salt_hex)?;
    let profile = Argon2Profile {
        m_cost_kib: bundle.m_cost_kib,
        t_cost: bundle.t_cost,
        p_cost: bundle.p_cost,
    };
    let export_key = derive_key(reader_passphrase, &salt, profile)?;
    let payload = from_hex(&bundle.payload_hex)?;
    let events = decrypt_events(&export_key, &payload)?;
    if events.len() != bundle.event_count {
        return Err(StorageError::Corrupt(format!(
            "export bundle claims {} events but decrypted {}",
            bundle.event_count,
            events.len()
        )));
    }
    Ok(events)
}

// ---------------------------------------------------------------------------
// Atomic replacement + writer lock — Impl Spec §3.1
// ---------------------------------------------------------------------------

fn tmp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .expect("data file path always has a file name")
        .to_string_lossy();
    path.with_file_name(format!("{file_name}{TMP_SUFFIX}"))
}

async fn atomic_write(path: &Path, data: &[u8]) -> Result<(), StorageError> {
    let tmp_path = tmp_path_for(path);
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    file.write_all(data).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

/// Held for the duration of a mutation batch's persist; released on drop.
pub struct WriterLockGuard {
    path: PathBuf,
}

impl Drop for WriterLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn acquire_writer_lock(dir: &Path) -> Result<WriterLockGuard, StorageError> {
    let path = dir.join(LOCK_FILE);
    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
    {
        Ok(_) => Ok(WriterLockGuard { path }),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(StorageError::LockHeld),
        Err(e) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// Git backup policy — Impl Spec §3.3 (best-effort: book.data.enc, not git,
// is the sole source of truth per §3.1)
// ---------------------------------------------------------------------------

async fn run_git(dir: &Path, args: &[&str]) -> Result<std::process::Output, StorageError> {
    tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .await
        .map_err(|e| StorageError::Git(format!("failed to run git: {e}")))
}

async fn git_init(dir: &Path) -> Result<(), StorageError> {
    if tokio::fs::try_exists(dir.join(".git"))
        .await
        .unwrap_or(false)
    {
        return Ok(());
    }
    run_git(dir, &["init", "-q"]).await?;
    run_git(dir, &["config", "user.email", "ledgerzero@local"]).await?;
    run_git(dir, &["config", "user.name", "LedgerZero"]).await?;
    Ok(())
}

/// Stages and commits the whole book folder. Returns `Ok(())` even when
/// there is nothing to commit (persisting an unchanged log) or when `git`
/// itself is unavailable — durability already happened in `book.data.enc`;
/// this is backup, not the source of truth (§3.1, §3.3).
async fn git_commit(dir: &Path, message: &str) -> Result<(), StorageError> {
    if run_git(dir, &["add", "-A"]).await.is_err() {
        return Ok(());
    }
    match run_git(dir, &["commit", "-q", "-m", message]).await {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Ok(()),  // most commonly "nothing to commit"; not fatal either way
        Err(_) => Ok(()), // git not installed; backup is best-effort
    }
}

// ---------------------------------------------------------------------------
// Storage trait + file driver — Impl Spec §3.2
// ---------------------------------------------------------------------------

/// The storage boundary the runtime backend uses (Impl Spec §3.2, §3.4).
/// Because the on-disk format is the append-ordered event log alone (§3.1),
/// the trait reduces to load-the-log / persist-the-log; every projection and
/// index is rebuilt above this boundary via [`crate::engine::EngineState::replay`].
#[async_trait]
pub trait BookStorage: Send + Sync {
    async fn load(&self) -> Result<Vec<EventRecord>, StorageError>;

    /// Rewrites the whole encrypted log (O(N) per mutation, per §3.1) and
    /// best-effort commits the book folder to its backup git repo.
    /// `new_event_ids` is used only for the commit message.
    async fn persist(
        &self,
        all_events: &[EventRecord],
        new_event_ids: &[Uuid],
    ) -> Result<(), StorageError>;
}

/// The v1 driver: one encrypted file per book folder (Impl Spec §3.1).
pub struct FileBookStore {
    dir: PathBuf,
    book_key: [u8; BOOK_KEY_LEN],
}

impl FileBookStore {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes a fresh book folder seeded with `events`: random book key
    /// wrapped for `key_provider`, an encrypted event log, and an
    /// initialized backup git repo (idempotent — a no-op if `.git` already
    /// exists, so restoring into a location with prior backup history
    /// keeps it, per §3.3's "backup, not source of truth" framing). Shared
    /// by `create` (refuses an existing book) and `restore` (always
    /// allowed — wipe-and-replace is the point, Impl Spec §8.2).
    async fn write_fresh(
        dir: &Path,
        key_provider: &dyn BookKeyProvider,
        events: &[EventRecord],
        refuse_if_exists: bool,
    ) -> Result<FileBookStore, StorageError> {
        tokio::fs::create_dir_all(dir).await?;
        if refuse_if_exists
            && tokio::fs::try_exists(dir.join(DATA_FILE))
                .await
                .unwrap_or(false)
        {
            return Err(StorageError::Corrupt(
                "a book already exists in this folder".into(),
            ));
        }

        let mut book_key = [0u8; BOOK_KEY_LEN];
        OsRng.fill_bytes(&mut book_key);
        let keystore = key_provider.wrap(&book_key)?;
        let keystore_json = serde_json::to_vec_pretty(&keystore)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        tokio::fs::write(dir.join(KEYSTORE_FILE), keystore_json).await?;

        let store = FileBookStore {
            dir: dir.to_path_buf(),
            book_key,
        };
        let encrypted = encrypt_events(&store.book_key, events)?;
        atomic_write(&store.dir.join(DATA_FILE), &encrypted).await?;
        Ok(store)
    }

    /// Bootstraps a brand-new book folder: random book key wrapped for the
    /// given key provider, an empty encrypted event log, and a fresh backup
    /// git repository with an initial commit.
    pub async fn create(
        dir: &Path,
        key_provider: &dyn BookKeyProvider,
    ) -> Result<FileBookStore, StorageError> {
        let store = Self::write_fresh(dir, key_provider, &[], true).await?;
        git_init(&store.dir).await?;
        git_commit(&store.dir, "book created (0 events)").await?;
        Ok(store)
    }

    /// Wipe-and-replace restore (Impl Spec §8.2, Impl Plan M9): unlike
    /// `create`, always allowed regardless of whether a book already
    /// exists at `dir` — a damaged book location being intentionally
    /// replaced is an explicit supported case, not an error. Seeds the
    /// fresh log with `events` (the export's decrypted log plus the
    /// caller's own `Restored` marker) under a freshly generated book key,
    /// wrapped for `key_provider` — restoring never reuses the exported
    /// book's old key, since the export itself was never re-encrypted with
    /// it (Impl Spec §8.2: "does not transfer the book key between
    /// users").
    pub async fn restore(
        dir: &Path,
        key_provider: &dyn BookKeyProvider,
        events: &[EventRecord],
    ) -> Result<FileBookStore, StorageError> {
        let store = Self::write_fresh(dir, key_provider, events, false).await?;
        git_init(&store.dir).await?;
        git_commit(
            &store.dir,
            &format!("book restored ({} events)", events.len()),
        )
        .await?;
        Ok(store)
    }

    /// Opens an existing book folder: unwraps the book key (wrong
    /// passphrase surfaces as `StorageError::Crypto`) and decrypts the event
    /// log. Does not replay — callers build `EngineState` via
    /// `EngineState::replay(book_id, &events)` from the returned events.
    pub async fn open(
        dir: &Path,
        key_provider: &dyn BookKeyProvider,
    ) -> Result<(FileBookStore, Vec<EventRecord>), StorageError> {
        let keystore_bytes = tokio::fs::read(dir.join(KEYSTORE_FILE)).await?;
        let keystore: KeystoreFile = serde_json::from_slice(&keystore_bytes)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        let book_key = key_provider.unwrap(&keystore)?;
        let store = FileBookStore {
            dir: dir.to_path_buf(),
            book_key,
        };
        let events = store.load().await?;
        Ok((store, events))
    }
}

#[async_trait]
impl BookStorage for FileBookStore {
    async fn load(&self) -> Result<Vec<EventRecord>, StorageError> {
        let data = tokio::fs::read(self.dir.join(DATA_FILE)).await?;
        decrypt_events(&self.book_key, &data)
    }

    async fn persist(
        &self,
        all_events: &[EventRecord],
        new_event_ids: &[Uuid],
    ) -> Result<(), StorageError> {
        let _lock = acquire_writer_lock(&self.dir).await?;
        let encrypted = encrypt_events(&self.book_key, all_events)?;
        atomic_write(&self.dir.join(DATA_FILE), &encrypted).await?;
        let message = if new_event_ids.is_empty() {
            format!("mutation batch ({} events total)", all_events.len())
        } else {
            let ids: Vec<String> = new_event_ids.iter().map(Uuid::to_string).collect();
            format!("mutation batch: {}", ids.join(", "))
        };
        git_commit(&self.dir, &message).await?;
        Ok(())
    }
}
