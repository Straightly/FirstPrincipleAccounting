# LedgerZero (FirstPrincipleAccounting)

A first-principles, AI-native accounting platform. Authoritative documents:

- `docs/LedgerZero_Impl_Spec_v1.md` — implementation spec (build from this)
- `docs/LedgerZero_Impl_Plan_v1.md` — milestone plan
- `docs/LedgerZero_Theorems.md` — standing architectural guarantees every change must preserve
- `docs/LedgerZero_Spec.md` — original vision/design document

## Layout

- `engine/` — Rust crate: the `AccountingEngine` (invariants, domain, storage boundary)
- `backend/` — Rust crate: routing server + runtime backend (Axum); the only component with storage access
- `frontend/` — React + Vite launcher (login, session, workflow menu); each workflow is later deployed as its own self-contained React app
- `mcp_server/` — Python MCP server + dev-time backend (LLM/workflow generation); no accounting storage access
- `scripts/check.sh` — builds and tests everything

## Getting started

Prerequisites: Rust (rustup.rs), Node.js 20+, Python 3.11+.

```bash
./scripts/check.sh                                 # build + test all components
cp server.config.example.toml server.config.toml   # then edit
(cd frontend && npm install && npm run build)
cargo run -p ledgerzero-backend                    # serves http://localhost:8080
```

`server.config.toml` (gitignored) holds the bootstrap owner email and Google
OAuth client credentials (Impl Spec §5.3). For local development without OAuth
credentials, set `[dev_login] enabled = true` — never on a network-reachable
deployment.

Frontend development with hot reload: `cd frontend && npm run dev` (proxies
`/api` to the backend on :8080).
