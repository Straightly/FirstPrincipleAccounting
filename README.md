# FirstPrincipleAccounting

Starter layout for a first-principles accounting system with:

- `frontend/` reserved for a future user-facing application
- `mcp_server/` for the Python MCP server and accounting engine layers

## Structure

- `frontend/`
  - Empty placeholder for now
- `mcp_server/`
  - `src/first_principle_accounting/domain/` domain model and invariants
  - `src/first_principle_accounting/application/` use-case layer and ports
  - `src/first_principle_accounting/storage/` storage abstractions and file driver
  - `src/first_principle_accounting/engine/` ledger engine orchestration
  - `src/first_principle_accounting/mcp/` MCP-facing adapter layer
  - `src/first_principle_accounting/cli/` local bootstrap and entry points
  - `tests/` test package placeholder
