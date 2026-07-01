# LedgerZero Project Plan

## Complete the first Impl Spec.

- [X] Define the executable `WorkflowDefinition` contract
  - [X] Add the minimum required fields for a deployable workflow definition.
  A workflow definition includes the name, an optional description, and list of one or more steps.  A step will be either a rest application with a form where user can fill in the information and then submit the information through a API call, or an API call if no user provided information is needed.
  - [X] Decide whether deployed workflows are stored as structured data, generated code, or both
  Deployed workflows will have its code saved as a copy in the file system, and also deployed to the web server so it can be served to the user.  In the future, we may decide to create repository of workflows that can be shared across multiple books AND multiple users, like from a market place.
  - [X] Define workflow versioning, status, and deployment metadata
  No versioning, nor status.  A workflow is always deployed, new versions are added by removing an exsiting workflow and adding a new workflow.
  - [X] Define how sample workflows map onto this contract
  As an implementation decision.

- [X] Define the minimum MCP primitive set for first implementation
  - Will define it as we implement the workflows.  It is expected that, when workflow requires a primitive that is missing, we will implement it and add it to the MCP primitive set.  Deploying the MCP.  Notice this is the traditionaly software deployment, not the workflow deployment which is part of the operation (dev time for end user, run time for me, the developer of the accounting system itself).

- [X] Define the minimum backend application API set for first implementation
  The backend API set will have the support for authorizaiton to start with.  From there, I will work on adding book etc.  We will implements the workflows one at a time, and add API to backend server and primitives to MCP server as we do.

- [X] Fill in section `4.1 Accounting Engine Responsibilities and Deployment Modes`
  - [X] Define the engine's core runtime responsibilities
  - [X] Define what the engine validates before posting.
  The engine will only call validation that can be defined by the account, default to nothing.
 There should be two kind of transactions.  One kind is realtime transaction which requires the posting to both affected accounts to have ACID properties.  The other kind will be asnchornous where the transaction will be logged to the ledger and the posting is be done asynchronously, gurantteeing only eventual consistency.
 - [X] Define what the engine serializes, snapshots, and audits
  The engine will deal with objects.  Serialization will be hanfled by the storage layer.  Snapshots will be handled by the export workflow.  Audits will be the ledger.  Operational log will be application log using whatever logging best practice of python backend/mcp logging, and rest frontend logging.
  - [X] Define supported first-implementation deployment modes
  The application will be deployed on premise as well as on an Oracle Cloud VM. Just making sure we do not do anything to jeopidize the ability to deploy them as containers, but that can wait till we seperate the logical servers into physical servers.

- [X] Simplify and clarify the approval model for first implementation
  - [X] Decide whether approval is modeled as workflow state, journal activity, or both
  Approval will be just another workflow:  posting a transaction from a pending account into the target account.  Approval of any transaction will be modelled as workflow, mayby with "Approve ...." as it name.  No real different treatments.
  - [X] Define the minimal approval flow needed for v1
  None.
  - [X] State what is deferred until later versions.
  Defer everything possible and do the minimum fundation to get the system up and running.

- [X] Add the missing supporting concepts needed by the sample workflows
  - [X] Decide whether `Counterparty` is a first-class entity or metadata in v1.
  Yes. It is an entity.  It does not have a book or a owner.
  - [X] Decide how bank accounts are represented
  Treat it as an asset account till difference must be added.
  - [X] Decide how bank statement lines are represented
  Not needed yet.
  - [X] Decide how reconciliation matches are represented
  Not needed yet.
  - [X] Decide how source documents and references are represented
  Just metadata.

- [X] Specify the first-implementation file storage layout
  We will have a ledger file and each account and its complete list of transactions and running total of balances will be saved as a json file.  These files will be saved in a folder named after the book.  

- [X] Define the first-book flow
  - It will be our second workflow.  The only boottrapped workfow will be "Adding a workflow."

- [X] Define generated-code deployment constraints
  - None to start with as I will be the only user with that authority.  The generated code will be executed in the users' browser and be executed using the users' credential and authorization, so we just need to make sure it does not damage user's broweser container in which it runs.  It should not able to do damage beyond what the user can do using customized code.  In the future, we may decide to have a approval process to approve and sign the generated code before it is deployed.

