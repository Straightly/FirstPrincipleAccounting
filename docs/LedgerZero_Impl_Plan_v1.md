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

**Delivered beyond plan:** `scripts/package.sh` builds a self-contained release tarball (binary + frontend + example config + deploy doc), and `docs/LedgerZero_Run_and_Deploy.md` documents local run, deployment rehearsal, monitoring, and remote deployment — both pulled forward from M12, which now only needs to extend them.

## M1 — Walking skeleton: authentication & authorization ✅ DONE (2026-07-11)

All architectural components stand up here, doing the minimum real work: routing, login, sessions, and authority checks — no accounting yet.

- [x] Axum server as the routing server: serves launcher assets, routes API requests (spec §7.1)
- [x] Google OAuth login flow; user records; AKA mapping `(provider, subject) → user_id` (spec §2.9, §5.2)
- [x] Session tokens (1h) with refresh rotation; every API call carries a verified identity claim
- [x] Bootstrap authorization from `server.config.toml`: on a fresh install, only `bootstrap_owner_email` passes owner-gated endpoints (spec §5.3)
- [x] Authorization framework: role store lookup + structured UNAUTHORIZED_* errors (spec §4.4); workflow-scoped enforcement completes in M5 when deployments exist
- [x] Operational audit log for failed authentications/authorizations, separate from the ledger (spec §5.2)
- [x] Launcher: Google login, session handling, and a protected "who am I / what may I do" page proving the full path browser → routing → backend
- [x] Integration tests: unauthenticated rejected, authenticated non-owner rejected at owner-gated endpoints, bootstrap owner accepted, token expiry/refresh

**Exit criteria:** in a browser: login with Google, see identity and authority; owner-gated test endpoint accepts only the bootstrap owner. All components (backend, frontend, config) participating. **Met:** verified by the user 2026-07-11 with real Google OAuth credentials against a locally deployed release package.

**Delivered beyond plan:**

- Pluggable authentication: `IdentityProvider` interface with data-driven `OidcProvider` and a runtime-mutable `ProviderRegistry` — new OIDC domains (Microsoft, enterprise IdP) are config records addable while the server runs, no code change or restart (Theorems T2/T3 in `LedgerZero_Theorems.md`, which was also created here with verifying tests). This was planned as bare "Google OAuth"; the generalization landed early.
- `dev_login` provider for credential-free local development (spec §5.2; must be disabled on any non-local deployment).
- `GET /api/health` liveness endpoint for basic monitoring (spec §7.1).

**Known M1 limitations (by design, resolved in later milestones):** users and sessions are in-memory — a restart logs everyone out; single instance only until M3 storage. No request logging/metrics/tracing until M12.

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

