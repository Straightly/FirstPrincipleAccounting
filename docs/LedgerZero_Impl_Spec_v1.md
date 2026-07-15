# LEDGER ZERO — Implementation Specification v1

This is the implementation baseline for the first build. It supersedes `LedgerZero_Spec.md` for implementation purposes; the original remains the vision/design document. Every contradiction and blocker identified in `LedgerZero_Spec_Gap_Analysis.md` is resolved here; Appendix A maps each gap to its resolution.

## Delivery phases (resolution R2 — Appendix A)

Everything in this document is the full v1 design; not all of it ships before the user starts running real books. **Phase 1** is the release the user operates as its first real user — single-book accounting core, hand-built and AI-generated workflows, export/restore, hardening/deployment (Impl Plan M1–M10, renumbered to be contiguous when this split landed). **Phase 2** — deferred until Phase 1 is in real use, not removed from scope — is periods-in-practice-as-a-workflow and reconciliation (Impl Plan M11) and sub-books/consolidation (Impl Plan M12). Nothing in Phase 1 depends on Phase 2: period close/reopen and closed-period rejection are already engine/backend/MCP-level capabilities since M2/M4/M8, reachable directly without a dedicated browser workflow; export/restore and hardening operate on one book regardless of whether sub-books ever exist. Sections describing Phase 2 features are marked inline; see the Impl Plan for the full milestone content, relocated there under "Phase 2."

## 1 Vision & Axioms

Ledger Zero is a production-grade, AI-native accounting platform built from accounting first principles, derived from one axiom:

**Assets = Liabilities + Owner's Equity**

For invariant checking, the equation is applied in expanded form: **A = L + E + (R − X)**, where REVENUE and EXPENSE accounts are treated as unclosed components of equity. Year-end closing (rolling R/X into retained earnings) is an ordinary workflow and is deferred past v1.

### 1.1 Design Axioms

- AXIOM 1 — The ledger is immutable. Entries are appended, never mutated. Correction is a new reversal entry, not an edit.
- AXIOM 2 — The double-entry invariant is enforced at the data layer. No entry posts unless debits equal credits exactly. There is no tolerance.
- AXIOM 3 — Each chart of accounts is a self-consistent account structure. Adding or changing a chart does not mutate historical ledger events.
- AXIOM 4 — Currency is a resource type. All resources share the same `ResourceType` primitive. Every resource type has exactly one unit of measure; every account has exactly one resource type.
- AXIOM 5 — Storage is abstracted. The engine speaks to a storage interface; the storage medium is a driver.
- AXIOM 6 — Workflows are generated, not hardcoded. Generated code is part of the application and auditable like core code.
- AXIOM 7 — Security is structural. Role permissions are enforced at the data layer, not the UI.
- AXIOM 8 — Audit trail is a consequence of architecture. Because the ledger is immutable and all state changes are events, the audit trail exists by default.
- AXIOM 9 — The system is maintainable by a person who understands accounting.
- AXIOM 10 — Migrations are workflows.
- AXIOM 11 — Start small without architectural regret.
- AXIOM 12 — Dev-time artifacts do not retain private accounting context.
- AXIOM 13 — Backend mutation APIs are idempotent, keyed by client-generated UUIDs.

## 2 Core Data Model

### 2.1 ResourceType

Implements Axiom 4. Replaces the former `resource_type` enum + `currency_code` fields on Account.

| Field | Type | Constraints |
|----|----|----|
| resource_type_id | UUID | Immutable, system-generated |
| book_id | UUID | Owning book |
| name | String | Required, unique within book (e.g. "US Dollar", "Widget-A") |
| kind | Enum | CURRENCY, INVENTORY, COMMODITY, DIGITAL_ASSET, OTHER |
| code | String | e.g. ISO 4217 for currency; required for CURRENCY |
| unit_of_measure | String | Required — exactly one per resource type (e.g. "USD", "each", "kg") |
| precision | Integer | Decimal places meaningful for this unit (e.g. 2 for USD, 0 for "each") |
| metadata | JSON | Schema-free |

### 2.2 Chart of Accounts

| Field | Type | Constraints |
|----|----|----|
| chart_id | UUID | Immutable |
| entity_id | UUID | Owning entity |
| name | String | Required, unique within entity |
| description | String | Optional |
| is_active | Boolean | Exactly one active chart per entity at a time |
| created_at | Timestamp | Immutable |

Rules:

- Accounts belong to exactly one chart; each chart must independently satisfy the expanded equation.
- Multiple charts coexist within an entity, but posting workflows post into the entity's **active** chart unless the workflow explicitly targets another chart.
- Copying a chart creates a new `chart_id` and clones every account with a **new** `account_id`; each clone records its source account in metadata. Historical entries keep their original account references.
- When a new chart requires transactions to be distributed differently, the required rules, tags, or extra inputs are added to the affected workflow definitions at development time (unchanged from the original spec).