- [X] Clarify role, workflow, and authorization administration
  - [X] Define who can create roles
  It is an authorization itself.  Anybody can create a role which is just a collection of workfows.
  - [X] Define who can assign roles
  The owner of the book will have the orginal role that will allow him to assign workfflows to roles and roles to other users.  Some role/workflow will allow the other user to assign roles and workflows to even more other users and so one.  Complete traceability will be enforced.
  - [X] Define who can create workflows
  It will be an authorizaiton that must be assigned.
  - [X] Define who can deploy workflows
  It will be an authorization that must be assigned.
  - [X] Define how workflow-to-role assignment works at runtime
  A role is just a list of workflows. It has a name, description, and can be assigned to users.

- [X] Define the portable `AccountingBook` export bundle
  -  Just a JSON file with everything encrypted. Maybe keep the deployed code in a seperated folder.  The intention is to commit it to a git repository for now till bigger storage is needed.

- [X] Define the `AccountingBook` import flow
Just create a new book and add everything to it, including the deployed workflows.

- [X] Define what “reconciled before export” means for v1
 Assuming nothing, until we import and find discrepencies.  Not sure if we will find anything. 

- [X] Make the spec internally consistent again
  - [X] Remove or fix references to sections that no longer exist
  - [X] Decide whether later sections such as deployment, migration, operations, and project plan belong in this spec or a separate document
  This.  Add sections below.
  - [X] Normalize terminology such as `AccountingBook`, `Entity`, `Workflow`, and `Role`
  - [X] Clean up obvious wording ambiguities that would block implementation

- [X] Define the frontend contract for first implementation
  - [X] Decide how workflow input forms are delivered to the frontend.
  It will be rest apps.  The artifacts will be copied to the frontend server under the right folder and the routing files updated to add the right end points.
  - [X] Decide whether forms are schema-driven, hardcoded, or mixed in v1
  It will be a rest application.
  - [X] Define how frontend auth context reaches backend and MCP
  OAuth for user authorization.
  - [X] Define what the frontend needs for the sample workflows and nothing more

## Decide pre-implementation architecture gates.

- [X] 1. Decide generated workflow code trust boundary
  - Gap: The spec says generated workflow code becomes deployed application code, is copied to the frontend, and runs in the user's browser with the user's credentials. It does not yet make the trust boundary explicit enough for implementation.
  - Recommendation: Make generated workflow code frontend-only for the first implementation and treat it as an untrusted client. All real authority stays in backend APIs. Generated code can render forms and call authorized APIs, but it cannot directly access files, secrets, storage, or privileged backend internals.
  - Front end or not, Code generated, deployed, and monitoring its execution etc. must follow the best practices of SDLC should be faithly followed, where it is done by a human or by an agent.  For the first implementation, I am doing everything as a senior developer, so I am taking fully responsibility for the whole cycle.  This workflow itself will grow with tools and harnesses.  We are deferring the more complicated aspects of the process till the roles must be seperated and more gates in CI/CD are added.  For example, we may want to introduce a workflow definition tool such as n8n.

- [X] 2. Decide workflow replacement and historical auditability model
  - Gap: The spec says there is no workflow versioning or status and that a workflow is removed and replaced by a new workflow, but journal entries also retain workflow identity and generated code is intended to be auditable.
  - Recommendation: Keep "no user-facing versioning," but internally store immutable workflow deployments. A workflow name can point to the latest deployment, while each journal entry records the exact deployment id or code hash used.
  - We will keep record of all deployments.

- [X] 3. Decide ledger source of truth versus account JSON projection
  - Gap: The file layout includes both an append-only `ledger.jsonl` and per-account JSON files containing transactions and running balances. The spec does not explicitly say whether account files are authoritative or derived.
  - Recommendation: Make `ledger.jsonl` the only accounting source of truth. Account JSON files should be derived projections or caches that can be rebuilt from the ledger.
  - Yes.  But the intention is that 'ledger.jsonl' should NEVER go out of balance with the accounts when all transactions' processing have finished.  When taking a snapshot, a marker must be inserted so transaciton before the marker are all done and the transactions after the marker will be blocked until the the snapshot has been fully created. When the transaction volume got too big, we may need to seperate the book into many books so the each ledger can be locked seperately, with their interactions made offline and recondiled as seperate transactions, follow the spirit of git's distributed repositories strategy.

