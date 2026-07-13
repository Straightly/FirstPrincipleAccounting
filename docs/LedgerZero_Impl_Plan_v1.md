# LedgerZero Implementation Plan v1

Milestone plan for building `LedgerZero_Impl_Spec_v1.md`. The plan starts with a **walking skeleton**: authentication and authorization across all architectural components first (M1), since creating even the first book requires authN/Z in place. The accounting core then fills in behind that skeleton. Each milestone ends with passing tests and something demonstrable. Do not start a milestone until the previous one's exit criteria pass. Spec section references are to the Impl Spec.

Working rules for each milestone:

- One milestone = one or a few focused sessions; each ends in a commit on a branch merged to main.
- Tests written inside the milestone, not deferred (spec §7.5).
- If implementation reveals a spec problem, record the resolution in the Impl Spec (and Appendix A) before coding around it.

## M0 — Repository scaffold ✅ DONE (2026-07-11)

- [x] Create Rust workspace: `engine/` and `backend/` crates (spec §7.2)
- [x] Scaffold `frontend/` React + Vite launcher (empty shell: builds and serves a page)
- [x] Restructure `mcp_server/`: remove `engine/` and `storage/` modules; keep MCP + dev-time backend skeleton
- [x] Add `server.config.example.toml` (OAuth config, books dir, listen address, bootstrap_owner_email)
- [x] Wire `cargo test`, `cargo clippy`, frontend build, and Python tests into a single check script / CI (`scripts/check.sh`)

**Exit criteria:** clean clone builds everything; all (empty) test suites pass. **Met:** `./scripts/check.sh` passes on the user's machine.

**Delivered beyond plan:** `scripts/package.sh` builds a self-contained release tarball (binary + frontend + example config + deploy doc), and `docs/LedgerZero_Run_and_Deploy.md` documents local run, deployment rehearsal, monitoring, and remote deployment — both pulled forward from M11, which now only needs to extend them.

## M1 — Walking skeleton: authentication & authorization ✅ DONE (2026-07-11)

All architectural components stand up here, doing the minimum real work: routing, login, sessions, and authority checks — no accounting yet.

- [x] Axum server as the routing server: serves launcher assets, routes API requests (spec §7.1)
- [x] Google OAuth login flow; user records; AKA mapping `(provider, subject) → user_id` (spec §2.9, §5.2)
- [x] Session tokens (1h) with refresh rotation; every API call carries a verified identity claim
- [x] Bootstrap authorization from `server.config.toml`: on a fresh install, only `bootstrap_owner_email` passes owner-gated endpoints (spec §5.3)
- [x] Authorization framework: role store lookup + structured UNAUTHORIZED_* errors (spec §4.4); workflow-scoped enforcement completes in M6 when deployments exist
- [x] Operational audit log for failed authentications/authorizations, separate from the ledger (spec §5.2)
- [x] Launcher: Google login, session handling, and a protected "who am I / what may I do" page proving the full path browser → routing → backend
- [x] Integration tests: unauthenticated rejected, authenticated non-owner rejected at owner-gated endpoints, bootstrap owner accepted, token expiry/refresh

**Exit criteria:** in a browser: login with Google, see identity and authority; owner-gated test endpoint accepts only the bootstrap owner. All components (backend, frontend, config) participating. **Met:** verified by the user 2026-07-11 with real Google OAuth credentials against a locally deployed release package.

**Delivered beyond plan:**

- Pluggable authentication: `IdentityProvider` interface with data-driven `OidcProvider` and a runtime-mutable `ProviderRegistry` — new OIDC domains (Microsoft, enterprise IdP) are config records addable while the server runs, no code change or restart (Theorems T2/T3 in `LedgerZero_Theorems.md`, which was also created here with verifying tests). This was planned as bare "Google OAuth"; the generalization landed early.
- `dev_login` provider for credential-free local development (spec §5.2; must be disabled on any non-local deployment).
- `GET /api/health` liveness endpoint for basic monitoring (spec §7.1).

