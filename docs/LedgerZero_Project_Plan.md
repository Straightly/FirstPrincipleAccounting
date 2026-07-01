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