- [X] 4. Decide realtime versus asynchronous accounting semantics
  - Gap: The spec allows realtime and asynchronous transactions, but does not clarify whether accounting posting itself can be eventually consistent.
  - Recommendation: Require every journal entry posting to be atomic and balanced before it is considered posted. Allow asynchronous behavior only for projections, account cache updates, export generation, external integrations, or UI refresh.
  - No.  Only a few kinds of transactions needs to be synchronized.  For example, sales off an inventory account of physical goods.  Most transactions can be made consistent eventually.  Keeping synchronization is very expensive and the killer of performance and scalability and must be as selective as possible.

- [X] 5. Decide identity linking and authorization merge rules
  - Gap: The spec says multiple authentication identities may be linked to one user and that if two previous users are combined, their authorizations are combined as a superset.
  - Recommendation: Do not auto-merge authorizations by default. Treat identity linking as a privileged workflow requiring explicit approval and audit. Keep authentication identities separate from the authorized user record. If two users are merged, record who approved it and which permissions were retained.
  - No.  A user can merge his own accounts.  He is the same person, or claim to be. We need to validate that he does have the credentials.  For example, if he has two logins, one with Google and one with Apple, he can merge them by login into Google and add his apple id, which will trigger an apple login.  If he can login into apple, he authenticated himself and all he does is saying he is the physical/legal entity behind both logins.  It is his decision and nobody's else and need no audit or approval.  A person is one person.  Until quantum scientist manage to split a person into two, we will be OK.

- [X] 6. Decide encryption and key ownership boundary
  - Gap: The spec says data will be encrypted and exports are encrypted JSON bundles, but it does not define where encryption/decryption happens or who owns the keys.
  - Recommendation: Define a v1 key boundary before writing storage. Suggested rule: backend/storage encrypts at rest, frontend and generated workflow code never see raw keys, and export encryption uses a book-level key or passphrase.
  - Even in the first implementation, key pairs should be create for each user for encruption.  If we can find an easy way to use Google Login or any the authentication provider to gate the access to the private key, that will be great.  Otherwise, keep the key in a local key store with a pass phrase.  The key will only be available in the backend service and only in memory.

- [X] 7. Decide `AccountingBook` versus `Entity` security boundary
  - Gap: The spec defines `AccountingBook` as the storage/export container and `Entity` as an accounting/reporting container, but the primary security boundary is not explicit.
  - Recommendation: Make `AccountingBook` the storage, export, and bootstrap security container. Keep `Entity` as the accounting/reporting boundary. Roles can still be scoped to entities, but book ownership controls bootstrap, export/import, and storage access.
  - AccountingBook has one and only owner.  As far as AccountingBook is concerned, the owner is a role and the only boottrapped role, similar to a owner of a file.  An owner of an AccountingBook can change the owner from himself to other user but he will loss authorization to this owner role of this AccountingBook.

- [X] 8. Decide import identity and continuity model
  - Gap: The spec says import creates a new book and loads exported data and workflows, but it does not say whether imported ids are preserved or remapped.
  - Recommendation: Preserve internal ids inside the imported book, assign a new outer `book_id` only if needed, and record an import event. Do not support merging an export into an existing live book in v1.
  - Maybe this is misnomer.  This is meant to be restore of a backup.  It will wipe out the current book in cases where the current book is deemed damaged and it it not worthwhile to move forward.  This will be a clean wipe out of the current book.  Seem like a checkout to a specific commit in git, nothing is needed to wrap or merge or that.  Reapply later ledger entries will be advanced workflow later by creating new transactions based on information in the old ledger which will likely have entries that need modifications and decisions.