### 2.3 Account

| Field | Type | Constraints |
|----|----|----|
| account_id | UUID | Immutable, never reused |
| chart_id | UUID | Required |
| entity_id | UUID | Required |
| name | String | Required, unique within chart |
| code | String | Optional, user-defined |
| account_type | Enum | ASSET, LIABILITY, EQUITY, REVENUE, EXPENSE — immutable after creation |
| normal_balance | Enum | DEBIT or CREDIT — always derived from account_type, never stored from input |
| resource_type_id | UUID | Required — FK to ResourceType |
| parent_account_id | UUID | Optional hierarchy |
| is_active | Boolean | Inactive accounts reject new lines; deactivation is a ledger event |
| validation_rules | JSON | Optional account-defined validation invoked by the engine |
| created_at | Timestamp | Immutable |
| metadata | JSON | Schema-free |

ASSET and EXPENSE have DEBIT normal balance; LIABILITY, EQUITY, REVENUE have CREDIT normal balance.

### 2.4 Journal Entry

**Lifecycle: posted-or-rejected.** There is no DRAFT, PENDING_APPROVAL, or VOIDED status and no `status` field. An entry submitted to the engine is validated atomically: it either becomes a POSTED immutable ledger event or is rejected with a structured error and leaves no trace in the ledger. Correction is a reversal entry referencing `reversal_of`. Approval is not an engine concept: where approval is needed, it is modeled as ordinary workflows (e.g. posting into a pending account, with a separate approval workflow moving value to the target account, assigned to a different user).

| Field | Type | Constraints |
|----|----|----|
| entry_id | UUID | Client-generated (idempotency key), verified unique by engine |
| book_id | UUID | Required |
| entity_id | UUID | Required |
| entry_date | Date | Economic date — may differ from posted_at |
| posted_at | Timestamp | System-set, immutable |
| posted_by | UUID | FK to user, immutable |
| workflow_id | UUID | Nullable — null for manual entries |
| workflow_deployment_id | UUID | Nullable — set when source = WORKFLOW |
| workflow_execution_id | UUID | Client-generated, required for workflow-originated entries |
| event_type | Enum | See event catalog below |
| description | String | Required |
| reversal_of | UUID | Nullable — the entry this reverses |
| period_id | UUID | FK to period, enforced open on post |
| prices | List | Required when lines span resource types — see 2.6 |
| source | Enum | MANUAL, WORKFLOW, DERIVED, ADMIN, RESTORE, SYSTEM |
| metadata | JSON | Schema-free |

**Event catalog** (`event_type`): ACCOUNTING, ADMINISTRATIVE, WORKFLOW_DEPLOYMENT, ROLE_ASSIGNMENT, SUB_BOOK_LINK, CONSOLIDATION_RULE, CONSOLIDATION, PRICE, PERIOD_STATUS, ACCOUNT_STATUS, SYSTEM_DERIVED, RESTORE.

The single ledger records both accounting transactions and administrative events. Only ACCOUNTING (and derived CONSOLIDATION entries carrying lines) affect balances. Each ledger record is a logical event envelope — `event_id` (= entry_id), `book_id`, `event_type`, `occurred_at`, `actor_user_id`, workflow context, payload — serialized inside the single encrypted book file (see Section 3). Client-generated IDs are the idempotency keys; no separate `idempotency_key` exists.

### 2.5 Journal Line

| Field | Type | Constraints |
|----|----|----|
| line_id | UUID | Client-generated, unique within entry |
| entry_id | UUID | FK, immutable |
| account_id | UUID | FK, account must be active |
| debit_amount | Decimal(18,8) | ≥ 0; null if credit side; exactly one side non-null |
| credit_amount | Decimal(18,8) | ≥ 0; null if debit side |
| memo | String | Optional |
| metadata | JSON | Schema-free (source documents, external references) |

**Amounts are always denominated in the account's own resource unit.** A line on a USD cash account is in USD; a line on a Widget-A inventory account is in "each". The former `quantity` and `unit_value` fields are removed — they are redundant with unit-denominated amounts plus entry-level prices.

### 2.6 Prices and Cross-Unit Balancing

- A price is a fact: `{base_resource_type_id, quote_resource_type_id, rate, as_of}`.
- **A cross-unit entry records its own prices and must balance exactly using them.** The `prices` list on the entry declares the rate for every resource-type pair the entry spans. The engine converts all lines to a single valuation unit using exactly those recorded rates and requires debits == credits exactly. No unbalanced entry is ever accepted; there is no tolerance and no implicit rounding.
- Rounding is the workflow's problem, solved before submission: the workflow computes line amounts (including any explicit rounding/gain-loss line it chooses to add) such that the entry balances exactly at its recorded prices.
- Standalone price changes are recorded as PRICE events. The in-memory **price projection** is built from PRICE events plus prices embedded in posted entries. It supplies default/suggested rates to workflows; it is never used to re-check a posted entry — the entry's own recorded prices are authoritative for that entry forever.

