# LedgerZero Implementation Plan v1

Milestone plan for building `LedgerZero_Impl_Spec_v1.md`. The plan starts with a **walking skeleton**: authentication and authorization across all architectural components first (M1), since creating even the first book requires authN/Z in place. The accounting core then fills in behind that skeleton. Each milestone ends with passing tests and something demonstrable. Do not start a milestone until the previous one's exit criteria pass. Spec section references are to the Impl Spec.

Working rules for each milestone:

- One milestone = one or a few focused sessions; each ends in a commit on a branch merged to main.
- Tests written inside the milestone, not deferred (spec §7.5).
- If implementation reveals a spec problem, record the resolution in the Impl Spec (and Appendix A) before coding around it.

## M0 — Repository scaffold

- [ ] Create Rust workspace: `engine/` and `backend/` crates (spec §7.2)
- [ ] Scaffold `frontend/` React + Vite launcher (empty shell: builds and serves a page)
- [ ] Restructure `mcp_server/`: remove `engine/` and `storage/` modules; keep MCP + dev-time backend skeleton
- [ ] Add `server.config.example.toml` (OAuth config, books dir, listen address, bootstrap_owner_email)
- [ ] Wire `cargo test`, `cargo clippy`, frontend build, and Python tests into a single check script / CI

**Exit criteria:** clean clone builds everything; all (empty) test suites pass.

## M1 — Walking skeleton: authentication & authorization

All architectural components stand up here, doing the minimum real work: routing, login, sessions, and authority checks — no accounting yet.

- [ ] Axum server as the routing server: serves launcher assets, routes API requests (spec §7.1)
- [ ] Google OAuth login flow; user records; AKA mapping `(provider, subject) → user_id` (spec §2.9, §5.2)
- [ ] Session tokens (1h) with refresh rotation; every API call carries a verified identity claim
- [ ] Bootstrap authorization from `server.config.toml`: on a fresh install, only `bootstrap_owner_email` passes owner-gated endpoints (spec §5.3)
- [ ] Authorization framework: role store lookup + structured UNAUTHORIZED_* errors (spec §4.4); workflow-scoped enforcement completes in M6 when deployments exist
- [ ] Operational audit log for failed authentications/authorizations, separate from the ledger (spec §5.2)
- [ ] Launcher: Google login, session handling, and a protected "who am I / what may I do" page proving the full path browser → routing → backend
- [ ] Integration tests: unauthenticated rejected, authenticated non-owner rejected at owner-gated endpoints, bootstrap owner accepted, token expiry/refresh

**Exit criteria:** in a browser: login with Google, see identity and authority; owner-gated test endpoint accepts only the bootstrap owner. All components (backend, frontend, config) participating.

## M2 — Domain model and engine core (in-memory)

- [ ] Domain types: ResourceType, Chart, Account (derived normal_balance), JournalEntry, JournalLine, Period, price facts, event envelope (spec §2)
- [ ] Structured error catalog completed (spec §4.4)
- [ ] Invariant checks on posting: exact balance incl. cross-unit via entry-recorded prices, account validity/active, single chart+entity, open period, unit coverage, account validation rules (spec §4.1)
- [ ] Posted-or-rejected semantics; reversal entries; expanded-equation balance check A = L + E + (R − X)
- [ ] Balance computation and price projection from the event log (spec §2.6, §4.3)
- [ ] Property tests: any generated entry either posts balanced or is rejected; no operation sequence unbalances a chart
- [ ] Replay tests: projections rebuilt from event log equal incrementally maintained state

**Exit criteria:** property and replay tests pass; engine usable as a pure library with an in-memory store.

## M3 — Encrypted single-file storage

- [ ] Storage interface (async trait) per spec §3.2
- [ ] Serialization of full book state to logical event records + reference state
- [ ] `book.data.enc`: AES-256-GCM; `book.keystore.json`: Argon2id-wrapped book key; `BookKeyProvider` contract (spec §5.4)
- [ ] Atomic file replacement (temp + fsync + rename), writer lock (spec §3.1)
- [ ] Load: decrypt, replay, build all in-memory indexes/projections incl. idempotency index
- [ ] Idempotency at storage boundary: identical replay returns original outcome; payload mismatch → IDEMPOTENCY_CONFLICT
- [ ] Git commit after successful mutation batch (spec §3.3)
- [ ] Crash-safety tests (kill during replace → pre-mutation state on reload); round-trip and wrong-passphrase tests

**Exit criteria:** create → mutate → kill → reload cycles prove durability; all M2 tests pass against the file driver.

## M4 — Book lifecycle and core accounting APIs

Now the skeleton gets its accounting organs: everything behind the M1 auth boundary.

- [ ] `create_accounting_book` (bootstrap-owner-gated) and `open_book` (owner passphrase → key into memory) (spec §5.3, §5.4)
- [ ] Reference APIs: `create_entity`, `create_resource_type`, `create_chart`, `copy_chart`, `create_account`, `update_account_metadata`, `deactivate_account`, `list_accounts`, `create_period`, `close_period`, `reopen_period`
- [ ] Ledger APIs: `post_entry`, `reverse_entry`, `get_balance`, `list_entries`, `get_audit_log`, `record_price`, `list_prices`
- [ ] Client-generated UUID idempotency on every mutation; structured errors over HTTP
- [ ] API integration tests: full book lifecycle via HTTP incl. auth failures, idempotent replay, and conflict cases

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

- [ ] Oracle Cloud VM deployment (systemd or equivalent) and on-premise instructions
- [ ] MFA guidance, ingress restrictions for non-local deployments (spec §5.5)
- [ ] Operational docs: bootstrap, open-book, backup/push, ownership transfer (incl. git-history caveat), restore runbook
- [ ] Full test-suite pass + a scripted demo covering M4–M10 flows

**Exit criteria:** a fresh operator can install, bootstrap, and run the demo from docs alone.

## Deferred (tracked, not scheduled)

Cross-server consolidation auth; FX translation between books; consolidation scheduling beyond on-demand; year-end close workflow; reporting tools; re-open-by-branching; brokerage import; containerization; SQLite/Postgres drivers; identity-merge workflow (Theorem T4) and runtime provider-administration workflow (Theorem T3) once workflow machinery exists (M6+).

Standing architectural guarantees are tracked in `LedgerZero_Theorems.md`; every milestone must preserve them.