- [X] 9. Decide whether `Counterparty` is separate from accounting `Entity`
  - Gap: The spec says `Counterparty` is a first-class entity but does not own a book or owner, which could be confused with accounting `Entity`.
  - Recommendation: Make `Counterparty` or `Party` a separate type from accounting `Entity`. Do not give counterparties books, roles, accounts, or ownership semantics in v1.
  - Remove the Counterparty entirely. So in a sale, there will be another entity other than the owner of the accountingBook.  Only in that transaction, we may call it a counterparty, or other things as approriate.  It is only a name for convenience of dis-ambiguation.  It is an entity.

- [X] 10. Decide backend API as the only runtime authority boundary
  - Gap: The spec says generated frontend workflow apps call backend APIs and backend owns stable accounting behavior, but it should explicitly forbid MCP or generated workflow code from bypassing backend APIs for runtime writes.
  - Recommendation: Decide that MCP may generate workflows and gather context, but runtime writes must go through backend application APIs. MCP and generated workflow code must not call engine/storage directly.
  - Absolutely.

## Decide remaining pre-implementation architecture gates.

- [X] 1. Decide administrative state source of truth
  - Gap: The spec makes the ledger immutable, but `roles.json`, `entities.json`, `users.json`, and workflow files are listed as metadata files rather than rebuildable event projections.
  - Recommendation: Decide that all mutable book state has an append-only event source, such as `book_events.jsonl`; JSON files should be projections.
  - Decision: We should keep these events as transactions and record them in the Ledger as different type of events.  There will be only one source of truth, in one file. The "Meta" data file will have the exact architecture position of an account.

- [X] 2. Decide asynchronous transaction state machine
  - Gap: `JournalEntry.status` has `POSTED`, but asynchronous transactions can be logged before downstream effects complete.
  - Recommendation: Define durable states for accepted, processing, finalized, projection-complete, failed, and reversed, plus idempotent replay rules.
  - Decision: A transaction will be marked as posted only after it is posted.  Eventual consistency will be managed by adding derived transactions to the ledger.  When the scale becomes too larger for one ledger file, we will introduce a queueing mechanism with queues and sub queues and the ledger becomes the persistent storage of the queue manager.  This will allow us to scale as larger as needed.

- [X] 3. Decide permission model granularity
  - Gap: Roles are collections of workflows, but generated workflow code calls backend APIs, and backend APIs are the runtime authority boundary.
  - Recommendation: Decide whether authorization is workflow-scoped, API-scoped, account-scoped, or a capability token combining all three.
  - Decision: Authorization will be only workflow-scoped.  There is a chain of authorication from the owner to the workflow, which implies authorization to the backend API.  APIs are routed to the storage which is limited to one book that belongs to the owner.  There is no cross walk between books:  They are physically seperated.

- [X] 4. Resolve SOX segregation conflict
  - Gap: The SOX table says one user cannot hold posting and approval for the same account type, but workflow lifecycle says a user holding both authorizations may execute both automatically.
  - Recommendation: Decide whether segregation of duties is a hard invariant or a configurable control.
  - Decision: SOX seperation will be enforced a the time of role assignment.  If approval and lodging must be seperated, we shall have two workflows and demonstrate or enforce the seperation of users assigned to these roles.  Defer this implementation.

- [X] 5. Decide chart/account classification model
  - Gap: Multiple charts can coexist, but `Account` still carries structural fields like account type, code, and parent relationship.
  - Recommendation: Move chart hierarchy and classification into separate chart mapping records before implementing account storage.
  - Decision: Different charts will have different accounts.  Each chart of account will be self consistent.  The only invariant is that A = L + OE.  When new chart of account is added, some transaction may need to be distributed which requires distribution rules or more tagging and more input fields.  These will affect existing and new workflows but will be resolved at "dev" time.

- [X] 6. Decide multi-resource balancing model
  - Gap: The spec treats currency, inventory, commodities, and digital assets as resources, but the debit/credit invariant assumes comparable amounts.
  - Recommendation: Decide whether v1 is currency-only, or define both resource quantity and valuation/balancing currency on journal lines.
  - Decision:  each transactoin's debit and credit may have different unit of measure, but each unit of measure must be exactly the same as the unit of measure of the corresponding account.  Each account will only have one unit of measure.