**Known M1 limitations (by design, resolved in later milestones):** users and sessions are in-memory — a restart logs everyone out; single instance only until M3 storage. No request logging/metrics/tracing until M11.

## M2 — Domain model and engine core (in-memory) — implemented 2026-07-11, exit gate: local `./scripts/check.sh`

- [x] Domain types: ResourceType, Chart, Account (derived normal_balance), JournalEntry, JournalLine, Period, price facts, event envelope (spec §2) — `engine/src/domain.rs`
- [x] Structured error catalog completed (spec §4.4) — `engine/src/error.rs`; all codes defined incl. the UNAUTHORIZED_*/BOOK_NOT_OPEN/INVALID_EXECUTION_CONTEXT codes raised in later milestones
- [x] Invariant checks on posting: exact balance incl. cross-unit via entry-recorded prices, account validity/active, single chart+entity, open period, unit coverage, account validation rules (spec §4.1) — `engine/src/engine.rs`
- [x] Posted-or-rejected semantics; reversal entries; expanded-equation balance check A = L + E + (R − X) — `equation_check` evaluates the equation per resource type, with cross-unit entries accounted for at their recorded prices
- [x] Balance computation and price projection from the event log (spec §2.6, §4.3)
- [x] Property tests: any generated entry either posts balanced or is rejected; no operation sequence unbalances a chart — `engine/tests/property_replay.rs` (deterministic dependency-free PRNG harness; 3 seeds × 300 ops)
- [x] Replay tests: projections rebuilt from event log equal incrementally maintained state — every mutation and `replay` share one `EngineState::apply` transition; tests compare full state plus an independent balance fold over the log

**Exit criteria:** property and replay tests pass; engine usable as a pure library with an in-memory store. **Gate:** the sandbox has no Rust toolchain — criteria are ticked as done once `./scripts/check.sh` passes on the user's machine.

**Notes:**

- Money is exact fixed-point `Decimal(18,8)` over i128 (`engine/src/amount.rs`) — no floats anywhere; cross-unit balance uses exact rational arithmetic at the entry's own recorded prices, no tolerance.
- Engine-level idempotency covers every mutation (client-generated UUID → identical replay returns the original outcome, §4.1.6), ahead of the M3 storage-boundary and M4 API idempotency work.
- v1 account-validation-rule vocabulary: `require_memo`, `max_amount`, `side` (debit_only/credit_only); unknown keys rejected at account creation.
- No new dependencies: engine remains serde + serde_json + uuid only.

## M3 — Encrypted single-file storage ✅ DONE (2026-07-11), verified via local `./scripts/check.sh`

- [x] Storage interface (async trait) per spec §3.2 — `engine/src/storage.rs::BookStorage { load, persist }`. Reduced from the original method list to load/persist-the-whole-log because §3.1 already states the log alone is the sole source of truth and every projection/index is rebuilt from it at load — there is no separate reference-state blob to serialize, so the M3 checklist's "reference state" bullet collapses into this one.
- [x] `book.data.enc`: AES-256-GCM over the JSON-serialized `Vec<EventRecord>`, random 96-bit nonce prefixed to the ciphertext; `book.keystore.json`: Argon2id-derived wrapping key (OWASP interactive-minimum params in production, salt/params recorded in the keystore) wraps a random 256-bit book key via AES-256-GCM; `BookKeyProvider` trait (`wrap`/`unwrap`) with `PassphraseKeyProvider` as the only v1 implementation, so OS keystore/KMS/HSM providers can be added later without touching the engine or format (spec §5.4)
- [x] Atomic file replacement (write `book.data.enc.tmp`, `sync_all`, `rename`) and a file-based writer lock (`book.lock`, create-new-or-fail) held for the duration of `persist` (spec §3.1)
- [x] Load: decrypt `book.data.enc`, then `EngineState::replay` rebuilds every projection and the idempotency index from the log alone — no separate load-time index construction needed
- [x] Idempotency at storage boundary: proven by `idempotent_replay_and_conflict_survive_reload` — after persist+reopen, replaying an entry's client id with the identical payload returns the original outcome and appends nothing; a mutated payload under the same id returns `IDEMPOTENCY_CONFLICT`
- [x] Git commit after successful mutation batch (spec §3.3) — `git init` (+ local `user.name`/`user.email`) on `create`, `git add -A && git commit` after every `persist`, commit message lists the batch's event ids. Treated as best-effort/non-fatal to `persist`: `book.data.enc` is the sole source of truth (§3.1) and git is explicitly backup/point-in-time-recovery only (§3.3), so a missing `git` binary or a no-op commit does not fail the mutation. Verified in `round_trip_create_persist_reopen_replay_matches` (asserts `.git` exists and the commit count).
- [x] Crash-safety tests (kill during replace → pre-mutation state on reload); round-trip and wrong-passphrase tests — `engine/tests/storage_crash_safety.rs`: `crash_during_atomic_replace_preserves_pre_mutation_state` simulates the kill by writing a corrupt `book.data.enc.tmp` directly (exactly the state left by a process death after temp-file-open but before fsync+rename) and proves a fresh load ignores it and returns the pre-crash log untouched; `round_trip_create_persist_reopen_replay_matches`; `wrong_passphrase_is_rejected`; plus `property_lite_persist_reopen_cycles`, a cut-down version of the M2 PRNG harness that interleaves persist/reopen every 15 ops over 4 cycles and re-checks replay equivalence and the chart equation each time — proving the M2 invariants hold through the file driver, not just in memory.