### 2.7 Accounting Period

| Field | Type | Constraints |
|----|----|----|
| period_id | UUID | Immutable |
| entity_id | UUID | Required |
| name | String | e.g. "2026-07" |
| start_date | Date | Required |
| end_date | Date | Required, ≥ start_date |
| status | Enum | OPEN, CLOSED |

Rules: periods must not overlap within an entity; `entry_date` must fall inside an OPEN period of the entry's entity. Closing and re-opening are PERIOD_STATUS ledger events performed through authorized workflows (`close_period`, `reopen_period`). Re-opening-by-branching/reversal remains deferred.

### 2.8 AccountingBook and Sub-Books

**Sub-book creation and consolidation are Phase 2 (Impl Plan M12, deferred — see "Delivery phases" above); the one-entity-per-book boundary below is Phase 1.**

Unchanged in structure from the original spec (book_id, optional parent_book_id, one storage folder, one owner at bootstrap, ownership transfer re-encrypts). **Amended (Impl Plan M7, resolution R1):** the book is the storage, export, restore, bootstrap security, *and* accounting boundary — the two now coincide. Every book has exactly one entity, auto-created when the book is created (2.9); there is no API to create a second one. A book's encryption key, owner, and authority are all scoped to that one legal/accounting entity, so a key compromise or an owner's authority never spans more than one entity's books.

Related legal entities — a holding company's subsidiaries, or one bookkeeper's several clients — are *not* modeled as multiple entities inside one book. Each gets its own book (its own key, its own owner, its own storage folder), linked to a parent via `create_sub_book` and combined only through read-only, rule-based, idempotent consolidation (below) — never by sharing a key or a file. This was already the sub-book mechanism's job; removing multi-entity-per-book doesn't remove any capability, it removes a second, weaker path to the same outcome that bypassed the book's own security boundary.

**Sub-book creation and role copying (resolves the §2.2.0.1 / §7.5.4 contradiction):**

- The `Add a sub-book` workflow requires the creator to specify the child book's owner.
- If child owner == parent owner, the creator chooses one of: **copy all** (all parent role/workflow assignments copied as defaults), **copy none**, or **copy owner-only** (role and workflow definitions copied, but assignments granted only to the owner).
- If the child owner differs from the parent owner, **nothing is copied**. The child owner assigns all child-book roles through the child book's own authorization workflows.
- Consequence: when assignments are not copied, default consolidation is **pending** until the child owner grants the parent's consolidation workflow read authorization in the child book. This is the intended operational behavior, not an error.
- The parent/sub-book link is recorded as immutable SUB_BOOK_LINK events in both books. Book links and consolidation rules are **in-memory projections persisted inside the book file** — there are no `book_links.json` or `consolidation_rules.json` files on disk.

All other sub-book and consolidation semantics carry over: separate first-class books, independent keys/storage/endpoints, consolidation via parent-initiated read APIs, consolidation requires the child book to be open, default rule consolidates all posted child transactions when chart/account mapping is resolvable, unresolvable mappings stay pending until a parent consolidation rule (e.g. summary-account mapping) is defined, consolidation is idempotent and never mutates the child ledger. Cross-server consolidation authentication and FX translation between books with different functional currencies are **explicitly deferred**; v1 assumes parent and child on one deployment and same-unit mapping.

### 2.9 Entity, User, Role, WorkflowDefinition

**Entity** — has charts, periods, balances; external parties (customers, vendors, banks) are never separate Entity records, only metadata/labels on transactions ("counterparty" is a contextual label, not a type — original spec §2.2.6). **Amended (Impl Plan M7, resolution R1):** exactly one `Entity` per book, auto-created (named after the book) when `create_accounting_book` runs; `create_entity` is no longer a client-facing operation — the engine rejects a second entity in the same book. Intercompany activity between related legal entities is separate transactions in each entity's own book, linked by metadata, not by sharing a book.

**User**

| Field | Type | Constraints |
|----|----|----|
| user_id | UUID | Immutable |
| display_name | String | Required |
| email | String | Required, unique |
| status | Enum | ACTIVE, DISABLED |

**AKA table**: `(auth_provider, subject_id) → user_id`. First provider is Google Login. A user merges identities by proving control of both (logging into each); merged identities share one user_id and combined authorizations. Every human actor has a unique user_id; impersonation, if ever needed, is itself a role.