- [X] 7. Decide cross-entity transaction representation
  - Gap: `JournalEntry` has one `entity_id`, and account validation requires accounts belong to the same entity, but intercompany transactions reference two entities.
  - Recommendation: Decide whether cross-entity activity is one compound entry or linked single-entity entries.
  - Decision:  Intercompany transactions are two different trasnactions.  We may create a book for a counter party itself, but they will be seperate books.  The "counter party" is for identificaton only as far as the current book goes.  All transactions for the same "counter party" is a selection criteria for data export and/or input.

- [X] 8. Decide encryption envelope and book identity
  - Gap: The spec defines per-user keys and encrypted exports, but not the book data key, key wrapping, owner transfer, revocation, or restored-book identity.
  - Recommendation: Define a book-level data key wrapped for authorized users, plus stable `book_id` and restore/instance identity rules.
  - Decision:  The book has one key.  It will be loaded into memory before the book is read into the memory.  In the memory, access to account data is limited by authorization, not cryption.  Exporting only export data readable by the user who create the export, and will be encrypted by the user who is going to read the export.  When owner ship is transferred, the book will be re-encrypted by the new owner's key.  There should be no key transfer of any kind.

## Implement the first LedgerZero system.

Implementation rule: complete one milestone, run its tests, stop for review, and only then begin the next milestone. Do not start a later milestone just because the code is nearby.

Spec interpretation used for implementation: sub-book creation copies parent book settings as defaults, but role assignments are not copied. The child owner assigns child-book roles through the child book's own authorization workflow.

- [ ] 1. Create the implementation workspace and quality gates
  - Dependencies: `Writing/Accounting/LedgerZero_Spec.md` is the source of truth.
  - Scope: Establish the repository structure for the Rust `AccountingEngine`, runtime backend, frontend app, MCP server, Python dev-time backend, shared contracts, and developer scripts without implementing business behavior.
  - Acceptance criteria: The logical server boundaries from the spec are visible in folders and package names; each component has a minimal build/test command; generated artifacts and accounting book data have separate folders; no accounting storage credentials are reachable from MCP, frontend, or dev-time backend code.
  - Tests: Empty/smoke test suites run for Rust, Python, MCP, and frontend packages; a top-level check command runs all available checks.
  - Review gate: Stop after the skeleton and commands are reviewable.

- [ ] 2. Define shared domain contracts and serialized book state
  - Dependencies: Milestone 1.
  - Scope: Define the first implementation data contracts for `AccountingBook`, `Entity`, `Account`, `JournalEntry`, `JournalLine`, ledger events, role/workflow authorization records, price table records, sub-book links, consolidation rules, and workflow deployment references.
  - Acceptance criteria: The contracts reflect client-generated UUIDs for entries, events, and workflow executions; `book.data.enc` is modeled as the single source of truth after decryption; administrative events and accounting events share one ledger event model; contracts can round-trip through JSON without losing stable ids.
  - Tests: Contract serialization round-trip tests; required-field validation tests; compatibility fixture for one empty book.
  - Review gate: Stop before implementing posting logic.

- [ ] 3. Implement the Rust `AccountingEngine` core posting model
  - Dependencies: Milestone 2.
  - Scope: Implement in-memory account, period, journal entry, journal line, and ledger event behavior for one book, with same-unit double-entry posting and immutable correction-by-new-entry semantics.
  - Acceptance criteria: Balanced same-unit entries can be posted; unbalanced entries are rejected; closed periods reject posting; inactive or missing accounts reject posting; posted entries are immutable; repeated client-generated entry/event ids return the existing outcome rather than creating duplicates.
  - Tests: Rust unit tests for balanced posting, unbalanced rejection, closed-period rejection, inactive-account rejection, duplicate-id idempotency, and correction entries.
  - Review gate: Stop before adding cross-unit price balancing.

- [ ] 4. Implement price table and multi-resource balancing
  - Dependencies: Milestone 3.
  - Scope: Add price table records and transaction-time price validation so entries with different account units balance by `debit_amount == credit_amount * price` using the applicable price at transaction time.
  - Acceptance criteria: Each line's unit matches its account; cross-unit entries require an applicable transaction-time price; missing or ambiguous prices reject posting; price changes are recorded as ledger transactions/events rather than hidden conversions.
  - Tests: Rust unit tests for same-unit entries, valid priced cross-unit entries, missing price rejection, stale/incorrect price rejection, and price-change audit events.
  - Review gate: Stop before adding encrypted storage.

