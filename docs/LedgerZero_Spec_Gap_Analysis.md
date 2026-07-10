# LedgerZero Spec — Gap Analysis

Review of `LedgerZero_Spec.md` (2026-07-09). Three categories: internal contradictions to fix in the spec, undefined items that block implementation, and items that are safely deferrable but should be marked as such.

## 1 Internal Contradictions

1. **`ledger.jsonl` vs single encrypted file.** §3 and §3.3 define a single `book.data.enc` with no append-only file on disk. But §2.1.2 describes "each `ledger.jsonl` record", §6.4 mentions "stream-level encryption for the raw `ledger.jsonl` stream", and the §3.2 driver table describes the default driver as "JSON/JSONL files, append-only event stream". These are leftovers from an earlier design. Recommend: keep the event-envelope definition in §2.1.2 (it's good) but describe it as the logical record format inside `book.data.enc`, and update §3.2/§6.4.

2. **Projection files on disk.** §3.1: "There are no separate projection files on disk." §7.5.4 requires creating/updating `book_links.json`; §7.5.5 requires `consolidation_rules.json`. Decide: either these are in-memory projections inside `book.data.enc`, or the no-projection-files rule has exceptions.

3. **Sub-book role copying.** §2.2.0.1: "parent book settings **and all authorizations/role assignments** are copied into the child book at creation time." §7.5.4: "copy parent book settings into the child book as defaults, **except role assignments** … Role assignments are intentionally not copied." Direct contradiction. Note the choice also determines whether consolidation works immediately after sub-book creation (§2.2.0.1 justifies copying precisely so "consolidation read APIs and parent workflows function immediately").

4. **Git "easy diff" claim.** §3.2 says the git-backed store gives "easy diff/backup/history", but the artifact is one encrypted blob — git cannot diff it, and every mutation stores a full new copy. Backup/history still work; diff does not. Also unstated: after ownership transfer re-encrypts the book, git history still holds blobs readable by the old owner's key.

5. **Approval fields vs approval model.** `status = PENDING_APPROVAL` and `approved_by` exist on the journal entry (§2.1.2), but §4.2(6), §6.2 and §7.1 say approval is *not* an engine concept and is modeled as separate workflows (e.g., posting from a pending account). Either remove the status/field for v1 or define exactly when the engine sets them.

6. **Per-workflow roles.** §7.1: "any workflow always belongs to a role with the same name." This auto-role rule appears once, is not in §2.2.4 or §6.1, and interacts oddly with "role = named collection of workflows". Confirm or delete.

## 2 Blockers — undefined but required to write v1 code

1. **Journal entry state machine.** §2.1.2 says "State machine — see Section 4", but §4 never defines the DRAFT → PENDING_APPROVAL → POSTED → VOIDED transitions. Bigger issue: how do DRAFT entries and status *transitions* exist in an immutable, append-only ledger (Axiom 1)? Is VOIDED a status mutation (violates Axiom 1) or an appended void event? Is `reversal_of` the only correction mechanism and VOIDED redundant? This is the core engine; it can't be coded without an answer. Simplest v1 answer: no DRAFT/PENDING_APPROVAL in the ledger at all — entries arrive complete and are either POSTED or rejected; corrections are reversal entries; drop VOIDED.

2. **Price tables.** Cross-unit balancing (§2.1.3, §4.2.1) depends on "the price map from the price tables", which is never defined: schema, how prices are recorded (an event type? which one?), which price applies at "transaction time" (entry_date vs posted_at), price direction/pair semantics, and rounding tolerance for `debit == credit * price` under Decimal(18,8). Without a rounding rule, most real prices make entries unpostable.

3. **Resource type / unit of measure.** Axiom 4: every resource type has exactly one unit of measure. But `resource_type` is a 5-value enum, Account has no `unit_of_measure` field, and only CURRENCY gets a code (`currency_code`). How does the engine know the unit of an INVENTORY or COMMODITY account? Likely fix: replace the enum + currency_code with a ResourceType entity (id, kind, unit, code) referenced by accounts.

4. **Journal line amount semantics for non-currency accounts.** §2.1.3 says debit/credit are "in account's resource unit", and also has `quantity` and `unit_value`. If debit_amount is already in units, `quantity` is redundant; if debit_amount is in currency, "account's resource unit" is wrong. §4.2(4) requires quantity+unit_value on non-currency lines but doesn't state the enforced relation (debit_amount == quantity × unit_value?). Pick one representation.

5. **Chart of accounts entity.** `chart_id` is referenced but no Chart table is defined (name, owner: entity or book?, status). Unresolved: an account has both `chart_id` and `entity_id`; §2.2.1 says each entity has its own chart; §5.3 allows multiple coexisting charts. Which chart does a posting workflow post into, and how is the "active" chart per entity chosen? How does copy-a-chart interact with account UUIDs (new accounts with new IDs, presumably)?

6. **Accounting Period schema and APIs.** §2.2.2 defines no fields (start/end, status, entity scope, fiscal calendar, overlap rules). Storage has `set_period_status` but the backend API list (§7.4) has only `create_period` — no close/re-open period API or sample workflow, even though period-close integrity is a claimed SOX control (§6.2) and §4.2(3) enforces open periods.

7. **Equation check across the five account types.** §5.1 requires each chart to be "internally balanced as Assets = Liabilities + Owner's Equity", but charts contain REVENUE and EXPENSE accounts. State the expanded equation (A = L + E + R − X) or define what the balance check actually computes. Related and absent: year-end closing / retained earnings — is closing revenue/expense into equity a workflow, and what does v1 need?

8. **Key management mechanics.** §6.4 defines good boundaries but not the mechanism: what secret does the owner present in `Open book` (passphrase? key file? something derived from Google identity?), the KDF/cipher for `book.keystore.json`, what "encrypted for the intended reader" means for exports (recipient public key? passphrase exchange?), and the concrete re-encryption procedure on ownership transfer. Code needs algorithm choices (e.g., age/AES-256-GCM + Argon2id) even if the provider is pluggable.

9. **First-run bootstrap.** §7.5.0 covers opening an *existing* book. Undefined for a virgin install: where OAuth client config lives (it cannot be inside the encrypted book, since login precedes `Open book`), how the very first user record and owner are created before any book exists, and how `create_accounting_book` is authorized when there is no book to hold the authorization.

10. **Reporting.** No trial balance, balance sheet, income statement, or account-activity report appears anywhere — only `get_balance` and `list_entries`. Even v1 of an accounting platform likely needs a trial balance (it's also the natural implementation of "reconciled before export", §8.2). Decide the minimum report set and whether reports are workflows.

11. **API-surface deltas.** Model features with no corresponding backend API or MCP primitive: reverse/void an entry (`reversal_of` exists), deactivate an account / update account metadata, close/re-open period, `get_audit_log`, and `assign_workflow_to_role` (MCP has it; backend §7.4 list has only `assign_role`). Fine to grow "workflow by workflow", but these are implied by v1 sample workflows and invariants.

12. **Idempotency conflict rule.** §7.4 defines replay behavior for the *same* ID, but not the same ID with a *different* payload (client bug). Recommend: reject with a conflict error rather than returning the original result. Also: where idempotent outcomes for administrative events are stored/looked up.

13. **Tech stack and repo layout for the vibe-coding loop.** The spec fixes Rust for the engine and Python for the dev-time server, but not: the runtime backend application server's language/framework (Rust with the engine in-process? something else?), frontend framework, what the routing server actually is (reverse proxy? app-level router?), repo layout, and build/test tooling. For AI-driven implementation this is the single highest-leverage missing section — without it every session re-decides the stack.
    - **Repo mismatch to resolve:** the existing `mcp_server` Python package contains `engine/` and `storage/` modules. Per §3 and §8.1, the Python side must have *no* accounting storage access; storage belongs to the Rust `AccountingEngine` behind the runtime backend. Either this code predates the spec and gets restructured, or the spec's boundary needs revisiting.

14. **Error model.** §4.2 promises "a structured error"; no error shape or code catalog is defined. A short table (code, meaning, HTTP status) is enough.

15. **Testing strategy.** Absent. For an invariant-enforcing engine, at minimum: property tests for the double-entry/period/authorization invariants, replay tests (rebuild projections from events and compare), and idempotency tests. Worth one paragraph in §8 so generated code is held to it.

## 3 Deferrable — but mark as deferred in the spec

1. **Consolidation cross-server authentication.** A parent user runs consolidation against a remote child server — with their Google identity, requiring a role in the child book? (Interacts with contradiction #3.) Fine to defer while parent/child share a deployment; say so.
2. **FX / unit translation in consolidation** when parent and child books use different currencies or units — undefined; defer explicitly.
3. **Consolidation scheduling** — "by default all posted child transactions are consolidated": continuously, on a schedule, or on demand? On-demand (`run_consolidation`) is a fine v1 answer; state it.
4. **Git operational policy** — who commits/pushes `book.data.enc`, at what cadence (per mutation? per session?), and remote credential handling.
5. **Concurrency detail** — writer lock is in-process only vs cross-process (file lock), and read behavior during a rewrite. Single-process v1 makes this trivial; one sentence saying so is enough.
6. **User/AKA table schema** — fields for user, Google subject mapping, AKA merge records.
7. **Session/token issuance details** — which component issues/verifies tokens (the routing server?), session storage.
8. **Document title** says "Product Specification & Project Plan", but the plan lives in `LedgerZero_Project_Plan.md` — retitle or cross-reference.

## Suggested order of resolution

Fix the six contradictions first (they're edits, not design). Then decide blockers 1–5 (entry lifecycle, prices, units, journal-line semantics, chart entity) — everything in the engine depends on them. Then 6–10 (periods, equation/close, keys, bootstrap, reporting), which shape the backend API. Item 13 (stack + repo layout) can be decided in parallel and should be written down before the first coding session.
