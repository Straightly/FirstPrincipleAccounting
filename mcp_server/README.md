# MCP Server

This package is the starting point for Ledger Zero style development in Python.

The intended separation is:

- `domain`: accounting concepts, entities, invariants
- `application`: engine-facing use cases and ports
- `storage`: file-backed event store and future drivers
- `engine`: the only layer allowed to mutate accounting storage
- `mcp`: MCP tool exposure and orchestration
- `cli`: local bootstrap for in-process or subprocess execution modes

The initial deployment target is a local Python runtime with strict storage
separation and file-backed append-only storage.