**Exit criteria:** create → mutate → kill → reload cycles prove durability; all M2 tests pass against the file driver. **Met:** 5 tests in `engine/tests/storage_crash_safety.rs`, all passing locally alongside the existing M2 suites.

**Notes:**

- New engine-crate dependencies (unavoidable for real AEAD/KDF primitives — no hand-rolled crypto): `aes-gcm`, `argon2`, `rand`, plus `async-trait` and `tokio` (fs/process/io-util) for the async storage boundary. `tempfile` is a dev-dependency for the tests.
- The trait is not yet wired into the backend/API layer — `create_accounting_book`/`open_book` and holding an open `AccountingEngine` behind the auth boundary are M4 work.

## M4 — Book lifecycle and core accounting APIs ✅ DONE (2026-07-13), verified via local `./scripts/check.sh` and a curl-driven golden path against a running server

Now the skeleton gets its accounting organs: everything behind the M1 auth boundary.

- [x] `create_accounting_book` (bootstrap-owner-gated) and `open_book` (owner passphrase → key into memory) (spec §5.3, §5.4) — `backend/src/books.rs::BooksRegistry`. `book.json` is plaintext registry metadata (book_id, name, owner_email, created_at) written beside the engine's `book.data.enc`/`book.keystore.json`; it is not accounting data so it does not go through the encrypted boundary. Creating a book also opens it; `open_book` is idempotent (returns immediately if already open) and only touches the passphrase/disk path for a book not yet in memory.
- [x] Reference APIs: `create_entity`, `create_resource_type`, `create_chart`, `copy_chart`, `create_account`, `update_account_metadata`, `deactivate_account` (`PUT .../accounts/:id/active`, covers both directions), `list_accounts`, `create_period`, `close_period`, `reopen_period` — `backend/src/books_api.rs`, routed in `backend/src/app.rs`. `copy_chart` and the `list_*` read methods (`list_entities`/`list_resource_types`/`list_charts`/`list_accounts`/`list_periods`/`list_prices`) did not exist in the M2 engine and were added to `engine/src/engine.rs` for this milestone: `copy_chart` duplicates a source chart's accounts under a new chart in the same entity, remapping `parent_account_id` in dependency order, as one idempotency-tracked mutation batch (one `ChartCreated` event keyed by the client `op_id`, followed by one `AccountCreated` event per copied account — the first engine mutation to emit more than one event).
- [x] Ledger APIs: `post_entry`, `reverse_entry`, `get_balance`, `list_entries`, `get_audit_log`, `record_price`, `list_prices`
- [x] Client-generated UUID idempotency on every mutation; structured errors over HTTP — every mutating request body carries `op_id` (or, for `post_entry`/`reverse_entry`, reuses `entry_id`/`new_entry_id` exactly as the engine itself does); `books.rs::mutate()` persists only when the call actually appended new events, so a pure idempotent replay skips the O(N) log rewrite. `EngineError -> ApiError` maps the §4.4 catalog onto HTTP status (400 invalid input, 409 idempotency conflict / book not open, 403 unauthorized, 422 the rest); `StorageError::Crypto` (wrong passphrase) maps to 401.
- [x] API integration tests: full book lifecycle via HTTP incl. auth failures, idempotent replay, and conflict cases — `backend/tests/books_flow.rs` (7 tests): golden-path lifecycle (create book → entity → resource type → chart → two accounts → period → post a balanced entry → read its balance → list entries → audit log); unauthenticated and non-owner rejection; `BOOK_NOT_OPEN` on an unopened book_id; wrong-passphrase rejection simulated by opening a second server instance against the same `books_dir` (a fresh in-memory registry, book already exists on disk — the realistic "server restarted" case); idempotent replay and `IDEMPOTENCY_CONFLICT` over HTTP; `copy_chart` over HTTP. Plus `engine/tests/copy_chart_and_lists.rs` (5 tests) for the new engine methods directly, including replay equivalence of the new multi-event mutation.