- [ ] 5. Implement file-backed encrypted book storage
  - Dependencies: Milestones 2-4.
  - Scope: Implement the storage driver for one book folder containing `book.data.enc`, `book.keystore.json`, and `export/`, with all book state loaded into memory and rewritten atomically after each mutation.
  - Acceptance criteria: `BookKeyProvider` loads encrypted/wrapped key material from `book.keystore.json`; `Open book` loads the key into backend memory; no plaintext book key is written to disk; file writes use temp-file-plus-rename atomic replacement; corrupt partial writes can be recovered by reverting to the last clean file/git checkpoint.
  - Tests: Storage round-trip tests, atomic rewrite failure simulation, duplicate-id persistence tests after reload, wrong-key rejection, no-plaintext-key fixture scan.
  - Review gate: Stop before exposing backend APIs.

- [ ] 6. Implement runtime backend shell, auth boundary, and book lifecycle
  - Dependencies: Milestone 5.
  - Scope: Create the runtime backend application server with book routing, Google Login authentication adapter, local test authenticator, session handling, owner-only `open_book`, and in-memory opened-book lifecycle.
  - Acceptance criteria: Backend is the only runtime authority boundary for durable writes; frontend/MCP/dev-time backend cannot access storage directly; unopened encrypted books reject book operations; owner can open a book; restart clears in-memory key state; authenticated user identity is separate from workflow authorization.
  - Tests: Backend API tests for unauthenticated rejection, unauthorized rejection, owner-only open-book success, opened-book operation success, restart/reopen behavior, and storage boundary enforcement.
  - Review gate: Stop before adding book administration APIs.

- [ ] 7. Implement book administration APIs
  - Dependencies: Milestone 6.
  - Scope: Add backend APIs for creating books, entities, accounts, periods, roles, workflow authorization assignments, and role-to-user assignments.
  - Acceptance criteria: New books bootstrap exactly one owner role; roles are collections of workflows; workflow authorization implies only the backend APIs listed for that workflow; all administrative state changes are recorded as immutable ledger events in `book.data.enc`; role assignment changes are idempotent by client-generated event id.
  - Tests: Backend integration tests for create book, create entity, create account, create period, create role, assign workflow to role, assign role to user, duplicate admin event handling, and unauthorized admin rejection.
  - Review gate: Stop before adding posting/query APIs.

- [ ] 8. Implement posting and query backend APIs
  - Dependencies: Milestones 3-7.
  - Scope: Add `post_entry`, `list_entries`, `get_balance`, `list_accounts`, and audit-log query behavior through the runtime backend.
  - Acceptance criteria: Every mutation carries `book_id`, `entity_id`, `workflow_id`, `workflow_deployment_id`, client-generated `workflow_execution_id`, authenticated `user_id`, and client-generated entry/event id; backend re-checks workflow authorization and deployment API allow-list; query APIs enforce book/entity scope.
  - Tests: Backend integration tests for authorized post, unauthorized post, duplicate post retry, invalid workflow context, invalid deployment API call, balance query, entry list filtering, and audit-log retrieval.
  - Review gate: Stop after one complete backend-only accounting vertical slice is reviewable.

- [ ] 9. Implement workflow artifact store and deployment metadata
  - Dependencies: Milestones 2, 7, and 8.
  - Scope: Implement `dev_artifacts/workflows/<workflow_deployment_id>/` with `workflow.json`, `manifest.json`, generated code folders, hashes, and immutable deployment records in the accounting ledger.
  - Acceptance criteria: Deployed workflow artifacts are stored outside accounting book storage; deployed artifacts are immutable; artifact identity is hash-based; workflow name points to the latest deployment; historical entries keep exact `workflow_deployment_id`; private accounting context is not persisted in dev artifacts.
  - Tests: Artifact hash tests, immutability tests, deployment event tests, latest-workflow lookup tests, historical deployment reference tests, private-context fixture scan.
  - Review gate: Stop before serving frontend workflow code.