- Authorization for every reference/ledger endpoint in this milestone is a blanket bootstrap-owner check (`Action::BookApi`) — v1 has no role system yet (that's M5). This is a known, documented gap, not an oversight: workflow-scoped, per-book authorization replaces it once roles and workflow deployments exist.
- `list_books`/`create_book` are similarly owner-gated for now (`Action::ListBooks`/`Action::CreateAccountingBook`), consistent with "one owner at bootstrap" (spec §2.8); `BookMeta.owner_email` is recorded but not yet enforced per-book since there is only one authority in v1.
- `copy_chart` copies within the same entity only (spec's contradiction-resolution notes for sub-books don't specify cross-entity chart copying); balances and transaction history are never copied, only chart/account structure.
- No new dependencies beyond M3's; `backend/Cargo.toml` gained `tempfile` as a dev-dependency for the new HTTP tests.

## M5 — First hand-built workflow, with deployment and authorization machinery ✅ DONE (2026-07-14), verified via local `./scripts/check.sh` and a real click-through in the browser (positive and negative cases)

Combined with what was originally two milestones (a hand-built workflow, then separately its deployment/authorization machinery) at the user's request: the old split meant the workflow would briefly run under stopgap "manual-path authorization" before real workflow-scoped auth replaced it. Building both together means there is never an interim state with weaker authorization than the finished design, and the very first workflow is verifiable end-to-end against real role/deployment checks from the start.

- [x] WorkflowDefinition contract, immutable deployment records, WORKFLOW_DEPLOYMENT events (spec §2.9) — `engine/src/domain.rs::WorkflowDefinition`, `EventPayload::WorkflowDeployed`. `workflow_deployment_id` *and* `workflow_id` are both caller-supplied (not engine-generated): the artifact itself must embed its own `workflow_id` in its JS before it is ever deployed (to build `WorkflowContext` on its calls), so the engine cannot be the one to generate it — the same reasoning that already made `workflow_deployment_id` caller-supplied for the dev-artifact-path reason. `workflow_id` is meant to stay stable across a future redeployment under the same name (M8+); v1 has no redeploy path, so it is simply recorded as given, with a uniqueness check.
- [x] Dev artifact store layout + hash verification (spec §7.4) — `backend/src/dev_artifacts.rs`: layout `dev_artifacts/workflows/<workflow_deployment_id>/{workflow.json, manifest.json, code/, signatures/}`; SHA-256 over `manifest.json` and over every file directly under `code/` (v1 artifacts are flat — no nested asset folders), covering file names too so a rename changes the hash even with identical bytes. Hashes are computed fresh from disk at deploy time (the identity authority per spec) rather than trusted from a self-declared value in the manifest.
- [x] Auto-role on deployment; `create_role`, `assign_workflow_to_role`, `assign_role_to_user` as ROLE_ASSIGNMENT events — `engine/src/engine.rs`. `deploy_workflow` is the first engine mutation besides `copy_chart` to emit more than one event in a batch: `WorkflowDeployed` (idempotency-tracked under `workflow_deployment_id`) then `RoleCreated` for the auto-role (fresh internal event id), which starts out containing exactly that one workflow.
- [x] Complete the M1 authorization framework: workflow-scoped API checks (`backend_api_calls`), execution-context verification (spec §6.5) — `AccountingEngine::authorize_workflow_api` (private, wired into `validate_entry` ahead of the structural/domain invariants, same precedent as idempotency being checked by the caller first): deployment must exist and match the claimed `entity_id`/`workflow_id` (else `INVALID_EXECUTION_CONTEXT`), the requested API must be in `backend_api_calls` (else `UNAUTHORIZED_API`), and the user must hold a role granting the workflow (else `UNAUTHORIZED_WORKFLOW`) — the three error codes M2 had already reserved in the catalog for "later milestones." `post_entry` is the only endpoint wired to accept workflow context in v1 (the one API the sample workflow needs); `NewEntry.workflow: Option<WorkflowContext>` and `JournalEntry.workflow` now actually thread through instead of being hardcoded `None`.
- [x] Launcher workflow menu navigating to workflow routes (spec §7.1) — `frontend/src/App.jsx`: a book_id/entity_id input (no book-browser UI yet — M4 added the book APIs but not a picker screen; intentionally simple per spec §8.3) plus "Show my workflows", calling the new `GET .../workflows/mine` and linking to each result's `frontend_route` with `book_id`/`entity_id` appended as query params.
- [x] Backend serves workflow artifacts from the artifact path — `backend/src/app.rs` nests a `ServeDir` at `/workflows` over `<dev_artifacts_dir>/workflows`; static assets only, no auth at the fetch — authorization happens at the backend API calls the artifact makes, per spec.
- [x] Hand-write (no AI) `Recording startup expense` as a standalone React app artifact — `dev_artifacts/workflows/2ef2f432-a548-4f24-87a2-8521bde76af8/`: `index.html` + hand-written `app.js` (plain `React.createElement`, no JSX/build step) + vendored `react.production.min.js`/`react-dom.production.min.js` copied from the launcher's own installed React (genuinely self-contained — no CDN, no shared JS with the launcher or any other workflow). Collects expense date/description/amount/expense-or-asset-account/source-account/memo per spec §7.5.1; `workflow_id`/`workflow_deployment_id` are baked-in JS constants (artifact-intrinsic, chosen before deployment); `book_id`/`entity_id` are read from the URL query string instead (deployment-time/launch-time context, not intrinsic to the code). Does a client-side pre-flight authorization check against `workflows/mine` for UX only — the backend re-verifies unconditionally regardless of what this check shows.
- [x] Deploy the artifact through the new machinery — no manual-path stopgap; it only ever runs via a valid deployment and role assignment — `POST .../workflows/deploy` (bootstrap-owner-gated: the developer is the sole deploy authority in v1).
- [x] Tests: unauthorized workflow/API/context combinations rejected — `engine/tests/workflows_and_roles.rs` (8 tests: auto-role creation, idempotency/conflict, name-collision rejection, role/workflow/user assignment round-trip, `UNAUTHORIZED_WORKFLOW`, `UNAUTHORIZED_API`, `INVALID_EXECUTION_CONTEXT` incl. nil execution id, and a full authorized-call replay-equivalence check) and `backend/tests/workflows_flow.rs` (5 tests, same matrix driven over HTTP, incl. deploy requiring bootstrap owner and failing when the artifact is missing from disk).

**Exit criteria:** end-to-end in a browser: login → open book → run the workflow → entry posted → balance visible; the workflow runs only via a valid deployment and role assignment; authorization tests pass. **Met:** beyond the 13 automated tests, manually drove the entire path in a real browser against a running server — logged in as an employee (dev-login), loaded the workflow menu, opened the hand-written artifact, filled and submitted the form, and confirmed a real balanced entry posted (verified via `get_balance` and `list_entries`, `workflow` context intact on the posted entry). Also drove the negative case in the browser: the book *owner* — who is not assigned this workflow's role — got the client-side "not authorized" warning, and submitting anyway produced a genuine server-side `403 UNAUTHORIZED_WORKFLOW`, proving authorization is truly workflow-scoped and not a blanket owner bypass.

**Notes:**

- Hit and fixed one real bug during manual verification: the deployed `frontend_route` initially pointed at `/workflows/<id>/index.html`, but the artifact's files live under `code/` inside the dev-artifact folder — `ServeDir` 404'd because the constructed route skipped that path segment. Fixed to `/workflows/<id>/code/index.html`. A reminder that `./scripts/check.sh` and unit/integration tests don't exercise real static-file serving end-to-end — this class of bug only surfaces by actually running the server and clicking through it, which is why that step is not optional for UI-touching milestones.
- `copy_chart` (M4) was the first multi-event engine mutation; `deploy_workflow` is the second, establishing the pattern (one idempotency-tracked primary event keyed by the caller-supplied id, followed by fresh-id secondary events) as reusable rather than one-off.

## M6 — Book and entity picker (launcher) ✅ DONE (2026-07-14), verified via local `./scripts/check.sh` and a real click-through in the browser (owner and non-owner viewpoints)

M5's workflow menu required pasting raw `book_id`/`entity_id` UUIDs to reach `workflows/mine` — enough to prove workflow-scoped authorization worked, but not something a real non-owner user could do unprompted, and flagged at the time as a known gap rather than an oversight. This milestone closes it with a real bootstrapped picker: the same kind of launcher-native capability as `Open book`/`Adding a workflow` (spec §6.6/§7.1), not a deployed `WorkflowDefinition` artifact.

- [x] Engine: `entities_with_workflows_for_user(user_id) -> Vec<Uuid>` — entities in this book where the user holds at least one workflow-granting role (spec §6.5) — `engine/src/engine.rs`, distinct from `workflows_authorized_for_user` (which needs an entity already picked); this is how the user discovers *which* entity to look in.
- [x] Backend: `GET /api/books/mine` (`list_my_books`) — the bootstrap owner sees every book, exactly as `list_books` already does; any other signed-in user sees only *currently open* books where `entities_with_workflows_for_user` is non-empty for at least one entity — `backend/src/books_api.rs::list_my_books`, backed by a new `BooksRegistry::list_open()`. No `Action::BookApi` gate: the engine's own role assignments are the authority.
- [x] Backend: `GET /api/books/:book_id/entities/mine` (`list_my_entities`) — the owner sees every entity in the book, exactly as `list_entities` already does; any other user sees only entities from `entities_with_workflows_for_user` — `backend/src/books_api.rs::list_my_entities`.
- [x] Launcher: replace the `book_id`/`entity_id` text inputs with cascading pickers (book → entity → workflow) driven by the above, showing names instead of raw UUIDs — `frontend/src/App.jsx`: three `useEffect`s cascade book selection → `entities/mine` → entity selection → `workflows/mine`, rendered as `<select>` dropdowns.
- [x] Tests: owner sees all books/entities; a role-assigned non-owner sees only their own; a signed-in user with no role assignments anywhere sees an empty picker, not an error — `engine/tests/workflows_and_roles.rs::entities_with_workflows_for_user_backs_the_picker` (incl. a role with zero workflows correctly not surfacing its entity) and `backend/tests/workflows_flow.rs::book_and_entity_picker_scopes_by_role_assignment` (owner/employee/stranger three-way comparison over HTTP) plus an unauthenticated-rejection test.

**Exit criteria:** in a browser, starting from nothing but sign-in, a non-owner user with a role assignment can find and run their workflow without ever typing or pasting a `book_id` or `entity_id`. **Met:** manually verified with two books (one the employee has a role in, one they don't) and two users — logged in as the employee, the book dropdown showed only the one book they're assigned to (the unrelated book never appeared), selecting it populated the entity dropdown with only their entity, and selecting that revealed the "Recording startup expense" link with the correct `book_id`/`entity_id` baked into its `href`; logged in as the owner in the same session, the book dropdown correctly expanded to show both books.

**Notes:**

- Discovery for non-owner users is scoped to *currently open* books only, not every book folder on disk — a book a non-owner is assigned into is only reachable once its owner has opened it (matches the `open_book` model already established in M4; there is no lookup path that would let a non-owner discover an unopened book).
- No new dependencies; purely additive to M4/M5's existing engine and backend surfaces.
- **Superseded by M7**: the entity-picker step this milestone added (the second dropdown, `list_my_entities`) was removed one milestone later once every book was constrained to exactly one entity — selecting a book already determines the entity, so there is nothing left to pick. The book-picker step and its tests below are unaffected and remain exactly as delivered.

## M7 — One entity per book (security-boundary correction)

Raised by the user while reviewing M6: a book's encryption key and owner authority could, as speced, span several *legally distinct* entities sharing one book. That's a real security-boundary crossing — a key compromise or an owner's blanket authority would reach every entity in the book — not just an implementation nicety, even though the accounting data was already logically partitioned by `entity_id`. Assessed and agreed: the multi-entity-per-book case was never actually the right mechanism for related legal entities (holding company + subsidiaries, one bookkeeper's several clients) — `create_sub_book` plus read-only, idempotent consolidation already existed for exactly that, with proper key/owner separation. Removing multi-entity-per-book removes a second, weaker path to the same outcome, not a capability. Recorded as resolution R1 in the Impl Spec's Appendix A.

- [x] Engine: `create_entity` rejects a second entity in the same book (`INVALID_INPUT`) — the structural enforcement, not just an API-surface removal, is what makes this a real fix rather than hiding the old capability behind a missing button.
- [x] Backend: `BooksRegistry::create()` auto-creates the book's one entity (named after the book) immediately after opening the encrypted store, before `book.json` is ever written — `BookMeta` (and therefore every `list_books`/`list_my_books`/`create_book` response) now carries `entity_id` directly, so no separate entity-discovery round trip exists or is needed.
- [x] Backend: retire `POST /books/:id/entities` (`create_entity`) and `GET /books/:id/entities/mine` (`list_my_entities`) — both client-facing operations that only made sense when a book could hold more than one entity. Keep `GET /books/:id/entities` (admin inspection) since it's still a legitimate read, now simply guaranteed to return exactly one result.
- [x] Launcher: the M6 picker drops its second dropdown — selecting a book goes straight to that book's `workflows/mine` using the `entity_id` already present on the book, matching the UX question raised independently while reviewing M6 ("why do I have to choose an entity after a book") — same design smell, same fix.
- [x] Update `scripts/demo_seed.sh` and `docs/LedgerZero_Manual_Verification.md` to match: no explicit entity-creation step, one dropdown instead of two.
- [x] Tests: `create_entity` rejects a second call; `create_book` response and `list_my_books`/`list_books` all carry the correct `entity_id`; the retired routes are gone (404); the simplified picker still correctly scopes by role assignment (re-verified, not just carried over untested).

**Exit criteria:** a book can never end up with more than one entity, structurally (not just by convention); the launcher picker needs only a book selection, never a separate entity selection, to reach a user's workflows.

**✅ DONE.** `engine::create_entity` rejects a second `create_entity` call with `INVALID_INPUT` (`a_book_has_exactly_one_entity` in `engine/tests/engine_core.rs`), and `BooksRegistry::create` auto-creates the one entity before `book.json` is written, threading `entity_id` through `BookMeta`/`create_book`/`open_book`. `POST /books/:id/entities` and `GET /books/:id/entities/mine` are gone from the router; `GET /books/:id/entities` remains as inspection-only. The launcher (`frontend/src/App.jsx`) is down to a single Book `<select>` — no entity dropdown, no `selectedEntityId` state. `scripts/demo_seed.sh` reads `entity_id` straight off the `create book` response; `docs/LedgerZero_Manual_Verification.md` Parts 4/5 describe the single-dropdown flow. Full workspace test suite green (`cargo test --workspace`: 13 test binaries, 0 failures), including the rewritten `backend/tests/books_flow.rs` and `backend/tests/workflows_flow.rs` (idempotency/copy-chart/unopened-book tests moved off the retired `/entities` route onto `/resource-types`; `book_and_entity_picker_scopes_by_role_assignment` re-verified against `/books/mine` + `/workflows/mine`). Browser-verified end-to-end via `./scripts/demo_seed.sh`: owner sees the book with no role-granted workflow ("No workflows in this book are assigned to you"); the assigned employee selects the book and the workflow link appears immediately (no entity step) with the correct `book_id`/`entity_id` query params, and posting through it succeeds (`Posted entry ... (execution ...)`).

**Notes:**

- This is a **narrowing** correction applied to already-shipped milestones (M2's `Entity` type, M4's `create_entity` API, M5's `WorkflowDefinition.entity_id`/`Role.entity_id` fields, M6's picker), not a new forward capability — hence recording it as a spec resolution (Appendix A, R1) in addition to a milestone entry, following the project's standing rule to record a spec resolution before coding around a problem.
- Deliberately *not* done: `entity_id` remains an explicit parameter on entity-scoped APIs (`deploy_workflow`, `create_role`, `create_period`, `create_chart`, etc.) even though it is now always the book's one entity. Scrubbing it from every one of those request bodies would be a much larger refactor for cosmetic benefit only, since the engine already validates it and it costs the caller nothing to keep supplying it. Not required to close the security-boundary gap, which is now closed structurally at the point that actually mattered (an entity cannot be *created* except the one auto-created with the book).
- Intercompany activity between related legal entities remains separate transactions in each entity's own book, linked by metadata — never by sharing a book (Impl Spec §2.9).

## M8 — AI generation path (MCP + Python dev-time backend)

- [x] MCP primitives: `generate_workflow_definition`, `deploy_workflow_definition`, `list_workflows`, `get_workflow_definition` + admin primitives from spec §6.4
- [x] Python dev-time backend: LLM wrapping, prompt/context assembly, artifact preparation — no storage credentials, no persisted private context (Axiom 12)
- [x] Generate and deploy `Recording bank account transactions manually` (spec §6.6) via the full path: MCP → dev backend → artifact → deployment
- [x] Verify generated app passes the same e2e and authorization tests as hand-built artifacts

**Exit criteria:** a workflow authored by natural language runs in the browser with no hand-edits, or fails with an explicit missing-primitive answer.

**✅ DONE.** `mcp_server/src/first_principle_accounting/` rebuilt on real dependencies (`mcp` SDK, `httpx`) in a project-local venv (`scripts/check.sh` now bootstraps it): `config.py`/`errors.py`/`runtime_client.py` (the only way anything here touches accounting data — one method per backend endpoint, never a book file, Axiom 12); `devtime/generator.py` (`generate_workflow_definition` — the "LLM wrapping" seam, v1 generating deterministically from a structured request per the original Project Plan's own guidance: "starting with deterministic template generation before enabling real LLM calls"; generalizes both the fixed-direction shape from M5's hand-built artifact and a new direction-toggle shape, so a workflow's amount can flip which account is debited based on a form field); `devtime/artifacts.py` (`prepare_artifact` — writes the dev artifact store layout, Impl Spec §7.4, with atomic-style cleanup on any failure so a retry never finds a half-written artifact); `mcp/tools.py` (all 15 primitives as plain, SDK-free async functions — unit-testable on their own); `mcp/server.py` (registers each as an actual MCP tool via `FastMCP`, one dev-login session per process lifetime); `cli/main.py` (`serve` runs the stdio server; `run-tool <name> --json '<args>'` dispatches one primitive directly — used to drive the deliverable below). Stale pre-Rust-pivot `domain/`/`application/` stub packages (empty since the M0 scaffold, contradicted by the package's own `test_no_engine_or_storage_modules` test) removed. 25 Python tests (generator template shapes incl. the direction-toggle math, artifact layout/overwrite-guard/failed-write cleanup, `RuntimeBackendClient` against a mocked transport, tools.py argument marshalling) plus the full Rust suite all green via `./scripts/check.sh`. Full path driven for real against a live backend via `run-tool`: `create_accounting_book` → `create_resource_type`/`create_chart`/`create_account` ×2/`create_period` → `generate_workflow_definition` (spec §7.5.2's exact field list: bank account, transaction date, amount, direction, description, offset account, optional reference) → `deploy_workflow_definition` (backend re-hashed the artifact from disk and returned real `manifest_hash`/`code_hash`, confirming a genuine deployment, not a stub) → `assign_role_to_user` on the auto-created role. Browser-verified end-to-end exactly like M5's hand-built artifact: the employee's launcher picker shows the generated workflow by name with no hand-edits; a deposit and a withdrawal both post correctly (`debit_total 2500.00`/`credit_total 400.00`/`net 2100.00` — proving the direction-toggle logic swaps debit/credit correctly both ways, not just the happy path); the bootstrap owner, holding no role for this workflow, gets the same client-side pre-flight warning and the same server-side `403 UNAUTHORIZED_WORKFLOW` on submission as the hand-built artifact — the generic backend authorization machinery doesn't know or care that this artifact was generated rather than hand-written.

**Notes:**

- `get_workflow_definition` reads `workflow.json`/`manifest.json` straight from the local dev artifact store rather than calling a backend endpoint — legitimate under Axiom 12 since dev artifacts are explicitly *not* book storage (Impl Spec §7.4), and "inspectable by authorized users" is exactly what this primitive does for the one developer identity in v1.
- Sub-book, consolidation, and `explain_reconciliation_issue` primitives from spec §6.4's list are deliberately not implemented yet — those backend features don't exist until M9/M11, and a primitive with no endpoint to call would just be a stub pretending to work.

## M9 — Periods in practice and reconciliation

- [ ] `Reconcile bank accounts at EOP` workflow: compare projection vs expected balance, report discrepancies, corrections as new entries, result as administrative event (spec §6.6)
- [ ] Period close/reopen exercised through workflows; closed-period posting rejected end-to-end
- [ ] `explain_reconciliation_issue` MCP primitive over runtime facts

**Exit criteria:** full monthly cycle: post, reconcile, close, attempt late post (rejected), reopen, correct, re-close.

## M10 — Export and restore

- [ ] `export_book`: encrypted JSON bundle, reader-passphrase encryption, deployment references + hashes, snapshot cut under writer lock + ledger marker (spec §7.3, §4.3)
- [ ] `restore_book`: wipe-and-replace, IDs and `book_id` preserved, RESTORE event, unavailable-workflow marking when artifacts don't match by id+hash
- [ ] Round-trip test: export → restore to fresh location → continue posting

**Exit criteria:** a book moves to a new folder/deployment and keeps operating.

## M11 — Sub-books and consolidation

- [ ] `create_sub_book` with owner choice and copy mode (all / none / owner-only; different owner → none) (spec §2.8)
- [ ] SUB_BOOK_LINK events in both books; in-file link and rule projections
- [ ] `define_consolidation_rule`, `list_consolidation_rules`, `run_consolidation`: derived parent entries, idempotent, pending when unmapped or unauthorized
- [ ] Tests: default consolidation, summary-account mapping, pending-until-authorized, idempotent re-run

**Exit criteria:** parent book consolidates a child book on one deployment; re-runs create no duplicates.

## M12 — Hardening and deployment

Partially pre-done during M0/M1: `scripts/package.sh` (release tarball) and `docs/LedgerZero_Run_and_Deploy.md` (local run, deployment rehearsal, monitoring signals, remote-deployment caveats) already exist; this milestone extends them.

- [ ] Oracle Cloud VM deployment (systemd or equivalent) and on-premise instructions
- [ ] MFA guidance, ingress restrictions for non-local deployments (spec §5.5)
- [ ] Operational docs: bootstrap, open-book, backup/push, ownership transfer (incl. git-history caveat), restore runbook
- [ ] Full test-suite pass + a scripted demo covering M4–M11 flows

**Exit criteria:** a fresh operator can install, bootstrap, and run the demo from docs alone.

## Deferred (tracked, not scheduled)

Cross-server consolidation auth; FX translation between books; consolidation scheduling beyond on-demand; year-end close workflow; reporting tools; re-open-by-branching; brokerage import; containerization; SQLite/Postgres drivers; identity-merge workflow (Theorem T4) and runtime provider-administration workflow (Theorem T3) once workflow machinery exists (M5+).

Standing architectural guarantees are tracked in `LedgerZero_Theorems.md`; every milestone must preserve them.
