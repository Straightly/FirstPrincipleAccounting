# LedgerZero Theorems

Standing qualifications and characteristics of this application. A **theorem** here is an architectural guarantee that must remain true across all future changes — not a feature, but a property every feature must preserve.

**Change discipline:** whenever code or data touching a theorem's area is added, changed, or removed, the change must be checked against that theorem's obligations before merge. Each theorem lists the tests or checks that verify it. New guarantees of this kind are added to this file as new theorems, never buried in code comments.

---

## T1 — Scale is bounded only by the storage model

**Statement.** Nothing in the application prevents scaling to very large deployments, except the storage model. Scaling up is achieved by (a) swapping the storage driver behind the storage interface (file → SQLite → PostgreSQL → distributed) and/or (b) splitting activity into multiple `AccountingBook`s reconciled by inter-book transactions. No other component may need redesign.

**Current basis.** The engine depends on an async storage interface, never a medium (Impl Spec §3.2, Axiom 5). The v1 single-encrypted-file store's O(N) rewrite is a driver property, not an architectural one. Books are physically separated with no cross-book runtime authority, so book-splitting scales horizontally by construction.

**Obligations.**
- No API contract, workflow contract, or data model may expose or depend on storage-format details (file layout, single-writer timing, in-memory-ness).
- The storage interface stays async even while the v1 driver is synchronous underneath.
- No component other than the engine may touch storage (Impl Spec §3.4) — anything else creates a second thing to redesign at scale.
- New features must not introduce global cross-book coordination; the multi-book split must remain the pressure-relief valve.

**Verified by.** M2/M3 replay and property tests running identically against in-memory and file drivers; code review against §3.4 boundary.

## T2 — Authentication domains are purely additive

**Statement.** Adding an authentication provider (Microsoft, Apple, an enterprise's own IdP/SSO domain) requires only *adding* code and/or data — a provider record, or a new `IdentityProvider` implementation for non-OIDC protocols — never modifying existing authentication, session, user, or authorization logic.

**Current basis.**
- `backend/src/auth_provider.rs` defines the `IdentityProvider` interface: `provider_id`, `display_name`, `authorization_url(csrf)`, `exchange_code(code) → AuthenticatedIdentity`.
- `OidcProvider` implements it generically from pure data (auth/token/userinfo endpoints + client credentials). Google is just an `OidcProvider` record in `server.config.toml`; Microsoft or an enterprise OIDC domain is another record — zero new code.
- Login/callback routes are provider-parameterized (`/api/auth/{provider}/login`); handlers know no provider specifics.
- The AKA table keys identities by `(provider, subject)` (Impl Spec §2.9), so user records absorb any number of domains.
- Downstream of `AuthenticatedIdentity`, everything (sessions, users, authorization) is provider-blind.

**Obligations.**
- No provider-specific branch may appear in handlers, session logic, user resolution, or authorization.
- New protocols (e.g. SAML) are added as new `IdentityProvider` implementations, not by widening existing ones.
- The provider registry and AKA table remain the only places that know providers exist.

**Verified by.** `backend/tests/auth_flow.rs`: registering a second provider from pure data makes it immediately loginable, with zero changes elsewhere; grep-level review that `google` appears nowhere in backend source except configuration data.

## T3 — Authentication domains can be added while the application is running

**Statement.** An authentication domain can be added (and later disabled) at runtime, without restarting, redeploying, or rebuilding the application.

**Current basis.** The provider registry (`ProviderRegistry`) is runtime-mutable: providers registered after startup are immediately usable by the login routes and listed to the launcher via `/api/auth/config`. Startup registration from `server.config.toml` is just the first use of the same mechanism.

**Obligations.**
- Provider lookup happens per-request against the registry — never captured at startup into route tables, caches, or per-provider routes.
- The future admin surface for adding a provider must be an authorized workflow recording an administrative ledger event (Axiom 8), reusing `ProviderRegistry::register`.
- Whenever authentication/authorization code is added, changed, or removed, re-verify this theorem: no change may reintroduce a startup-only provider set.

**Verified by.** `auth_flow.rs` test: a provider registered *after* the router is built serves logins through the existing routes.

## T4 — Multiple identities of one person can be unified

**Statement.** A person holding identities in more than one authentication domain resolves to a single authorized user, and identities that arrive as separate users can later be merged into one — by an explicit, auditable operation.

**Current basis.**
- Automatic: the AKA table maps every `(provider, subject)` to a `user_id`; a new identity presenting an already-known verified email attaches to the existing user (`UserStore::resolve_identity`).
- Deliberate merge (identities with different emails): a planned **identity-merge workflow** — the user proves control of both identities by logging into each, and the two user records merge into one with combined authorizations (Impl Spec §2.9). Scheduled once workflow machinery exists (M5+); the merge is recorded as an administrative ledger event.

**Obligations.**
- The AKA table remains the single mapping from identities to users; no feature may key durable authority off a provider-specific identity directly.
- Merging must be user-initiated and control-proven; the system verifies control of both identities, nothing more.
- Merges (and any future un-merges) are auditable events, never silent row updates.

**Verified by.** `auth_flow.rs` test: same verified email via two different providers yields one `user_id`. Merge-workflow tests arrive with the workflow.

## T5 — Extension never bypasses the authority path

**Statement.** Every extension mechanism (new auth provider, new workflow, new MCP primitive, new storage driver) feeds into the same session → user → authorization → engine-invariant path. No extension point may create a second door.

**Current basis.** Providers produce only an `AuthenticatedIdentity`; workflows call only backend APIs; MCP and dev-time services hold no storage credentials; the engine re-checks all invariants regardless of caller (Impl Spec §5.5).

**Obligations.** Any new extension point must be reviewed against this theorem before it is built.

**Verified by.** M1 authorization tests; M5 workflow-scope tests; standing code review rule.

---

*Candidate future theorems (promote when their basis exists): deployment topology can be split without code change (Impl Spec §7.1); a book is portable across deployments without losing operational continuity (§7.3).*