**Role** — a named collection of workflows within an entity: `role_id, entity_id, name, description, workflow list`. There is no role hierarchy. **Auto-roles are kept:** deploying a workflow automatically creates (or refreshes) a role with the same name containing exactly that workflow, as the fine-grained assignment unit. Additional composite roles may be created freely. Role/workflow/user assignment events are ROLE_ASSIGNMENT ledger events with full traceability, and all such authority flows from the book owner as in the original spec.

**WorkflowDefinition** — unchanged contract: workflow_name (unique per entity), workflow_deployment_id, artifact_id, entity_id, roles, steps, dev_artifact_path, manifest_hash, code_hash, frontend_route, backend_api_calls, required_inputs, metadata. No user-facing versioning; every deployment is an immutable deployment record; historical entries keep the exact deployment id used.

## 3 Storage

### 3.1 Format — one encrypted file, logical event records

The book is stored as a **single encrypted file** `book.data.enc` containing the complete serialized state: the append-ordered ledger event log plus serialized reference state (accounts, resource types, charts, entities, users, roles, periods, workflow deployment records). It is the sole source of truth. All in-memory indexes and projections (balances, price map, book links, consolidation rules, idempotency index) are rebuilt from it at load.

There is **no `ledger.jsonl` on disk anywhere** — the JSONL/event-envelope framing survives only as the logical record format inside the encrypted file. There are no projection files on disk.

Every mutation rewrites the whole file via atomic replacement (write temp file, fsync, rename). One authoritative writer holds the writer lock during mutation. Writes are O(N) per mutation, which is acceptable for standard accounting datasets; if a book grows too costly to rewrite, the scaling direction is splitting into multiple books, not a new format.

Book folder layout:

```text
<book_name>/
  book.data.enc
  book.keystore.json
  export/
```

### 3.2 Storage interface

As in the original spec (all operations async): append_entry, append_event, get_entry, query_entries, get_balance, append_account, update_account_metadata, get_account, query_accounts, get_period, set_period_status, get_audit_log, load_snapshot, save_snapshot, acquire_writer_lock, release_writer_lock. Drivers enforce idempotency by checking client IDs against the loaded index before applying; a duplicate ID returns the existing outcome.

Later drivers (SQLite, PostgreSQL, DynamoDB, MongoDB) are performance options behind the same interface. **Corrected claim:** the git-backed store provides backup and history, **not diff** — the artifact is one encrypted blob, and each commit stores a full copy.

### 3.3 Git policy

- The book folder lives in a git repository used for backup and point-in-time recovery; a corrupt write rolls back to the last clean commit.
- v1 policy: the backend commits after every successful mutation batch (at minimum, after every workflow execution that mutated state), with the entry/event IDs in the commit message. Push to remote is manual or externally scheduled — the backend never stores remote credentials beside the book.
- **Ownership-transfer caveat:** re-encryption on transfer rewrites the current file, but prior commits remain readable with the old key. Operationally, transfer to an untrusted-with-history party means starting a fresh repository (or rewriting history) for the new owner; this is a documented operator decision, not engine behavior.

### 3.4 Access boundary

Only the runtime backend, acting through the Rust `AccountingEngine`, may open, read, or write accounting storage. MCP, the Python dev-time server, frontend code, and generated workflow code always go through runtime backend APIs.

## 4 Ledger Engine

The engine is the Rust `AccountingEngine` — sole writer, invariant enforcer, deployment-agnostic (in-process for v1). Reads may be concurrent; writes serialize through the writer lock.

### 4.1 Invariants checked on every posting

Any failure rejects the entry atomically with a structured error:

1. **Balance**: debits == credits exactly, converting across units only via the entry's own recorded prices (2.6). `MISSING_PRICE` if lines span units without a recorded price; `UNBALANCED_ENTRY` otherwise.
2. **Accounts**: all referenced accounts exist, are active, and belong to one chart and entity within one book.
3. **Units**: each line's amount is in its account's resource unit by construction; lines must reference accounts whose resource types the entry's prices cover.
4. **Period**: entry_date falls within an OPEN period of the entity.
5. **Authorization**: posted_by is authorized for the workflow (or manual path) in this book; the requested API is in the deployment's `backend_api_calls`; the workflow execution context is consistent (see 7.4).
6. **Idempotency**: unknown entry_id proceeds; known entry_id with identical payload returns the original outcome; known entry_id with **different payload is rejected with `IDEMPOTENCY_CONFLICT`**.
7. **Account validation rules**: if the account defines extra validation, it runs; absent rules, only platform checks apply.

### 4.2 Transaction modes

Unchanged: **realtime** (ACID posting across affected accounts, for e.g. physical inventory movement) and **asynchronous** (accepted event first, POSTED only when actually posted, eventual consistency via derived entries; final ledger consistency is the invariant). Consolidation is derived parent-book activity and idempotent.

