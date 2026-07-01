# LEDGER ZERO

A First-Principles Accounting Platform

Product Specification & Project Plan

*Working Draft*

*Classification: Confidential*

## Table of Contents

- [1 Vision & Guiding Principles](#1-vision--guiding-principles)
  - [1.1 The Core Problem](#11-the-core-problem)
  - [1.2 Design Axioms](#12-design-axioms)
  - [1.3 What This Is Not](#13-what-this-is-not)
- [2 Core Data Model](#2-core-data-model)
  - [2.1 The Three Primitives](#21-the-three-primitives)
  - [2.2 Supporting Entities](#22-supporting-entities)
- [3 Storage Abstraction Layer](#3-storage-abstraction-layer)
  - [3.1 Storage Interface Contract](#31-storage-interface-contract)
  - [3.2 Driver Implementations](#32-driver-implementations)
  - [3.3 First-Implementation File Layout](#33-first-implementation-file-layout)
- [4 Ledger Engine](#4-ledger-engine)
  - [4.1 Engine Responsibilities and Deployment Modes](#41-engine-responsibilities-and-deployment-modes)
  - [4.2 Invariant Enforcement](#42-invariant-enforcement)
  - [4.3 Balance Computation](#43-balance-computation)
- [5 Chart of Accounts Engine](#5-chart-of-accounts-engine)
  - [5.1 What the Chart of Accounts Is](#51-what-the-chart-of-accounts-is)
  - [5.2 Redefining the Chart of Accounts](#52-redefining-the-chart-of-accounts)
  - [5.3 Multiple Charts of Accounts](#53-multiple-charts-of-accounts)
- [6 Role & Security Model](#6-role--security-model)
  - [6.1 Role Model](#61-role-model)
  - [6.2 SOX Control Mapping](#62-sox-control-mapping)
  - [6.3 Authentication & Authorization](#63-authentication--authorization)
  - [6.4 Encryption and Key Ownership](#64-encryption-and-key-ownership)
  - [6.5 Runtime Application Security](#65-runtime-application-security)
- [7 Workflow Generation Engine](#7-workflow-generation-engine)
  - [7.1 Workflow Lifecycle](#71-workflow-lifecycle)
  - [7.2 AI-Generated Workflow Boundaries](#72-ai-generated-workflow-boundaries)
  - [7.3 First-Implementation MCP Primitives](#73-first-implementation-mcp-primitives)
  - [7.4 First-Implementation Backend Application API](#74-first-implementation-backend-application-api)
  - [7.5 Sample Workflows](#75-sample-workflows)
- [8 Non-Functional & Architecture Specification](#8-non-functional--architecture-specification)
  - [8.1 First Implementation Topology](#81-first-implementation-topology)
  - [8.2 AccountingBook Export and Restore](#82-accountingbook-export-and-restore)
  - [8.3 First-Implementation Frontend Contract](#83-first-implementation-frontend-contract)
  - [8.4 Dev-Time Artifact Storage](#84-dev-time-artifact-storage)

## 1 Vision & Guiding Principles

Ledger Zero is a production-grade, AI-native accounting platform built entirely from accounting first principles. It is not a reimagining of existing ERP software. It is a ground-up reconstruction starting from the only axiom that matters:

**Assets = Liabilities + Owner's Equity**

Every feature, every data structure, every workflow, and every security control is a logical derivation from that axiom. Nothing else is assumed.

### 1.1 The Core Problem

Legacy ERP systems (Oracle, SAP, NetSuite) did not fail because accounting is complex. Accounting data is structurally simple: three entities, one invariant. They failed because application complexity became entangled with accounting logic. Workflows were hardcoded. Charts of accounts became structural constraints rather than logical classifications. Compliance was documented around rather than enforced by design. Businesses bent themselves to fit the software, then became trapped by their own customizations.

The result is systems too dangerous to change, surrounded by shadow spreadsheets, offshore reconciliation teams, and multi-million-dollar consultant engagements that produce more complexity, not less.

### 1.2 Design Axioms

The following axioms are non-negotiable. Every architectural decision must be traceable to one or more of them.

- AXIOM 1 — The ledger is immutable. Transactions are appended, never mutated. Correction is a new event, not an edit.

- AXIOM 2 — The double-entry invariant is enforced at the data layer. No transaction posts unless debits equal credits. This is a constraint, not a validation rule.

- AXIOM 3 — Each chart of accounts is a self-consistent account structure. Adding or changing a chart does not mutate historical ledger events; required redistribution is handled by explicit rules, tags, workflow inputs, or derived transactions.

- AXIOM 4 — Currency is a resource type. All resources — including currency, inventory, commodities, and digital assets — share the same primitive. Currency is distinguished only by universal exchangeability.  An account must have a resource type and a resource type must have one and only one unit of measure.  

- AXIOM 5 — Storage is abstracted. The ledger engine never speaks to a database directly. It speaks to a storage interface. The database is a driver. 

- AXIOM 6 — Workflows are generated, not hardcoded. Generated application code become part of the application and can be audited the same way as the core code. Workflows are defined by natural language, compiled to executable definitions, and stored as data.

- AXIOM 7 — Security is structural. Role permissions are enforced at the data layer. A clerk cannot see controller data because it is architecturally unreachable, not because a UI hides it.

- AXIOM 8 — Audit trail is a consequence of architecture, not a feature. Because the ledger is immutable and all state changes are events, a complete audit trail exists by default with no additional implementation.

- AXIOM 9 — The system is maintainable by a person who understands accounting. No proprietary knowledge of the platform is required to operate, audit, or extend it.

- AXIOM 10 — Migrations are workflows. Scaling to a larger database, adding entities, or reorganizing the chart of accounts are first-class operations with turnkey execution, not engineering projects.

- AXIOM 11 — Start small without architectural regret. The system must be able to begin as a single-user, single-workflow, local deployment and grow through clean storage and deployment transitions into larger installations without requiring a fundamentally different design.

- AXIOM 12 — Dev-time artifacts do not retain private accounting context. Private accounting context is either not used, replaced with synthetic/redacted examples, or used transiently through authorized runtime APIs and not saved in the dev artifact store.

- AXIOM 13 — Backend mutation APIs are idempotent. Client-generated UUIDs (for transactions, events, and workflow executions) are used to detect duplicate submissions and ensure that retried operations are ignored or return the existing transaction outcome without duplicate side effects.

### 1.3 What This Is Not

- Not a SaaS product with per-seat licensing — it is a deployable system the client owns

- Not a wrapper around an existing accounting engine — it is a clean-room implementation

- Not dependent on any specific cloud vendor — it runs anywhere a container runs

- Not a system that requires a programmer to maintain — it requires an accountant and an AI coding tool

## 2 Core Data Model

The entire accounting data model derives from three entities and one constraint. This section defines those entities precisely. All other data structures in the system are projections over these three.

### 2.1 The Three Primitives

#### 2.1.1 Account

An account is a named classification of economic value. It has a type that determines its normal balance and its position in the accounting equation.

|  |  |  |  |
|----|----|----|----|
| **Field** | **Type** | **Description** | **Constraints** |
| account_id | UUID | Immutable unique identifier | System-generated, never reused |
| chart_id | UUID | Owning chart of accounts | Required |
| name | String | Human-readable account name | Required, unique within entity |
| code | String | Alphanumeric chart code | Optional, user-defined |
| account_type | Enum | ASSET, LIABILITY, EQUITY, REVENUE, EXPENSE | Required, immutable after creation |
| normal_balance | Enum | DEBIT or CREDIT | Derived from account_type, enforced by system |
| resource_type | Enum | CURRENCY, INVENTORY, COMMODITY, DIGITAL_ASSET, OTHER | Required — implements Axiom 4 |
| currency_code | String | ISO 4217 or custom code | Required if resource_type = CURRENCY |
| parent_account_id | UUID | Optional parent for hierarchy | Self-referential FK to account |
| entity_id | UUID | Owning entity | Required for multi-entity support |
| is_active | Boolean | Soft deactivation flag | Inactive accounts reject new transactions |
| created_at | Timestamp | Creation time | Immutable |
| metadata | JSON | Arbitrary extensible fields | Schema-free, indexed |

> *Normal balance is always derived, never stored independently. ASSET and EXPENSE accounts have DEBIT normal balance. LIABILITY, EQUITY, and REVENUE accounts have CREDIT normal balance. This derivation is enforced by the engine, not trusted from input.*

#### 2.1.2 Journal Entry

A journal entry is an immutable record of an economic event. It is the unit of truth in the ledger.

|  |  |  |  |
|----|----|----|----|
| **Field** | **Type** | **Description** | **Constraints** |
| entry_id | UUID | Immutable unique identifier | Client-generated for idempotency, verified unique by engine |
| entity_id | UUID | Owning entity | Required |
| entry_date | Date | Economic date of the transaction | Required — may differ from posted_at |
| posted_at | Timestamp | System timestamp of posting | Immutable, system-set |
| posted_by | UUID | User who posted | FK to user, immutable |
| approved_by | UUID | User who approved | Nullable, FK to user |
| workflow_id | UUID | Originating workflow | Nullable — null for manual entries |
| workflow_deployment_id | UUID | Exact deployed workflow artifact used | Nullable — set when source = WORKFLOW |
| event_type | Enum | ACCOUNTING, ADMINISTRATIVE, WORKFLOW_DEPLOYMENT, ROLE_ASSIGNMENT, SUB_BOOK_LINK, CONSOLIDATION_RULE, CONSOLIDATION, SYSTEM_DERIVED, RESTORE | Required |
| description | String | Human-readable memo | Required |
| status | Enum | DRAFT, PENDING_APPROVAL, POSTED, VOIDED | State machine — see Section 4 |
| reversal_of | UUID | Entry this reverses | Nullable — implements correction without mutation |
| period_id | UUID | Accounting period | FK to period, enforced on post |
| source | Enum | MANUAL, WORKFLOW, DERIVED, ADMIN, RESTORE, SYSTEM | Audit classification |
| metadata | JSON | Arbitrary extensible fields | Schema-free |

The single ledger file records both accounting transactions and administrative events. Accounting transactions have journal lines and must satisfy the double-entry invariant. Administrative events, workflow deployments, role assignments, sub-book links, consolidation rules, restore markers, and system-derived events are still immutable ledger events, but they do not affect balances unless they create accounting journal lines.

In storage, each `ledger.jsonl` record is a ledger event envelope. The minimum envelope includes a client-generated `event_id`, `book_id`, `event_type`, `occurred_at`, `actor_user_id`, optional `workflow_id`, optional `workflow_deployment_id`, client-generated `workflow_execution_id`, and an event-specific payload. Because all event, transaction, and workflow execution IDs are client-generated, they serve as unique keys to prevent duplicates, and a separate `idempotency_key` is not required. For `ACCOUNTING` events, the payload is the journal entry and journal lines. For administrative events, the payload is the role, workflow, book-link, consolidation, restore, or system-change data needed to rebuild projections.

#### 2.1.3 Journal Line

A journal line is one side of a double-entry transaction. Every posted journal entry must have at least two lines whose debits and credits balance according to the units and conversion rules applicable to the affected accounts. This is the enforcement point of the accounting equation.

|  |  |  |  |
|----|----|----|----|
| **Field** | **Type** | **Description** | **Constraints** |
| line_id | UUID | Immutable unique identifier | Client-generated, unique within the journal entry |
| entry_id | UUID | Parent journal entry | FK to journal_entry, immutable |
| account_id | UUID | Account being affected | FK to account, must be active |
| debit_amount | Decimal(18,8) | Debit amount in account's resource unit | \>=0, null if credit side |
| credit_amount | Decimal(18,8) | Credit amount in account's resource unit | \>=0, null if debit side |
| quantity | Decimal(18,8) | Non-currency resource quantity | Nullable, for inventory/commodity |
| unit_value | Decimal(18,8) | Value per unit at transaction time | Nullable, for inventory/commodity |
| memo | String | Line-level description | Optional |
| metadata | JSON | Arbitrary extensible fields | Schema-free |

> *The double-entry invariant: debits and credits must balance before a journal entry transitions to POSTED. Lines may reference accounts with different units of measure, but each line's unit must match the unit of its account. Cross-unit entries must balance according to the applicable transaction-time price from the price tables (debit_amount == credit_amount * price). Price changes are recorded as transactions in the ledger. This constraint is enforced by the Ledger Engine and cannot be bypassed by any workflow, user, or API call.*

### 2.2 Supporting Entities

The following entities support the three primitives. They do not contain accounting data — they provide classification, context, and operational structure.

#### 2.2.0 AccountingBook

An `AccountingBook` is the top-level operational container for one set of ledger data. It owns the storage folder, contains one or more entities, and is the unit that can be exported or restored.

For the first implementation:

- one `AccountingBook` has a stable `book_id`
- one `AccountingBook` may optionally have a `parent_book_id`
- one `AccountingBook` maps to one storage folder
- one `AccountingBook` has one owner role at bootstrap
- the owner role is initially held by one user
- the current owner may transfer the owner role to another user, but loses that owner authority after the transfer
- ownership transfer requires re-encrypting the book for the new owner
- entities, roles, workflows, accounts, and entries all belong to exactly one `AccountingBook`
- `AccountingBook` is the storage, export, restore, and bootstrap security boundary
- `Entity` remains the accounting and reporting boundary inside the book

#### 2.2.0.1 Sub-Ledger / Sub-Book

A sub-ledger or sub-book is an `AccountingBook` whose `parent_book_id` points to another `AccountingBook`. It remains a separate, fully completed, first-class book with its own storage folder/environment, owner, chart of accounts, roles, workflows, ledger, encryption key, and authorization boundary, and it can run remotely or on external systems.

For the first implementation:

- creating a sub-ledger creates a new child `AccountingBook` with its own independent storage and endpoints
- the child book may have a different owner from the parent book
- the child book may have a different chart of accounts from the parent book
- parent book settings and all authorizations/role assignments are copied into the child book at creation time as defaults, ensuring consolidation read APIs and parent workflows function immediately
- user authentication is managed via Google Login, which verifies the identity against the copied/assigned authorizations in the target book
- if the child book is subsequently run on another server, accessing it will trigger a normal Google Login authentication flow against that server and the child book's copied authorizations
- the parent/sub-book relationship is recorded as immutable ledger events in both books
- runtime authority does not automatically cross between parent and child books unless authorized in both books
- when child book ownership changes, the parent owner intentionally loses any privileges that depended on the prior child owner or prior child-book role assignments
- parent access to child activity happens through consolidation workflows that start from the parent book and make read API calls to the child book API
- consolidation is dependent on the child book being open; if the child book is not open (i.e. the key has not been loaded into the child server's memory), the read API call and consolidation will fail
- by default, all posted child-book transactions are consolidated to the parent book when parent and child chart/account mapping is resolvable
- the parent book may define consolidation rules to filter, summarize, transform, or map child transactions into the parent chart of accounts
- if default consolidation fails because of child ownership, authorization, or chart-mapping changes, the failure is highlighted for parent operators and resolved operationally with the child operator
- if the child chart differs and no mapping exists, consolidation remains pending until a parent consolidation rule defines the mapping, such as consolidating into a summary account

#### 2.2.1 Entity (Legal Entity / Business Unit)

Supports multi-entity deployments from day one. A managed accounting entity has its own chart of accounts, periods, and balances. Intercompany activity is represented by separate transactions rather than one journal entry that spans books.

An `Entity` may also represent an external legal or business party referenced by a transaction. The word "counterparty" is only a contextual label for the other entity in a particular transaction, not a separate domain type.

Managed entities participate in the book's role and authorization model. External referenced entities do not receive books, roles, accounts, or ownership semantics unless they are explicitly promoted into managed accounting entities within an `AccountingBook`.

Intercompany or counterparty activity is not represented as one journal entry spanning books. It is represented as separate transactions in each relevant `AccountingBook`, linked by metadata when useful. Within the current book, the counterparty is an identifier and selection criterion for export, input, matching, or reporting.

#### 2.2.2 Accounting Period

A period defines an open/closed time window for posting. Closing a period is itself recorded as a transaction. Re-opening a period is another transaction. Re-opening-by-branching or re-opening-by-reversal is still under design and is deferred until later.

#### 2.2.3 User

A user is a human principal with a role. Roles are scoped to entities. A user may have different roles in different entities. A user with different roles must choose the entity in which they are working. Each workflow a user is authorized to use for an entity is explicitly authorized.

Assigning a role to an entity is itself a workflow and is logged and auditable. At bootstrap, an `AccountingBook` has one owner. The owner's initial authority is to assign roles to users, including to themself.

#### 2.2.4 Role

A role is always defined within an entity and is made up of a set of workflows. A role has a name, a description, and a list of workflows authorized for that role.

#### 2.2.5 Workflow Definition

A workflow definition is a compiled description of a business process. It is generated by AI from natural language and stored as structured data and deployed code in the dev artifact store. When deployed, it is associated with one or more roles. The accounting ledger records deployment events, hashes, and references, but generated workflow artifacts are not stored inside the accounting book storage folder.

For the first implementation, the minimum `WorkflowDefinition` contract is:

| Field | Description |
|----|----|
| workflow_name | Unique workflow name within an entity |
| workflow_deployment_id | Immutable deployment identifier for audit |
| artifact_id | Immutable dev artifact store identifier |
| description | Optional human-readable explanation |
| entity_id | Owning entity |
| roles | Roles authorized to use the workflow |
| steps | One or more ordered steps |
| dev_artifact_path | Saved copy of the deployed workflow artifact in the dev artifact store |
| manifest_hash | Hash of the deployed workflow artifact manifest |
| code_hash | Hash of the deployed workflow code artifact |
| frontend_route | Route or artifact path served to the frontend |
| backend_api_calls | Backend operations the workflow may invoke |
| required_inputs | User-provided inputs required by the workflow |
| metadata | Extensible implementation-specific metadata |

Each step is either:

- a REST application with a form where the user fills in information and submits it through an API call, or
- an API call if no user-provided information is needed.

There is no user-facing workflow versioning or workflow status in the first implementation. A workflow name points to the currently deployed workflow artifact.

Internally, every deployment is retained as an immutable deployment record with at least `workflow_deployment_id`, `artifact_id`, `manifest_hash`, `code_hash`, `deployed_by`, and `deployed_at`. Replacing a workflow creates a new deployment record and moves the workflow name to that deployment. Historical journal entries keep the exact deployment id used, so generated code remains auditable even though users do not manage workflow versions.

#### 2.2.6 Transaction Party Naming

There is no separate `Counterparty` object in the first implementation. A sale, purchase, receipt, or accrual may refer to another `Entity`, and that entity may be called a counterparty, vendor, customer, broker, bank, or another workflow-specific name for clarity.

For the first implementation:

- bank accounts are represented as ordinary asset accounts until a stronger distinction is required
- source documents and external references are stored as metadata
- bank statement lines and reconciliation matches are deferred until the reconciliation workflow needs them explicitly

Each workflow has a name. Sample workflow names include:
- Sale
- Acquire
- Accrue
- Receive 

## 3 Storage Abstraction Layer

The ledger engine never calls a database directly. All persistence is mediated through a storage interface. This is the architectural enforcement of Axiom 5.

The first implementation will use a single encrypted book data file (`book.data.enc`) in a git-backed working directory. After each workflow execution/mutation completes, the entire book state (ledger events, accounts, roles, entities, users, and rules) is encrypted and written back to this single file ($O(N)$ write complexity, which is highly performant for standard accounting datasets). All data is loaded into memory at startup. There is no append-only file writing on disk; the directory is tracked externally using git for version control and backup. Relational databases or append-only event streams are treated as later storage drivers and scale optimizations if write speed eventually becomes a bottleneck, not as part of the conceptual model.

The storage boundary is strict: only the runtime backend application server acting through the Rust `AccountingEngine` may open, read, or write accounting storage. MCP, the Python dev-time backend server, web UI code, batch tooling, and workflow orchestration must call runtime backend application APIs. They never touch ledger files directly.

### 3.1 Storage Interface Contract

The storage interface exposes the following operations. Any compliant driver must implement all of them. The ledger engine depends on the interface, not on any driver.

All operations in this contract are asynchronous (`async`) to allow database-backed drivers (Postgres, SQLite, DynamoDB) to be plugged in later without changing the engine interface.

|  |  |  |
|----|----|----|
| **Operation** | **Description** | **Atomicity Required** |
| append_entry(entry, lines) | Write a journal entry and its lines atomically | Yes — all-or-nothing |
| append_event(event) | Write a non-balance-affecting administrative, workflow, role, sub-book, consolidation rule, restore, or system event | Yes — all-or-nothing |
| get_entry(entry_id) | Retrieve a single entry with its lines | Read-only |
| query_entries(filters) | Filtered query over entries | Read-only |
| get_balance(account_id, as_of) | Compute running balance for an account at a point in time | Read-only |
| append_account(account) | Create a new account record | Yes |
| update_account_metadata(account_id, metadata) | Update non-structural account fields | Yes |
| get_account(account_id) | Retrieve account record | Read-only |
| query_accounts(filters) | Filtered account query | Read-only |
| get_period(period_id) | Retrieve period record | Read-only |
| set_period_status(period_id, status, user_id) | Open or close a period | Yes — creates audit event |
| get_audit_log(entity_id, from, to) | Retrieve all events for an entity in a time range | Read-only |
| load_snapshot(snapshot_id) | Load a previously materialized balance or index snapshot | Read-only |
| save_snapshot(snapshot) | Persist a read-optimization snapshot | Yes |
| acquire_writer_lock() | Ensure only one writer mutates authoritative storage at a time | Yes |
| release_writer_lock() | Release the authoritative writer lock | Yes |

Any operation that mutates book state rewrites the single encrypted book file. There are no separate projection files on disk; all projections are maintained in memory and written as part of the single consistent encrypted state.

Storage drivers must support idempotent mutation by verifying the uniqueness of client-provided IDs (like `entry_id` or `event_id`) before applying a transaction. If the ID is already present, the storage driver returns the existing transaction state or ignores the mutation. For the file-backed implementation, this is checked against the loaded in-memory index of processed IDs before the updated state is written back to the single encrypted file.

### 3.2 Driver Implementations

|  |  |  |  |
|----|----|----|----|
| **Driver** | **Use Case** | **Deployment Size** | **Notes** |
| File Event Store (Git-backed) | Default starter | Micro to small (\< 10 users) | JSON/JSONL files, append-only event stream, easy diff/backup/history |
| SQLite | Second-stage local scale | Micro to small (\< 50 users) | Single file, zero config, faster queries than raw file replay |
| PostgreSQL | Production scale | Small to mid-market (50-500+ users) | Requires separate DB server or managed service |
| DynamoDB | High-volume cloud | Large scale, AWS-native deployments | NoSQL — projections stored as materialized views |
| MongoDB | Flexible schema cloud | Mid-market with heavy metadata usage | Document model maps well to metadata-rich entries |

> *Performance is the primary difference between drivers. The accounting invariants, audit trail completeness, and security model are intended to remain identical across all drivers. Swapping drivers is a configuration change followed by an export/restore or migration workflow.*

### 3.3 First-Implementation File Layout

For the first implementation, one `AccountingBook` is stored in one folder named after the book.

The minimum accounting book storage layout is:

```text
<book_name>/
  book.data.enc
  book.keystore.json
  export/
```

The intent of each artifact is:

- `book.data.enc`
  - the single encrypted file containing the complete serialized state of the book (including all ledger transactions, administrative events, accounts, entities, roles, and users). It is the sole source of truth for book state
- `book.keystore.json`
  - book-local keystore file containing encrypted or wrapped book-key material and key metadata; it must not contain plaintext book keys or owner keyphrases
- `export/`
  - temporary or retained export artifacts

The storage layer is responsible for serialization. The engine works with objects and does not manipulate raw file formats directly.

The single `book.data.enc` file is loaded into memory on startup, and all in-memory indexes and account balance projections are built from it. Dev-time workflow artifacts are referenced by hash and metadata but are stored separately in the dev artifact store.

For the first implementation:

- writing out the entire encrypted state file is the default rule for persistence
- `book.data.enc` is the only source of truth for mutable book state
- the storage layer is responsible for atomic file replacement when rewriting the book file (e.g., write to temporary file, then rename to prevent corruption on crash)
- one authoritative writer holds the writer lock during mutations
- recovery from partial failure is managed via external git checkpoints; any corrupt write can be rolled back to the last clean git commit
- if a single book becomes too expensive to rewrite at scale, the preferred scaling direction is to split activity into multiple `AccountingBook` instances and reconcile interactions between books as separate transactions, similar in spirit to distributed git repositories

## 4 Ledger Engine

The ledger engine is the core of the system. It enforces all accounting invariants, manages entry state transitions, and is the only component authorized to write to the storage layer. Nothing else writes to the ledger directly.

For the first implementation, the ledger engine is the `AccountingEngine`, and Rust is its primary implementation language. Rust owns the invariant-enforcing domain logic, the posting state transitions, the storage writer boundary, and any accounting rule that must not be bypassed by generated code, frontend code, MCP tools, or Python development-time services.

The engine is deployment-agnostic. The same engine code may be invoked in-process by the backend application server, executed as a separate local worker behind the backend application boundary, run as a directory-watching worker, consume a queue, or later be exposed as a service. These are deployment modes of the same engine, not different engines.

In the initial deployment, the engine is expected to run with a single authoritative writer against file-backed storage. Read paths may be concurrent, but writes are serialized through the engine's writer lock.

### 4.1 Engine Responsibilities and Deployment Modes

The accounting engine is responsible for:

- receiving domain objects from the backend application server
- enforcing posting invariants before ledger mutation
- serializing write access through the storage-layer writer lock
- invoking account-defined validation rules when present
- appending successful ledger activity to the authoritative ledger
- exposing balances and account views for query paths

The engine validates only rules that can be defined by the account or by the core posting rules. If no extra validation rule is defined for an account, the engine performs only the default platform checks.

There are two transaction modes in the first implementation:

- realtime transaction
  - posting to all affected accounts must have ACID-like behavior from the engine's perspective
  - this mode is reserved for transactions where synchronized consistency is required, such as inventory movement for physical goods
- asynchronous transaction
  - a request or control event may be accepted first, but an accounting transaction is marked POSTED only after it is actually posted
  - eventual consistency is represented by derived transactions appended to the ledger
  - this mode is preferred where immediate synchronized posting would add cost without business need

Synchronized consistency is intentionally selective because global synchronization is expensive and can become a scalability bottleneck. The invariant for asynchronous work is final ledger consistency: after all derived transactions have been appended and processing completes, the ledger and all account projections must reconcile.

At larger scale, asynchronous processing may become a queueing system with queues and subqueues. In that mode, the ledger remains the durable persistence layer for queue state and derived accounting outcomes.

Consolidation from a sub-book to a parent book is handled as derived parent-book activity. The child ledger is not mutated by parent consolidation. The parent ledger records the consolidation event or derived accounting entry with enough metadata to identify the child book, child entry, and consolidation rule. Consolidation must be idempotent.

Supported first-implementation deployment modes are:

- on-premise local deployment
- Oracle Cloud VM deployment

The architecture should avoid blocking a later containerized deployment, but containerization is explicitly deferred.

### 4.2 Invariant Enforcement

The following checks are performed by the ledger engine on every transition to POSTED. Any failure rejects the transition and returns a structured error.

1.  Double-entry balance check: debits and credits must balance. If the accounts involved have different units of measure, they must balance using the price map from the price tables at the transaction time (debit_amount == credit_amount * price). Price changes are recorded as transactions in the ledger.

2.  Account validity: all referenced accounts exist, belong to the same chart and entity within one `AccountingBook`, and are active

3.  Period check: entry_date falls within an open accounting period

4.  Resource type consistency: lines referencing non-currency accounts include quantity and unit_value

5.  Authorization check: posted_by user is authorized to execute the workflow or manual workflow path that posts the entry within the current `AccountingBook`

6.  Approval check: if approval is required, approval is handled as a separate workflow step before the target posting is finalized. Approval is not a special engine-only concept in v1; it is modeled through ordinary workflow behavior.

### 4.3 Balance Computation

Balances may be stored, but they can always be computed from the immutable event log. This means a balance is always correct by construction and reflects the full history of the ledger when audited.

For performance at scale, the storage driver may implement materialized balance snapshots. These are treated as read-optimization caches, never as authoritative values. The authoritative balance is always the sum of journal lines. When a period closes, a full snapshot of all accounts may be created. This snapshot can be used as the starting point for moving or exporting the entity's book.

For the first file-backed implementation, snapshots and exports that need a stable accounting cut use the storage writer lock plus a ledger marker. Transactions before the marker must be fully processed before the cut is materialized. Transactions after the marker wait until the snapshot or export finishes.

## 5 Chart of Accounts Engine

The chart of accounts is a self-consistent account structure inside an `AccountingBook`. Different charts can exist, but they have different accounts. Each chart must independently preserve the accounting equation.

### 5.1 What the Chart of Accounts Is

The chart of accounts is a hierarchical set of accounts — codes, names, groupings, account types, and presentation order. It determines how transactions are classified and how balances are aggregated for reporting.

For every chart:

- accounts belong to exactly one chart
- each chart must be internally balanced as `Assets = Liabilities + Owner's Equity`
- each account has exactly one resource unit of measure
- each transaction line's unit of measure must match the unit of measure of the account it affects

### 5.2 Redefining the Chart of Accounts

Because different charts can have different accounts, adding or changing a chart may require distribution rules, tags, or additional workflow input fields. Those changes affect existing and future workflows and are handled at development time for the affected workflow definitions.


> *Account type (ASSET, LIABILITY, EQUITY, REVENUE, EXPENSE) is immutable after an account is created within a chart. Accounts can be divided into sub-accounts, and some sub-accounts may have associated accounting entities, subdivisions, or external entity references.*

### 5.3 Multiple Charts of Accounts

Every chart of accounts is named. Different charts of accounts can co-exist, and one should be able to copy a chart of accounts, give it a new name, and keep both.

When a new chart is added, the system does not assume a free projection from the old chart to the new one. If transactions must be distributed into the new chart differently, the required distribution rules, tags, or extra inputs must be added to the relevant workflows before those workflows post into the new chart.

For parent/sub-book consolidation, the parent book owns the consolidation rules. If a child book has a different chart of accounts, the parent consolidation rule is responsible for mapping, summarizing, or transforming child-book entries into the parent chart. If no custom rule exists, the default rule includes all child-book posted transactions in parent consolidation when the parent can resolve the child accounts into parent accounts.

The default consolidation rule can only post into the parent book when the parent can resolve the child accounts into parent accounts. If parent and child charts are identical or an identity mapping is valid, the default rule may consolidate all posted child transactions directly. If charts differ without a valid mapping, child activity is eligible for consolidation but must remain pending until a consolidation rule supplies the mapping.

## 6 Role & Security Model

Security in Ledger Zero is structural, not cosmetic. A user cannot access data outside their authorized workflows because the backend query and write paths enforce workflow scope at the data retrieval and mutation level — not by hiding UI elements.

### 6.1 Role Model

There is no role hierarchy.

In the first implementation:

- a role is a named collection of workflows
- the owner of the book holds the original authority to assign workflows to roles and roles to users
- creating roles, creating workflows, deploying workflows, assigning workflows to roles, and assigning roles to users are all explicit authorizations
- these authorizations may themselves be delegated through workflows, with complete traceability retained in the ledger and audit record
- authorization is workflow-scoped
- authorization flows from the book owner to roles to workflows to users
- workflow authorization implies authority to call the backend APIs needed by that workflow
- backend API calls are scoped to one `AccountingBook`
- there is no automatic crosswalk between books in the first implementation; books are physically separated
- a parent/sub-book relationship allows consolidation workflows, but does not grant parent users direct runtime authority inside the child book

### 6.2 SOX Control Mapping

The following SOX controls are enforced by design, not by documentation.  Some workflow involves more than one role that must be be executed by different users, that's all.

|  |  |
|----|----|
| **SOX Control** | **How Enforced in Ledger Zero** |
| Segregation of duties | When required, separation is enforced at role assignment time by assigning recording/posting and approval to separate workflows and separate users |
| Access control | Workflow scope is enforced at query and write boundaries — unauthorized data is structurally unreachable |
| Audit trail completeness | Immutable event log — every state change is recorded, nothing can be deleted |
| Period close integrity | Period close is a workflow requiring the relevant authorization — cannot be bypassed |
| Journal entry authorization | Every posted entry has a traceable posted_by and (where required) approved_by |
| Change management | Chart of accounts changes are ledger events with full history |

SOX segregation enforcement is deferred until a workflow requires it, but the architecture decision is made: separation is enforced through role assignment and workflow assignment, not by treating approval as a special ledger-engine concept.

### 6.3 Authentication & Authorization

Authentication and authorization are totally separated. The authenticated user is associated with an authorized user id and is granted roles within an entity. Which authentication channel is allowed is specified by the system admin through an OAuth2 trust setup. The system does not distinguish business authority by authentication mechanism.

- Authentication: OAuth2

- Session tokens: short-lived (1 hour), refresh token rotation enforced

- All API calls carry a user identity claim verified against the role store

- Failed authorization attempts are logged as audit events in an operational audit trail that is separate from the accounting operations audit trail.

- No shared credentials — every human actor has a unique user_id,  En-personating will be a role itself.
- First Authentication Authority will be Google Login.
- When another Authentication Authority is added, the system creates an AKA table linking multiple authenticated identities to the same authorized user id.
- A user may merge their own authentication identities by proving control of both identities. For example, a user logged in with Google may add an Apple identity, which triggers Apple login; if that succeeds, both identities map to the same user.
- When identities are merged by the user, authorizations combine because the system treats the merged identities as the same physical or legal person. The system only verifies control of trusted authentication identities; it does not try to adjudicate whether the user made a wise or externally correct identity claim.

### 6.4 Encryption and Key Ownership

Data will be saved encrypted. For the first file-backed implementation, encryption is handled in memory (the ledger content and projections are decrypted upon loading into memory). Stream-level encryption for the raw `ledger.jsonl` stream is deferred until that need arises.

For the first implementation:

- each `AccountingBook` has one active encryption key
- the active book key is represented through a `BookKeyProvider` contract rather than hardcoded key-loading logic
- the v1 `BookKeyProvider` reads a book-local `book.keystore.json` file saved with the book folder
- the book-local keystore may store encrypted or wrapped key material, key identifiers, and key metadata, but never plaintext book keys or owner keyphrases
- an owner-only bootstrapped `Open book` workflow loads the book key into backend memory before the encrypted book is read into memory
- once opened, the book key stays in backend memory for that backend process
- if the backend server restarts or the process loses memory, the owner must run `Open book` again before the book can be used
- after the book is loaded, in-memory access to account data is controlled by authorization, not by per-account encryption
- encryption at rest primarily protects offline storage, backups, and stolen disks; it does not by itself protect data after the backend has decrypted the book into memory
- book keys and keyphrases must not be stored in application code, generated workflow code, or inside the ledger/book data itself
- key material is injected into backend memory only through the `BookKeyProvider`; later providers may use an operating-system keystore, KMS, HSM, environment secret manager, or other secret source without changing the ledger engine or book format
- frontend code, generated workflow code, and MCP tools must not receive raw encryption keys
- there is no key transfer between users
- when ownership is transferred, the book is re-encrypted for the new owner
- exports are encrypted for the intended reader of the export, usually the user who creates or receives the export
- an export contains readable data only for the user authorized to read that export

### 6.5 Runtime Application Security

Encryption is not the primary runtime security boundary. When the system is running, the backend necessarily has decrypted key material or decrypted book data in memory. Therefore, runtime safety depends on reducing application-level entry points and enforcing authorization and accounting invariants on every request.

For the first implementation:

- authentication proves who submitted a request; authorization decides whether that authenticated identity may execute the requested workflow or ledger change
- every request is treated as untrusted input, even when it comes from an authenticated user, a generated workflow, MCP, or an LLM-assisted path
- backend domain logic must re-check workflow authorization, book scope, closed periods, account validity, unit consistency, and double-entry balance before posting
- backend mutation APIs must be idempotent so browser retries, refreshes, double-clicks, network retries, or replayed workflow submissions cannot double-post or duplicate administrative state changes
- generated workflow code and frontend code may guide users and call backend APIs, but they are never trusted to enforce security or accounting correctness
- storage/database access code must avoid string-built commands and use parameterized or structured storage operations where applicable
- secrets, passphrases, and service credentials must not be committed to code or stored beside the book
- deployments exposed beyond a local/private machine should require MFA where the authentication provider supports it
- public or cloud deployments should prefer network restrictions such as IP allowlists, private networks, or equivalent ingress controls where practical

## 7 Workflow Generation Engine

### 7.1 Workflow Lifecycle

For any workflow, the lifecycle has three steps.
- If the workflow do not exist, it will be created by somebody with the authorization to create workflows.  
- The workflow will be assigned to roles.  Notice that any workflow always belong to a role with the same name.
- A user authorized to a workflow will be able to choose it and use it to carry it out.  One or many transactions maybe created as the result of carrying out a workflow.  All transactions will be created and recorded in the ledger this way.
- A user authorized to a workflow may or may not have access to all accounts the workflow may impact. Some of the accounts may require additional authorization steps. There is no concept of “higher authorization.” Approval of posting to an account needing approval is handled like other workflow activity, with the approval represented through ordinary transaction and workflow mechanics. When approval and recording/posting must be separated, they are modeled as separate workflows and enforced through role assignment to separate users.
- A workflow specifies what information is needed, how it is entered, and which parts are not AI-generated at runtime.

### 7.2 AI-Generated Workflow Boundaries

- A workflow will be AI generated.  It may 
    1. include a data entry form, 
    2. an integration step to read information from another system or to post to another system.
- In reality, when AI is asked to create a workflow, it may not be able to because it may not have the tools, skills, or resources to make a workflow work. In such a case, it is a legitimate answer from the AI to say that those tools, skills, and resources must be provided by the MCP server.
- `generate_workflow_definition` is an MCP primitive, not a normal runtime backend operation.
- The normal flow is: user request -> MCP server -> Python dev-time backend server -> context gathering from backend as needed -> LLM call -> generated code or generated workflow definition -> deployment into the application server.
- In the first implementation, development-time AI generation belongs to the MCP and Python dev-time backend side of the architecture. The runtime backend application server does not need direct production LLM access merely to support workflow authoring.
- The Python dev-time backend server owns workflow-generation related backend operations, including LLM wrapping, prompt/context assembly, generated artifact preparation, and deployment support for workflow definitions.
- The Python dev-time backend server does not receive accounting storage credentials. If it needs accounting context, it obtains that context through authorized runtime backend APIs.
- Private accounting context is either not used, replaced with synthetic/redacted examples, or used transiently through authorized runtime APIs and not saved in the dev artifact store.
- MCP owns the user-facing primitive and orchestration surface for workflow generation. It calls the Python dev-time backend server for workflow-generation backend work rather than embedding that responsibility in the runtime accounting backend.
- Explanatory and investigative AI functions such as `explain_reconciliation_issue` are also MCP primitives for now: MCP gathers runtime facts from the backend application server, sends those facts through the Python dev-time backend server when LLM access or generation infrastructure is needed, and returns an explanation.
- Direct backend-to-LLM runtime access is deferred until there is a real production requirement for AI features that must operate without MCP in the loop.
- Generated code is deployed by a user with the relevant authorization. For the first implementation, no separate approval or signing flow is required because the developer is the only user expected to hold that authority.
- Generated workflow code is treated as frontend application code and as an untrusted runtime client.
- Generated code runs in the user's browser with the user's own credentials and authorizations.
- Generated code may render forms, guide the workflow, and call authorized backend APIs.
- Generated code must not directly access accounting files, storage credentials, backend internals, encryption keys, or the ledger engine.
- Data submitted by generated code is treated as untrusted input; backend authorization checks and ledger invariants must still pass even when the authenticated user is authorized to run the workflow.
- Workflow generation, deployment, and monitoring should follow normal software development lifecycle discipline whether code is produced by a human or an agent. In the first implementation, the developer takes responsibility for the whole cycle; later versions may add CI/CD gates, approval/signing, or workflow definition tooling such as n8n.

### 7.3 First-Implementation MCP Primitives

The MCP primitive set for the first implementation is intentionally incremental. If implementing a workflow requires a missing primitive, that primitive should be added to MCP and then recorded in this section.

The minimum starting primitive set is:

- `generate_workflow_definition`
- `deploy_workflow_definition`
- `list_workflows`
- `get_workflow_definition`
- `create_role`
- `assign_workflow_to_role`
- `assign_role_to_user`
- `create_account`
- `create_accounting_book`
- `create_sub_book`
- `list_sub_books`
- `create_entity`
- `create_period`
- `define_consolidation_rule`
- `list_consolidation_rules`
- `run_consolidation`
- `explain_reconciliation_issue`

These are sufficient to bootstrap the sample workflows and to let MCP orchestrate first-implementation system evolution.

### 7.4 First-Implementation Backend Application API

The backend application API begins with authorization support and then grows workflow by workflow. The minimum initial backend surface is:

- authentication and authorization endpoints
- `open_book`
- `create_accounting_book`
- `create_sub_book`
- `list_sub_books`
- `create_entity`
- `create_account`
- `list_accounts`
- `post_entry`
- `get_balance`
- `list_entries`
- `create_period`
- `assign_role`
- `define_consolidation_rule`
- `list_consolidation_rules`
- `run_consolidation`

New backend APIs may be added one workflow at a time. The intended rule is that backend and MCP primitives grow only as needed to support real workflows.

The backend application API is the only runtime authority boundary for durable accounting writes. Workflows are client-side applications that run strictly in the user's browser (e.g., entering data, submitting data, requesting a report). The backend never executes or runs workflow code. Backend APIs represent stable accounting operations based on double-entry accounting principles and do not change much. MCP may generate workflows, gather context, and call backend APIs, but it must not bypass backend APIs to write to the engine or storage. Generated frontend workflow code has the same restriction. Even if the backend, MCP, and engine are initially deployed in one physical process or on one machine, runtime writes still cross the backend application API boundary.

Backend API authorization is derived from workflow authorization. A user is authorized to execute a workflow, and that workflow implies the backend API calls it needs within one `AccountingBook`. Backend APIs do not create cross-book authority; books are physically separated and routed independently. Parent/sub-book consolidation APIs operate through explicit parent-authorized consolidation workflows and do not let parent users bypass child-book ownership or child-book role assignments.

Every workflow-originated backend request must carry enough execution context for the backend to re-check authority. The minimum context is `book_id`, `entity_id`, `workflow_id`, `workflow_deployment_id`, client-generated `workflow_execution_id`, and authenticated `user_id`. The backend verifies that the user is authorized for the workflow, that the workflow deployment is the active or historically valid deployed artifact for the request, and that the requested backend API is listed in the deployment's `backend_api_calls`.

Every backend mutation request must carry a client-generated UUID for the transaction or administrative event. If the same ID is received more than once, the backend returns the original result or current completed state rather than creating duplicate ledger entries, duplicate administrative events, or duplicate side effects.

Workflow execution identity is client-generated. The browser generates the `workflow_execution_id` and passes it with API calls. The backend records the workflow execution start and context as administrative ledger events for audit logging and tracing purposes, but the backend does not manage or execute the workflow's state machine.

The backend must verify that each client-provided `workflow_execution_id` belongs to the requested `book_id`, `entity_id`, `workflow_id`, `workflow_deployment_id`, and authenticated user before accepting any workflow-originated backend mutation. Repeated requests with the same client-generated ID must be handled idempotently and return the existing transaction outcome.

For v1 and later, this does not require a separate workflow capability token. The browser's authenticated user context supplies the user identity, and the workflow execution context supplies the workflow/deployment identity. The browser is not the authority boundary: it carries context, but the backend re-checks that context against server-side workflow, role, artifact, book state, and idempotency state before honoring the request.

### 7.5 Sample Workflows

#### 7.5.0 Bootstrap and first-book flow

The bootstrapped workflows in the first implementation are `Open book` and `Adding a workflow`.

`Open book` is accessible only to the owner of the `AccountingBook`. It unlocks the book on the backend by loading the book key into backend memory. The key remains in memory for that backend process. If the backend server restarts, the owner must run `Open book` again before the book can be used.

The first-book flow is:

- run `Open book` when an existing encrypted book must be made available to the backend
- bootstrap enough authority to add workflows
- use `Adding a workflow` to create `Add an entity (and a book)`
- use `Add an entity (and a book)` to create the first live book context

#### 7.5.1 Recording startup expense

This workflow records an owner-funded or company-paid startup expense.

The workflow must:

- collect expense date, description, amount, resource unit, source account, expense or asset account, and optional source-document metadata
- verify that the user is authorized to run the workflow for the selected entity and book
- call the runtime backend API to post a balanced entry
- preserve the workflow execution id and workflow deployment id on the posted entry

#### 7.5.2 Recording bank account transactions manually

This workflow records bank activity before bank-statement import or automated reconciliation exists.

The workflow must:

- collect bank account, transaction date, amount, direction, description, offset account, and optional reference metadata
- post only through the runtime backend API
- reject entries that would affect a closed period or inactive account
- leave bank-statement-line import and automated matching to the later reconciliation workflow

#### 7.5.3 Reconcile bank accounts at EOP

This workflow performs an end-of-period manual reconciliation.

The workflow must:

- identify the bank account, reconciliation period, opening balance, ending balance, and manually entered outstanding items
- compare the account projection against the expected ending balance
- report discrepancies without mutating ledger history
- create any required corrections as new journal entries through ordinary authorized posting workflows
- record the reconciliation result as an administrative ledger event

#### 7.5.4 Add a sub-ledger or sub-book

This workflow creates a child `AccountingBook` under a parent `AccountingBook`.

The workflow must:

- verify that the user is authorized in the parent book to create sub-books
- create a new child `AccountingBook` with its own `book_id`, storage folder, ledger, owner role, encryption key, and authorization boundary
- set the child book's `parent_book_id` to the parent book
- copy parent book settings into the child book as defaults, except role assignments
- allow the child book to use a different owner from the parent book
- allow the child book to use a different chart of accounts from the parent book
- record the parent/sub-book link as immutable ledger events in both books
- create or update the parent book's `book_links.json` projection
- create the default parent consolidation rule: consolidate all posted child-book transactions into the parent book when chart/account mapping is resolvable

Role assignments are intentionally not copied. The child owner must assign child-book roles through the child book's own authorization workflow.

#### 7.5.5 Define or update sub-book consolidation rules

This workflow adds or changes rules in the parent book for consolidating child-book activity.

The workflow must:

- verify that the user is authorized in the parent book to manage consolidation rules
- identify the child book by `book_id`
- define whether the rule filters, summarizes, transforms, or maps child transactions
- support chart mapping when the child book uses a different chart of accounts from the parent book
- record the rule as an immutable `CONSOLIDATION_RULE` event in the parent ledger
- update the parent book's `consolidation_rules.json` projection

If no custom rule exists for a child book, the default rule is to consolidate all posted child-book transactions to the parent book when chart/account mapping is resolvable.

If the child book uses a different chart and the default rule cannot resolve the parent accounts, consolidation must remain pending until an explicit mapping rule is defined. The operational resolution may be to map detailed child activity into a parent summary account instead of asking the child book to expose or conform to the parent's account structure.

#### 7.5.6 Consolidate sub-book activity into the parent book

This workflow applies the parent book's consolidation rules to one or more child books.

The workflow must:

- verify that the user is authorized in the parent book to run consolidation
- read child-book activity only through the backend application API or an authorized export/input path
- apply the parent book's consolidation rules
- append derived `CONSOLIDATION` events or accounting entries to the parent ledger
- preserve links to the child `book_id`, child `entry_id`, and `consolidation_rule_id`
- be idempotent, so re-running consolidation does not duplicate parent entries for the same child transaction

Consolidation does not give parent users direct authority to post inside the child book. It creates parent-book derived activity from child-book activity according to parent-owned rules. If no valid mapping to parent accounts exists, or if the child owner's authorization boundary prevents the parent from reading the needed detail, the workflow must report a pending consolidation issue rather than inventing parent entries. The parent operator and child operator resolve the issue operationally, for example by defining a summary-account consolidation rule acceptable to both books.

The following workflows will not be included in the first phase.

##### 7.5.7 Record external transactions from other accounting records like a brokerage firm

## 8 Non-Functional & Architecture Specification

This section records deployment-oriented and operational requirements that are not part of the core accounting model itself, but are required for the system to be practical, portable, and evolvable.

### 8.1 First Implementation Topology

In the first implementation, all major system components will exist and work, but the physical deployment will be intentionally simple.

The top-level runtime will be a routing server responsible for:

- authentication
- authorization
- directing requests to the correct logical component

Within that routing server, the system will contain four logical servers:

- a runtime backend application server that exposes authorized accounting APIs and invokes the Rust `AccountingEngine`
- a frontend web server that serves a web application which talks to the runtime backend application server through the routing server with complete authorization checks
- an MCP server that exposes agent-facing primitives and participates in workflow generation, orchestration, and other AI-assisted behavior
- a Python dev-time backend server that handles workflow-generation related backend operations, including LLM access, prompt/context assembly, generated artifact preparation, and workflow deployment support

The responsibility split is:

- the Rust `AccountingEngine` owns stable transactional behavior, accounting invariants, posting state transitions, and durable ledger mutation
- the runtime backend application server is the only logical server with accounting storage credentials and durable write authority, and it reaches storage only through the `AccountingEngine`
- the MCP server owns development-time and administration-time AI primitives such as workflow generation, workflow explanation, and system-evolution tasks
- the Python dev-time backend server owns workflow-generation backend operations and LLM integration needed by MCP
- production runtime LLM access from the runtime backend application server is not required in the first implementation and will be added only when there is a direct product requirement for backend-native AI behavior

For the first implementation, these four logical servers may run together in one physical deployment unit. This is acceptable and preferred for simplicity as long as the logical boundaries remain clear in code and configuration.

The architecture must preserve the ability to separate these logical servers later into different physical deployments and different security zones. In later deployments:

- the runtime backend application server and Rust `AccountingEngine` may be isolated in a tighter security zone because they have direct access to accounting storage
- the frontend web server may be deployed separately as a public-facing component
- the MCP server may be deployed separately because it has different LLM, tool, and outbound-integration concerns
- the Python dev-time backend server may be deployed separately because it has LLM, code-generation, artifact-preparation, and development-time supply-chain concerns

The first implementation is therefore required to be physically simple but logically separable.

### 8.2 AccountingBook Export and Restore

The system must allow an `AccountingBook` to be exported and restored from a prior export.

The export requirement is:

- an `AccountingBook` must be reconciled before export
- if the implementation guarantees that the book is always reconciled, this reconciliation step may be a no-op
- the exported representation must contain enough information to reconstruct a usable book, not merely a read-only archive
- the first-implementation accounting export artifact is an encrypted JSON bundle for the book data, including workflow deployment references and hashes but not the generated workflow artifact bodies
- deployed workflow artifacts may be exported separately from the dev artifact store as an inspectable artifact bundle, but that bundle must not contain private accounting context
- the export is encrypted for the intended export reader and does not transfer the book key between users
- the initial intended storage target for these export artifacts is a git repository until larger storage is needed

The restore requirement is:

- restoring an export is a restore operation, not a merge operation
- restore may target a new empty book location or a damaged book location that is being intentionally replaced
- when restore targets an existing book location, the current book contents are wiped and replaced by the exported state
- internal ids from the export are preserved
- restoring a backup preserves the logical `book_id`
- restored workflow deployment references must resolve to matching dev artifact store objects by artifact id and hash before those workflows can run
- if the matching dev artifact bundle is not present, the book may be restored, but affected workflows must be marked unavailable until their artifacts are restored or redeployed
- creating a separate clone or divergent copy is a future workflow, not the default restore behavior
- after restore completes, one must be able to continue adding to the restored book
- the restored book does not need to be a dead historical snapshot; it must be a live operational starting point

The main portability goal is that a book can move from one deployment or storage implementation to another without losing continuity of operation. Restore is similar in spirit to checking out a known good git state. Reapplying later ledger entries from a damaged or divergent book is deferred to an advanced recovery workflow that creates new transactions from the old ledger after human review.

For v1, “reconciled before export” is intentionally lightweight:

- if no discrepancies are known, export proceeds
- if restore later reveals discrepancies, they become implementation issues to resolve
- if the engine always preserves internal balance, reconciliation before export may be a no-op

### 8.3 First-Implementation Frontend Contract

The frontend contract for the first implementation is intentionally simple.

- each workflow-facing UI is a REST-style application served from the frontend server
- workflow artifacts are copied to the frontend server under the correct folder and routing is updated to expose the endpoint
- forms are application artifacts rather than generic schema-driven forms in v1
- frontend authentication context reaches backend and MCP through OAuth-based user authorization
- the frontend only needs enough capability to support the first sample workflows and no more

### 8.4 Dev-Time Artifact Storage

Dev-time artifact storage is separate from accounting book storage. Its security profile is integrity-first and transparency-first, while accounting book storage is confidentiality-first and authority-first.

For the first implementation:

- deployed workflow definitions, generated source, manifests, hashes, deployment metadata, and later signatures are stored in the dev artifact store, not inside the accounting book storage folder
- accounting book storage records workflow deployment events, artifact references, hashes, and later signatures needed to audit which artifact was used
- dev artifacts are immutable after deployment; changing a workflow creates a new artifact and a new `workflow_deployment_id`
- hashes are authority for artifact identity; paths are locators only
- deployed workflow artifacts should be readable and inspectable by authorized users so they can verify what code and workflow behavior will run
- unauthorized changes to deployed artifacts must be prevented
- private accounting context is either not used, replaced with synthetic/redacted examples, or used transiently through authorized runtime APIs and not saved in the dev artifact store
- prompts, context bundles, LLM traces, generation logs, test inputs, failed generations, and debugging records must not persist private accounting data in the dev artifact store
- if real accounting context is required to generate or troubleshoot a workflow, it must be fetched through authorized runtime backend APIs, used only for the current operation, and then discarded
- synthetic examples should be preferred for workflow generation, tests, and inspectable artifacts

The minimum dev artifact store layout is:

```text
dev_artifacts/
  workflows/
    <workflow_deployment_id>/
      workflow.json
      manifest.json
      code/
      signatures/
```

The `manifest.json` records the artifact hash, generator metadata, deployment target, and any signature metadata. It must not contain private accounting context.