- [ ] 10. Implement frontend shell and bootstrapped workflow screens
  - Dependencies: Milestones 6, 8, and 9.
  - Scope: Create the frontend web app shell, authenticated session handling, owner-only `Open book` screen, and minimal screens for listing workflows and launching workflow-facing REST-style applications.
  - Acceptance criteria: Frontend never receives raw encryption keys; workflow execution ids are generated client-side; backend treats all submitted data as untrusted; frontend can call backend APIs with authenticated user context and workflow context; frontend can show backend validation errors clearly.
  - Tests: Frontend unit tests for workflow execution id generation and API payload construction; browser/API smoke test for login/open-book/list-workflows; negative test proving no raw key appears in frontend state or network payload fixtures.
  - Review gate: Stop before adding MCP or workflow generation.

- [ ] 11. Implement MCP server primitives for existing backend operations
  - Dependencies: Milestones 6-9.
  - Scope: Implement the first MCP primitives that wrap authorized backend operations: workflow list/get, book/entity/account/period creation, role assignment, sub-book primitives as stubs or not-yet-implemented responses, consolidation primitives as stubs or not-yet-implemented responses, and `explain_reconciliation_issue` as a placeholder primitive.
  - Acceptance criteria: MCP never reads or writes book files directly; MCP calls runtime backend APIs for runtime facts; missing primitives return explicit "not implemented yet" responses rather than silently bypassing architecture; primitive names match the spec.
  - Tests: MCP smoke tests for primitive discovery, successful backend-backed primitive call, unauthorized backend response propagation, and storage-bypass guard test.
  - Review gate: Stop before adding LLM/dev-time backend behavior.

- [ ] 12. Implement Python dev-time backend and workflow generation adapter
  - Dependencies: Milestones 9 and 11.
  - Scope: Create the Python dev-time backend that owns LLM wrapping, prompt/context assembly, generated artifact preparation, and deployment support, starting with deterministic template generation before enabling real LLM calls.
  - Acceptance criteria: Dev-time backend has no accounting storage credentials; any real accounting context is fetched only through authorized runtime backend APIs and not saved; synthetic/redacted examples are preferred; generated artifacts are inspectable before deployment.
  - Tests: Python tests for template generation, redaction/no-private-context persistence, manifest generation, backend-context fetch authorization, and failed-generation cleanup.
  - Review gate: Stop before implementing the "Adding a workflow" end-to-end flow.

- [ ] 13. Implement the `Adding a workflow` end-to-end flow
  - Dependencies: Milestones 9-12.
  - Scope: Connect MCP, Python dev-time backend, workflow artifact store, frontend artifact deployment, and backend workflow deployment events for one generated workflow.
  - Acceptance criteria: An authorized user can request a workflow, inspect generated artifacts, deploy it, assign it to a role, and see it available in the frontend; deployment records include artifact id, manifest hash, code hash, deployment id, deployer, and timestamp; unauthorized users cannot deploy.
  - Tests: End-to-end test using deterministic generation; deployment hash verification; unauthorized deploy rejection; redeploy creates a new immutable deployment; old deployment remains auditable.
  - Review gate: Stop before implementing accounting sample workflows.

- [ ] 14. Implement sample workflow: recording startup expense
  - Dependencies: Milestones 8, 10, and 13.
  - Scope: Build the startup expense workflow as a frontend workflow application backed by stable backend posting APIs.
  - Acceptance criteria: Workflow collects expense date, description, amount, resource unit, source account, expense/asset account, and optional source-document metadata; it posts only through backend APIs; successful entries preserve workflow execution id and deployment id; invalid entries are rejected by backend invariants.
  - Tests: End-to-end workflow test for valid startup expense; unbalanced entry rejection; closed period rejection; duplicate submit retry; source metadata preservation.
  - Review gate: Stop before implementing the next sample workflow.