### 4.3 Balance computation

Authoritative balances are always computable by summing journal lines from the event log. Materialized snapshots are read caches only. Snapshot/export cuts use the writer lock plus a ledger marker as in the original spec.

### 4.4 Error model

Every rejection returns `{ error_code, message, details }`:

| error_code | Meaning |
|----|----|
| UNBALANCED_ENTRY | Debits ≠ credits at the entry's recorded prices |
| MISSING_PRICE | Entry spans resource types without a recorded price |
| UNKNOWN_ACCOUNT / INACTIVE_ACCOUNT | Account missing or deactivated |
| CHART_MISMATCH | Lines span charts or entities |
| PERIOD_CLOSED / NO_OPEN_PERIOD | entry_date not in an open period |
| UNAUTHORIZED_WORKFLOW / UNAUTHORIZED_API | Authorization re-check failed |
| INVALID_EXECUTION_CONTEXT | workflow_execution_id inconsistent with book/entity/workflow/deployment/user |
| IDEMPOTENCY_CONFLICT | Known client ID with different payload |
| BOOK_NOT_OPEN | Book key not loaded in backend memory |
| VALIDATION_FAILED | Account-defined validation rule failed (details name the rule) |
| INVALID_INPUT | Structural/schema failure |

Idempotent replay of an identical request is **not** an error; it returns the original result.

## 5 Security

### 5.1 Roles and authorization

As in the original spec: structural enforcement at query/write boundaries; authorization is workflow-scoped; workflow authorization implies the backend APIs it needs within one book; no automatic cross-book authority; SOX controls enforced by design (segregation via assigning recording and approval workflows to different users; deferred until a workflow requires it).

### 5.2 Authentication

OAuth2/OIDC; Google Login first. Authentication domains are pluggable behind the `IdentityProvider` interface: any OIDC domain (Google, Microsoft, an enterprise IdP) is a pure data record in a **runtime-mutable provider registry** — adding a domain requires no code change and no restart (Theorems T2/T3 in `LedgerZero_Theorems.md`). Short-lived (1h) session tokens with refresh rotation; every API call carries a verified identity claim; failed authorizations logged to a separate operational audit trail; AKA identity merging per 2.9, with a user-initiated identity-merge workflow (Theorem T4) once workflow machinery exists. A `dev_login` provider exists for credential-free local development only; it MUST be disabled (config) on any deployment reachable by others.

### 5.3 Bootstrap (fresh install)