**Exit criteria:** with curl/httpie plus a browser login: create book, open book, set up chart/accounts/period, post entries, read balances — every call authenticated and authorized. **Met:** the full sequence above was driven against a real running `ledgerzero-backend` process with `curl` (dev-login, create book, entity, USD resource type, chart, Cash/Capital accounts, period, a balanced $1000 opening entry, balance read, audit-log length); the book folder on disk showed the expected `book.data.enc`/`book.keystore.json`/`book.json` plus 8 real git commits (create + one per mutation batch). Browser/real-OAuth walkthrough of this same flow is available to the user the same way M1's was (dev-login also works from the launcher UI), not re-run here since the API contract is what M4 adds.

**Notes:**

- Authorization for every reference/ledger endpoint in this milestone is a blanket bootstrap-owner check (`Action::BookApi`) — v1 has no role system yet (that's M6). This is a known, documented gap, not an oversight: workflow-scoped, per-book authorization replaces it once roles and workflow deployments exist.
- `list_books`/`create_book` are similarly owner-gated for now (`Action::ListBooks`/`Action::CreateAccountingBook`), consistent with "one owner at bootstrap" (spec §2.8); `BookMeta.owner_email` is recorded but not yet enforced per-book since there is only one authority in v1.
- `copy_chart` copies within the same entity only (spec's contradiction-resolution notes for sub-books don't specify cross-entity chart copying); balances and transaction history are never copied, only chart/account structure.
- No new dependencies beyond M3's; `backend/Cargo.toml` gained `tempfile` as a dev-dependency for the new HTTP tests.

**Exit criteria:** with curl/httpie plus a browser login: create book, open book, set up chart/accounts/period, post entries, read balances — every call authenticated and authorized.

## M5 — First hand-built workflow

- [ ] Launcher workflow menu navigating to workflow routes (spec §7.1)
- [ ] Backend serves workflow artifacts from the artifact path
- [ ] Hand-write (no AI) `Recording startup expense` as a standalone React app artifact — the reference artifact proving the contract: self-contained bundle, session-cookie auth, execution context (`book_id`, `entity_id`, `workflow_id`, `workflow_deployment_id`, `workflow_execution_id`) on every call
- [ ] Manual-path authorization for this milestone (workflow machinery arrives in M6)

**Exit criteria:** end-to-end in a browser: login → open book → run workflow → entry posted → balance visible.

## M6 — Workflow deployment and authorization machinery

- [ ] WorkflowDefinition contract, immutable deployment records, WORKFLOW_DEPLOYMENT events (spec §2.9)
- [ ] Dev artifact store layout + hash verification (spec §7.4)
- [ ] Auto-role on deployment; `create_role`, `assign_workflow_to_role`, `assign_role_to_user` as ROLE_ASSIGNMENT events
- [ ] Complete the M1 authorization framework: workflow-scoped API checks (`backend_api_calls`), execution-context verification (spec §6.5)
- [ ] Deploy the M5 workflow through this machinery (retire the manual path)
- [ ] Tests: unauthorized workflow/API/context combinations rejected

**Exit criteria:** M5 workflow runs only via a valid deployment and role assignment; authorization tests pass.

## M7 — AI generation path (MCP + Python dev-time backend)

- [ ] MCP primitives: `generate_workflow_definition`, `deploy_workflow_definition`, `list_workflows`, `get_workflow_definition` + admin primitives from spec §6.4
- [ ] Python dev-time backend: LLM wrapping, prompt/context assembly, artifact preparation — no storage credentials, no persisted private context (Axiom 12)
- [ ] Generate and deploy `Recording bank account transactions manually` (spec §6.6) via the full path: MCP → dev backend → artifact → deployment
- [ ] Verify generated app passes the same e2e and authorization tests as hand-built artifacts

**Exit criteria:** a workflow authored by natural language runs in the browser with no hand-edits, or fails with an explicit missing-primitive answer.

## M8 — Periods in practice and reconciliation

- [ ] `Reconcile bank accounts at EOP` workflow: compare projection vs expected balance, report discrepancies, corrections as new entries, result as administrative event (spec §6.6)
- [ ] Period close/reopen exercised through workflows; closed-period posting rejected end-to-end
- [ ] `explain_reconciliation_issue` MCP primitive over runtime facts

**Exit criteria:** full monthly cycle: post, reconcile, close, attempt late post (rejected), reopen, correct, re-close.

## M9 — Export and restore

- [ ] `export_book`: encrypted JSON bundle, reader-passphrase encryption, deployment references + hashes, snapshot cut under writer lock + ledger marker (spec §7.3, §4.3)
- [ ] `restore_book`: wipe-and-replace, IDs and `book_id` preserved, RESTORE event, unavailable-workflow marking when artifacts don't match by id+hash
- [ ] Round-trip test: export → restore to fresh location → continue posting

**Exit criteria:** a book moves to a new folder/deployment and keeps operating.

## M10 — Sub-books and consolidation

- [ ] `create_sub_book` with owner choice and copy mode (all / none / owner-only; different owner → none) (spec §2.8)
- [ ] SUB_BOOK_LINK events in both books; in-file link and rule projections
- [ ] `define_consolidation_rule`, `list_consolidation_rules`, `run_consolidation`: derived parent entries, idempotent, pending when unmapped or unauthorized
- [ ] Tests: default consolidation, summary-account mapping, pending-until-authorized, idempotent re-run

**Exit criteria:** parent book consolidates a child book on one deployment; re-runs create no duplicates.

## M11 — Hardening and deployment

Partially pre-done during M0/M1: `scripts/package.sh` (release tarball) and `docs/LedgerZero_Run_and_Deploy.md` (local run, deployment rehearsal, monitoring signals, remote-deployment caveats) already exist; this milestone extends them.

- [ ] Oracle Cloud VM deployment (systemd or equivalent) and on-premise instructions
- [ ] MFA guidance, ingress restrictions for non-local deployments (spec §5.5)
- [ ] Operational docs: bootstrap, open-book, backup/push, ownership transfer (incl. git-history caveat), restore runbook
- [ ] Full test-suite pass + a scripted demo covering M4–M10 flows

**Exit criteria:** a fresh operator can install, bootstrap, and run the demo from docs alone.

## Deferred (tracked, not scheduled)

Cross-server consolidation auth; FX translation between books; consolidation scheduling beyond on-demand; year-end close workflow; reporting tools; re-open-by-branching; brokerage import; containerization; SQLite/Postgres drivers; identity-merge workflow (Theorem T4) and runtime provider-administration workflow (Theorem T3) once workflow machinery exists (M6+).

Standing architectural guarantees are tracked in `LedgerZero_Theorems.md`; every milestone must preserve them.
