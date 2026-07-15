# MCP server + Python dev-time backend

The MCP server and workflow-generation ("dev-time") backend for LedgerZero
(Impl Spec §6.4, §7.1). Owns MCP primitives, prompt/context assembly,
generated-artifact preparation, and deployment support — nothing else. The
Rust `engine`/`backend` crates own every accounting invariant and all
storage; this package never opens a book file and holds no accounting
storage credentials (Axiom 12). Everything it knows about a book comes from
calling the runtime backend's authenticated HTTP API, exactly like a
browser would.

## Layout

- `config.py` — `Config`: the backend's HTTP base URL, the dev-login
  identity this process authenticates as, and local filesystem paths
  (dev artifact store, vendored React). Read from `LZ_MCP_*` env vars.
- `runtime_client.py` — `RuntimeBackendClient`: the only way anything here
  touches accounting data. One method per backend endpoint.
- `errors.py` — `BackendApiError`, mirroring the backend's own
  `{error_code, message, details}` error shape.
- `devtime/generator.py` — `generate_workflow_definition`: the "LLM
  wrapping" seam (Impl Spec §6.2). v1 generates deterministically from a
  structured request rather than calling a live model — a real model call
  can replace this function's body later without changing anything
  downstream.
- `devtime/artifacts.py` — `prepare_artifact`: writes a generated
  definition to the dev artifact store (Impl Spec §7.4) in the same layout
  hand-written artifacts use.
- `mcp/tools.py` — the MCP primitives (Impl Spec §6.4) as plain,
  `Session`-injected async functions. No MCP SDK dependency — unit-testable
  on their own.
- `mcp/server.py` — `create_server`: registers each `mcp/tools.py`
  primitive as an actual MCP tool via the `mcp` SDK's `FastMCP`.
- `cli/main.py` — `serve` runs the stdio MCP server; `run-tool <name>
  --json '<args>'` invokes one primitive directly, for local testing
  without a full MCP client.

## Local setup

```sh
cd mcp_server
python3 -m venv .venv
.venv/bin/pip install -e .
.venv/bin/python -m first_principle_accounting.cli.main serve
```

`serve` expects a running LedgerZero backend (`cargo run -p
ledgerzero-backend` from the repo root) with `dev_login` enabled, and logs
in as `LZ_MCP_DEV_LOGIN_EMAIL` (defaults to the bootstrap owner) — the
developer is the sole deploy authority in v1 (Impl Spec §6.2).