A plaintext server configuration file (e.g. `server.config.toml`) lives **outside any book folder** and contains: OAuth client configuration, the books directory path, listen address, and `bootstrap_owner_email`. On an installation with no books, only the authenticated identity matching `bootstrap_owner_email` may call `create_accounting_book` (becoming that book's owner) or `open_book`/restore for an existing book folder. This resolves the login-before-open chicken-and-egg: authentication config is never inside the encrypted book.

### 5.4 Encryption and keys

- Book content encrypted at rest with a per-book **AES-256-GCM** book key.
- `book.keystore.json` holds the book key **wrapped by a key derived from the owner's passphrase via Argon2id** (parameters recorded in the keystore; never plaintext keys or passphrases).
- `Open book` (owner-only): owner submits the passphrase; backend derives the wrapping key, unwraps the book key into process memory, loads and decrypts the book. Key stays in memory for the process; restart requires `Open book` again.
- `BookKeyProvider` contract retained — later providers (OS keystore, KMS, HSM) swap in without changing engine or format.
- No key transfer between users; ownership transfer re-encrypts under the new owner's passphrase (see 3.3 for the git-history caveat).
- Exports are encrypted for the intended reader: the export workflow takes a reader passphrase (same Argon2id + AES-256-GCM construction) supplied out-of-band to the reader. Frontend, generated code, and MCP never receive raw keys.

### 5.5 Runtime security

Unchanged from the original spec §6.5: every request untrusted; backend re-checks authorization, book scope, periods, accounts, units, and balance on every mutation; idempotent mutation APIs; generated/frontend code never trusted; parameterized storage operations; no secrets in code or beside the book; MFA and network restrictions for non-local deployments.

## 6 Workflows

### 6.1 Lifecycle

As in the original spec §7.1, with one amendment: deploying a workflow auto-creates its same-named role (2.9). Creation, role assignment, and use follow the original flow; all transactions enter the ledger through workflows; approval-needing accounts use separate approval workflows assigned to different users; no "higher authorization" concept.

### 6.2 AI generation boundaries

Unchanged from the original spec §7.2: generation flows user → MCP → Python dev-time backend → LLM → deployment; the Python dev server has no storage credentials and no persistent private accounting context (Axiom 12); generated code is an untrusted browser-side client running with the user's credentials; the developer is the sole deploy authority in v1.

### 6.3 Reporting

**There are no built-in reports in v1.** All reporting is built as workflows over the read APIs (`get_balance`, `list_entries`, `get_audit_log`). Dedicated reporting tools are an open question deferred until real reporting workflows expose the need. The engine's always-balanced construction is what makes "reconciled before export" a no-op (8.2).

### 6.4 MCP primitives (v1)

`generate_workflow_definition`, `deploy_workflow_definition`, `list_workflows`, `get_workflow_definition`, `create_role`, `assign_workflow_to_role`, `assign_role_to_user`, `create_accounting_book`, `create_sub_book`, `list_sub_books`, `create_resource_type`, `create_chart`, `copy_chart`, `create_account`, `create_period`, `close_period`, `reopen_period`, `define_consolidation_rule`, `list_consolidation_rules`, `run_consolidation`, `explain_reconciliation_issue`. The set grows when a workflow needs a missing primitive; additions are recorded here. **Amended (Impl Plan M7):** no `create_entity` primitive — `create_accounting_book` creates the book's one entity automatically (§2.8, §2.9). **Amended (Impl Plan M8, resolution R2):** everything through `reopen_period` above shipped in Phase 1; `create_sub_book`, `list_sub_books`, `define_consolidation_rule`, `list_consolidation_rules`, `run_consolidation`, and `explain_reconciliation_issue` are Phase 2 (Impl Plan M11/M12, deferred) — no backend endpoint exists for them yet, so they are not exposed as MCP tools until their milestone ships.

### 6.5 Backend application API (v1)

Authentication/authorization endpoints, plus:

- Books: `open_book`, `create_accounting_book` (also creates the book's one entity, §2.9), `export_book`, `restore_book`; `create_sub_book`/`list_sub_books` are Phase 2 (Impl Plan M12, deferred)
- Discovery: `list_my_books` — the launcher's book picker (§7.1). The owner sees every book (as `list_books` already allows); any other signed-in user sees only books where they hold at least one workflow-granting role — never a raw enumeration they'd have to already know the id to request. Each result carries its `entity_id` directly (one per book, §2.8), so no separate entity-discovery call exists or is needed.
- Reference: `create_resource_type`, `create_chart`, `copy_chart`, `create_account`, `update_account_metadata`, `deactivate_account`, `list_accounts`, `list_entities` (inspection only — always returns the book's one entity)
- Ledger: `post_entry`, `reverse_entry`, `get_balance`, `list_entries`, `get_audit_log`, `record_price`, `list_prices`
- Periods: `create_period`, `close_period`, `reopen_period`
- Authorization: `create_role`, `assign_workflow_to_role`, `assign_role_to_user`
- Consolidation (Phase 2, Impl Plan M12, deferred): `define_consolidation_rule`, `list_consolidation_rules`, `run_consolidation`

All rules from the original spec §7.4 carry over verbatim: the backend API is the only runtime write authority; workflows run strictly in the browser; every workflow-originated request carries `book_id`, `entity_id`, `workflow_id`, `workflow_deployment_id`, client-generated `workflow_execution_id`, and authenticated `user_id`, all re-verified server-side; every mutation carries a client-generated UUID handled idempotently (with `IDEMPOTENCY_CONFLICT` on payload mismatch); no capability tokens — the backend re-checks context against server-side state.

### 6.6 Sample workflows

Carried over from the original spec §7.5 with these updates:

- **Bootstrap flow**: fresh install per 5.3 → `create_accounting_book` (which also creates the book's one entity, §2.9 — there is no separate `Add an entity` step, Impl Plan M7) or `Open book` for an existing book → `Adding a workflow`. Finding a workflow to run is itself bootstrapped the same way: the launcher's book picker (`list_my_books`, §6.5) needs no deployed artifact and no prior knowledge of a `book_id` — it is how a non-owner, role-assigned user reaches `workflows/mine` at all (Impl Plan M6, simplified in M7 once a book's `entity_id` no longer needs its own discovery step).
- **7.5.1 Recording startup expense** and **7.5.2 Manual bank transactions**: unchanged, except inputs reference resource types (not free-form units) and cross-unit entries carry recorded prices. Shipped in Phase 1 (Impl Plan M5, M8 — the latter as an AI-generated artifact, not hand-built).
- **7.5.3 EOP bank reconciliation** (Phase 2, Impl Plan M11, deferred): unchanged design; corrections are new reversal/adjusting entries; result recorded as an administrative event. Period close/reopen themselves are not blocked on this — they are already reachable directly through the backend API and MCP primitives (Impl Plan M2/M4/M8) without a dedicated browser workflow.
- **7.5.4 Add a sub-book** (Phase 2, Impl Plan M12, deferred): updated to require the child-owner choice and copy mode per 2.8; projections in-file, not JSON files.
- **7.5.5 / 7.5.6 Consolidation rules and runs** (Phase 2, Impl Plan M12, deferred): unchanged semantics, with pending-until-authorized behavior per 2.8.
- External brokerage import remains out of scope for phase 1.

## 7 Architecture & Operations

### 7.1 Topology and stack

**Backend: Rust end-to-end.** One Rust binary (Axum) is the routing server, authentication/authorization layer, runtime backend application server, and hosts the `AccountingEngine` in-process. It is the only component with storage access. It also serves the built frontend assets and deployed workflow artifacts, and exposes `GET /api/health` as an unauthenticated liveness endpoint (fuller observability arrives with hardening, M10).

**Frontend: self-contained React apps per workflow, plus a minimal launcher.** Each deployed workflow is a complete, standalone React app: its own bundle (including its own React copy), its own route, its own mount point — served from the artifact path. A minimal launcher app owns login, session, a book picker, and the workflow menu, and simply navigates to each workflow's route. The picker (`list_my_books`, §6.5) is itself a bootstrapped launcher capability in the same sense as `Open book`/`Adding a workflow` (§6.6) — built into the launcher, not a deployed `WorkflowDefinition` artifact — so any authorized user can reach their assigned workflows starting from nothing but sign-in, never needing to already know a `book_id` (Impl Plan M6). It is a *book* picker, not a book/entity picker: since M7 constrains every book to exactly one entity (§2.8), selecting a book already determines the entity — there is no second selection step. **There are no shared JavaScript dependencies between launcher and workflows** — no shared React instance, no import maps, no shell-provided context. Auth/session reaches workflows through the session cookie/token on backend API calls, not through frontend coupling. Deploying a workflow never rebuilds anything else. Cost: each workflow bundle carries React (~50 KB gzipped, cached after first load) — accepted as insignificant. Benefit: zero version-skew risk, and every deployed artifact is a complete, independently auditable app.

**MCP server + Python dev-time backend:** remain Python (the existing `mcp_server` package), owning MCP primitives, LLM access, prompt/context assembly, artifact preparation, and deployment support. **The `engine/` and `storage/` modules currently inside `mcp_server` are removed** — those responsibilities belong to the Rust engine. The Python side reaches accounting data only through runtime backend APIs.

Logical servers remain separable into different physical deployments and security zones later, as in the original spec §8.1. Deployment targets: on-premise local and Oracle Cloud VM; containerization deferred but not blocked.

### 7.2 Repository layout

```text
FirstPrincipleAccounting/
  Cargo.toml                # Rust workspace
  engine/                   # crate: AccountingEngine — invariants, domain, storage drivers
  backend/                  # crate: Axum server — routing, OAuth, APIs; depends on engine
  frontend/                 # minimal React + Vite launcher (login, session, workflow menu)
  mcp_server/               # Python MCP + dev-time backend (no engine/, no storage/)
  dev_artifacts/            # dev artifact store (7.4)
  docs/
  server.config.example.toml
```

### 7.3 Export and restore

As in the original spec §8.2, with v1 clarifications already decided there: encrypted JSON bundle with deployment references/hashes but not artifact bodies; restore is wipe-and-replace preserving internal IDs and `book_id`; workflows whose artifacts can't be matched by id+hash are marked unavailable until restored/redeployed; restored books are live operational starting points; "reconciled before export" is a no-op because the engine never accepts an unbalanced state.

### 7.4 Dev-time artifact store

Unchanged from the original spec §8.4: separate from book storage; integrity-first; immutable artifacts; hashes are identity, paths are locators; inspectable by authorized users; no private accounting context in artifacts, prompts, traces, or logs; layout `dev_artifacts/workflows/<workflow_deployment_id>/{workflow.json, manifest.json, code/, signatures/}`.

### 7.5 Testing strategy

Required from the first commit:

- **Property tests** (proptest) on the engine: any generated entry either posts with debits == credits at its recorded prices or is rejected; no sequence of operations can produce an unbalanced chart.
- **Replay tests**: rebuild all projections from the event log and compare to incrementally maintained state after every test scenario.
- **Idempotency tests**: every mutation replayed with identical payload returns the original outcome; with mutated payload returns IDEMPOTENCY_CONFLICT.
- **Crash-safety tests**: kill during atomic file replacement; reload must yield the pre-mutation state.
- **API integration tests** against the Axum boundary for authorization and execution-context checks.

Generated workflow code is held to the same discipline: its backend calls are covered by the API tests, and it can never bypass them.

---

## Appendix A — Resolution Log

Gap references are to `LedgerZero_Spec_Gap_Analysis.md`.

| Gap | Resolution |
|----|----|
| C1 `ledger.jsonl` vs single file | Single `book.data.enc`; JSONL is logical record format only (3.1) |
| C2 projection files | No projection files; book links and consolidation rules persisted inside the book file (2.8, 3.1) |
| C3 sub-book role copying | Owner-dependent: same owner → creator chooses copy all / none / owner-only; different owner → copy none; consolidation pending until child grants read (2.8) |
| C4 git diff claim | Corrected to backup/history only; ownership-transfer history caveat documented (3.2, 3.3) |
| C5 approval fields vs model | `status` and `approved_by` removed; approval is pure workflow (2.4) |
| C6 per-workflow auto-roles | **Kept**: deployment auto-creates same-named role (2.9) |
| B1 entry state machine | Posted-or-rejected; no DRAFT/PENDING_APPROVAL/VOIDED; corrections via reversal (2.4) |
| B2 price tables | Entries record their own prices and must balance exactly at them; PRICE events + in-memory price projection for defaults (2.6) |
| B3 unit of measure | New `ResourceType` entity, exactly one unit each; accounts reference it (2.1) |
| B4 line semantics | Amounts always in account's unit; `quantity`/`unit_value` removed (2.5) |
| B5 chart entity | Chart table defined; one active chart per entity; copy = new IDs with source metadata (2.2) |
| B6 period schema/APIs | Full schema; non-overlap rule; close/reopen APIs as PERIOD_STATUS events (2.7, 6.5) |
| B7 equation & close | Expanded equation A = L + E + (R − X); year-end close deferred (1) |
| B8 key mechanics | Passphrase → Argon2id → AES-256-GCM wrapped book key; export same construction (5.4) |
| B9 bootstrap | Plaintext server config outside books with OAuth config + `bootstrap_owner_email` (5.3) |
| B10 reporting | **No built-in reports**; all reports are workflows over read APIs; tools TBD (6.3) |
| B11 API deltas | reverse_entry, deactivate_account, update_account_metadata, get_audit_log, close/reopen_period, assign_workflow_to_role, record_price/list_prices, export/restore added (6.5) |
| B12 idempotency conflict | Same ID + different payload → IDEMPOTENCY_CONFLICT (4.1) |
| B13 stack & repo | Rust end-to-end (Axum, engine in-process); each workflow a self-contained React app with a minimal launcher, no shared frontend dependencies; Python keeps MCP/dev-time only, engine/ and storage/ removed from mcp_server; workspace layout fixed (7.1, 7.2) |
| B14 error model | Structured error catalog (4.4) |
| B15 testing | Property/replay/idempotency/crash/API tests required (7.5) |
| D1–D8 deferrals | Cross-server consolidation auth, book-level FX translation, consolidation scheduling beyond on-demand `run_consolidation`, containerization, year-end close, reporting tools, re-open-by-branching, brokerage import — all explicitly deferred |

Post-implementation resolutions (found after M0–M8 were built, not in the original gap analysis):

| # | Resolution |
|----|----|
| R1 | Multi-entity-per-book removed (Impl Plan M7): a book's encryption key and owner authority previously could span several legally distinct entities, which is a real security-boundary crossing, not just an implementation convenience — a key compromise or an owner's blanket authority reached every entity sharing the book. Every book now has exactly one entity, auto-created with `create_accounting_book`; `create_entity` is retired as a client-facing operation. Related legal entities (holding company/subsidiaries, one bookkeeper's several clients) already had a correct mechanism — separate books linked via `create_sub_book` and combined only through read-only, idempotent consolidation — so this removes a second, weaker path to the same outcome, not a capability (2.8, 2.9, 6.5, 6.6, 7.1). Also simplified the M6 launcher picker to a single book-selection step, since selecting a book now already determines its one entity.|
| R2 | Delivery split into two phases after M8 shipped (raised by the user): Phase 1 (Impl Plan M1–M10) is what the user starts running real books on; Phase 2 (Impl Plan M11 — periods-in-practice/reconciliation-as-workflow, and M12 — sub-books/consolidation) is deferred until Phase 1 is in real use, not dropped from scope. Verified no Phase 1 milestone depends on Phase 2: period close/reopen and closed-period rejection are already engine/backend/MCP-level capabilities since M2/M4/M8 (reachable directly, without a dedicated browser workflow); export/restore and hardening both operate on one book regardless of whether sub-books ever exist. The originally-numbered M9 (periods/reconciliation) and M11 (sub-books/consolidation) — the two deferred to Phase 2 — were renumbered to M11 and M12; the originally-numbered M10 (export/restore) and M12 (hardening) — both still Phase 1 — moved up to M9 and M10, so Phase 1 reads as a clean, contiguous M1–M10. Every cross-reference elsewhere (this document, `mcp_server/`, `docs/LedgerZero_Manual_Verification.md`) was swept to match (2.8, 6.4, 6.5, 6.6, and the "Delivery phases" note above §1).|