- [ ] 15. Implement sample workflow: manual bank account transactions
  - Dependencies: Milestone 14.
  - Scope: Build the manual bank transaction workflow for bank activity before automated statement import exists.
  - Acceptance criteria: Workflow collects bank account, transaction date, amount, direction, description, offset account, and optional reference metadata; it rejects closed-period and inactive-account attempts through backend responses; bank accounts remain ordinary asset accounts.
  - Tests: End-to-end workflow test for deposit/withdrawal style entries; inactive account rejection; duplicate submit retry; balance update verification.
  - Review gate: Stop before implementing reconciliation.

- [ ] 16. Implement sample workflow: end-of-period bank reconciliation
  - Dependencies: Milestone 15.
  - Scope: Build manual end-of-period reconciliation that compares expected ending balance and outstanding items against account projection without mutating ledger history except for the reconciliation result event.
  - Acceptance criteria: Workflow records reconciliation result as an administrative ledger event; discrepancies are reported without hidden ledger mutation; corrections are created only through ordinary posting workflows.
  - Tests: End-to-end reconciliation match test; discrepancy report test; reconciliation result audit event test; no-ledger-mutation-for-discrepancy test.
  - Review gate: Stop before implementing sub-books.

- [ ] 17. Implement sub-book creation workflow and APIs
  - Dependencies: Milestones 7, 8, 10, and 13.
  - Scope: Implement `create_sub_book`, `list_sub_books`, parent/child link events, child book folder creation, child owner setup, and default copied settings.
  - Acceptance criteria: Child book is a first-class `AccountingBook` with its own storage folder, owner role, encryption key, chart/accounts, and endpoints; parent settings are copied as defaults; role assignments are not copied; parent/sub-book link is recorded in both books; changing child ownership intentionally reduces parent privileges.
  - Tests: Backend integration tests for child book creation, copied defaults, role assignment exclusion, independent open-book requirement, link events in both books, and unauthorized parent access rejection.
  - Review gate: Stop before implementing consolidation rules.

- [ ] 18. Implement consolidation rules and consolidation execution
  - Dependencies: Milestone 17.
  - Scope: Implement `define_consolidation_rule`, `list_consolidation_rules`, and `run_consolidation` for parent-owned consolidation from child-book activity.
  - Acceptance criteria: Default consolidation posts only when parent can resolve child accounts; chart mismatches remain pending until a rule maps/summarizes/transforms child activity; consolidation reads child activity only through authorized child APIs or export/input path; rerunning consolidation is idempotent.
  - Tests: Consolidation success test with matching charts; pending test for missing mapping; summary-account mapping test; unauthorized child read test; duplicate run idempotency test.
  - Review gate: Stop before implementing export/restore.

- [ ] 19. Implement AccountingBook export and restore
  - Dependencies: Milestones 5, 8, 9, and 18.
  - Scope: Implement encrypted export, restore-to-empty-location, restore-over-damaged-location, workflow artifact reference validation, and continued posting after restore.
  - Acceptance criteria: Export is encrypted for the intended reader; export contains enough book data to continue operation after restore; workflow artifact bodies remain separate; restore preserves logical `book_id` and internal ids; missing workflow artifacts mark affected workflows unavailable; restored books can accept new entries after `Open book`.
  - Tests: Export/import round-trip; restore-over-existing-book replacement test; wrong-reader/wrong-key rejection; missing-artifact unavailable test; post-after-restore test; balance preservation test.
  - Review gate: Stop before packaging/deployment work.

- [ ] 20. Implement local deployment, Oracle VM readiness, and operational checks
  - Dependencies: Milestones 1-19.
  - Scope: Add developer and operator commands for local on-premise deployment, Oracle Cloud VM deployment readiness, health checks, logs, backup/git checkpoint guidance, and smoke tests across the four logical servers.
  - Acceptance criteria: One local command can run the routing server with runtime backend, frontend, MCP, and Python dev-time backend logically separated; health endpoints prove component readiness; no component crosses the storage credential boundary; documentation explains how to open a book, run sample workflows, checkpoint to git, export, and restore.
  - Tests: Full-system smoke test; health-check test; storage-boundary regression test; sample workflow smoke test; export/restore smoke test; restart/reopen test.
  - Review gate: Stop for full first-implementation review before adding non-v1 features.
