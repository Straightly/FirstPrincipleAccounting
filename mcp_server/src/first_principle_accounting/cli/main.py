"""Local runner (Impl Spec §7.1): run the stdio MCP server, or invoke a
single primitive directly for local testing/dogfooding without a full MCP
client — the same `mcp.tools` functions the server itself calls, just
dispatched straight from the command line.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from typing import Any

from .. import runtime_client
from ..config import load_config
from ..mcp import tools
from ..mcp.server import create_server

# Every tool `run-tool` can dispatch to, and what session-like argument (if
# any) it expects as its first parameter — mirrors mcp/server.py's wiring
# one level down, without the MCP protocol in the way.
_NEEDS_SESSION = {
    "deploy_workflow_definition",
    "list_workflows",
    "get_workflow_definition",
    "create_accounting_book",
    "create_resource_type",
    "create_chart",
    "copy_chart",
    "create_account",
    "create_period",
    "close_period",
    "reopen_period",
    "create_role",
    "assign_workflow_to_role",
    "assign_role_to_user",
}


def _serve() -> None:
    create_server().run()


async def _run_tool(name: str, args: dict[str, Any]) -> Any:
    fn = getattr(tools, name, None)
    if fn is None or name.startswith("_"):
        raise SystemExit(f"unknown tool: {name!r}")
    if name not in _NEEDS_SESSION:
        return await fn(**args)

    config = load_config()
    client = runtime_client.RuntimeBackendClient(config.backend_base_url)
    try:
        await client.dev_login(config.dev_login_email)
        session = tools.Session(client=client, config=config)
        return await fn(session, **args)
    finally:
        await client.aclose()


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="first-principle-accounting-mcp")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("serve", help="run the stdio MCP server")

    run_tool = subparsers.add_parser(
        "run-tool", help="invoke one primitive directly, for local testing"
    )
    run_tool.add_argument("tool_name")
    run_tool.add_argument(
        "--json",
        dest="json_args",
        default="{}",
        help="JSON object of keyword arguments for the tool",
    )

    args = parser.parse_args(argv)

    if args.command == "serve":
        _serve()
        return

    if args.command == "run-tool":
        kwargs = json.loads(args.json_args)
        result = asyncio.run(_run_tool(args.tool_name, kwargs))
        json.dump(result, sys.stdout, indent=2)
        sys.stdout.write("\n")
        return


if __name__ == "__main__":
    main()
