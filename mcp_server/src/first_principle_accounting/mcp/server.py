"""MCP server bootstrap and tool registration (Impl Spec §6.4, §7.1).

Thin adapter layer: every tool body here is a one-line call into
`mcp.tools`, which holds the actual logic and has no dependency on the MCP
SDK. This file's only job is protocol wiring — turning each primitive into
something an MCP client can discover and call, and supplying it with the
one long-lived `RuntimeBackendClient` session for the process's lifetime
(Impl Spec §6.2: the developer is the sole deploy authority in v1, so one
dev-login session per server process is the right lifetime).
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

from mcp.server.fastmcp import Context, FastMCP

from . import tools
from ..config import Config, load_config
from ..runtime_client import RuntimeBackendClient


def create_server(config: Config | None = None) -> FastMCP:
    cfg = config or load_config()

    @asynccontextmanager
    async def lifespan(_server: FastMCP) -> AsyncIterator[tools.Session]:
        client = RuntimeBackendClient(cfg.backend_base_url)
        await client.dev_login(cfg.dev_login_email)
        try:
            yield tools.Session(client=client, config=cfg)
        finally:
            await client.aclose()

    server = FastMCP("first-principle-accounting", lifespan=lifespan)

    def ctx_of(ctx: Context) -> tools.Session:
        return ctx.request_context.lifespan_context

    @server.tool()
    async def generate_workflow_definition(
        workflow_name: str,
        description: str,
        spec_reference: str,
        fields: list[dict[str, Any]],
        date_field: str,
        description_field: str,
        amount_field: str,
        primary_account_field: str,
        offset_account_field: str,
        submit_label: str,
        memo_field: str | None = None,
        direction_field: str | None = None,
        direction_primary_debit_value: str | None = None,
        backend_api_calls: list[str] | None = None,
    ) -> dict[str, Any]:
        """Generate a workflow definition (form fields + posting rule) from
        a structured request — the AI-generation entry point (Impl Plan
        M8). Returns the full definition; pass it to
        deploy_workflow_definition to deploy it."""
        return await tools.generate_workflow_definition(
            workflow_name=workflow_name,
            description=description,
            spec_reference=spec_reference,
            fields=fields,
            date_field=date_field,
            description_field=description_field,
            amount_field=amount_field,
            primary_account_field=primary_account_field,
            offset_account_field=offset_account_field,
            submit_label=submit_label,
            memo_field=memo_field,
            direction_field=direction_field,
            direction_primary_debit_value=direction_primary_debit_value,
            backend_api_calls=backend_api_calls,
        )

    @server.tool()
    async def deploy_workflow_definition(
        generated: dict[str, Any], book_id: str, entity_id: str, ctx: Context
    ) -> dict[str, Any]:
        """Write the artifact and register the deployment for a definition
        returned by generate_workflow_definition."""
        return await tools.deploy_workflow_definition(
            ctx_of(ctx),
            generated=generated,
            book_id=book_id,
            entity_id=entity_id,
        )

    @server.tool()
    async def list_workflows(
        book_id: str, entity_id: str, ctx: Context
    ) -> list[dict[str, Any]]:
        """Every deployed workflow in an entity."""
        return await tools.list_workflows(
            ctx_of(ctx), book_id=book_id, entity_id=entity_id
        )

    @server.tool()
    async def get_workflow_definition(
        workflow_deployment_id: str, ctx: Context
    ) -> dict[str, Any]:
        """Read a deployed workflow's definition back from the dev artifact
        store for inspection (Impl Spec §7.4)."""
        return await tools.get_workflow_definition(
            ctx_of(ctx), workflow_deployment_id=workflow_deployment_id
        )

    @server.tool()
    async def create_accounting_book(
        name: str, passphrase: str, ctx: Context
    ) -> dict[str, Any]:
        """Create a new book (auto-creates its one entity, Impl Plan M7)."""
        return await tools.create_accounting_book(
            ctx_of(ctx), name=name, passphrase=passphrase
        )

    @server.tool()
    async def create_resource_type(
        book_id: str,
        name: str,
        kind: str,
        code: str,
        unit_of_measure: str,
        precision: int,
        ctx: Context,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return await tools.create_resource_type(
            ctx_of(ctx),
            book_id=book_id,
            name=name,
            kind=kind,
            code=code,
            unit_of_measure=unit_of_measure,
            precision=precision,
            metadata=metadata,
        )

    @server.tool()
    async def create_chart(
        book_id: str,
        entity_id: str,
        name: str,
        ctx: Context,
        description: str | None = None,
        activate: bool = True,
    ) -> dict[str, Any]:
        return await tools.create_chart(
            ctx_of(ctx),
            book_id=book_id,
            entity_id=entity_id,
            name=name,
            description=description,
            activate=activate,
        )

    @server.tool()
    async def copy_chart(
        book_id: str,
        chart_id: str,
        name: str,
        ctx: Context,
        description: str | None = None,
        activate: bool = False,
    ) -> dict[str, Any]:
        return await tools.copy_chart(
            ctx_of(ctx),
            book_id=book_id,
            chart_id=chart_id,
            name=name,
            description=description,
            activate=activate,
        )

    @server.tool()
    async def create_account(
        book_id: str,
        chart_id: str,
        name: str,
        account_type: str,
        resource_type_id: str,
        ctx: Context,
        code: str | None = None,
        parent_account_id: str | None = None,
        validation_rules: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return await tools.create_account(
            ctx_of(ctx),
            book_id=book_id,
            chart_id=chart_id,
            name=name,
            account_type=account_type,
            resource_type_id=resource_type_id,
            code=code,
            parent_account_id=parent_account_id,
            validation_rules=validation_rules,
            metadata=metadata,
        )

    @server.tool()
    async def create_period(
        book_id: str,
        entity_id: str,
        name: str,
        start_date: str,
        end_date: str,
        ctx: Context,
    ) -> dict[str, Any]:
        return await tools.create_period(
            ctx_of(ctx),
            book_id=book_id,
            entity_id=entity_id,
            name=name,
            start_date=start_date,
            end_date=end_date,
        )

    @server.tool()
    async def close_period(book_id: str, period_id: str, ctx: Context) -> dict[str, Any]:
        return await tools.close_period(
            ctx_of(ctx), book_id=book_id, period_id=period_id
        )

    @server.tool()
    async def reopen_period(book_id: str, period_id: str, ctx: Context) -> dict[str, Any]:
        return await tools.reopen_period(
            ctx_of(ctx), book_id=book_id, period_id=period_id
        )

    @server.tool()
    async def create_role(
        book_id: str,
        entity_id: str,
        name: str,
        ctx: Context,
        description: str | None = None,
    ) -> dict[str, Any]:
        return await tools.create_role(
            ctx_of(ctx),
            book_id=book_id,
            entity_id=entity_id,
            name=name,
            description=description,
        )

    @server.tool()
    async def assign_workflow_to_role(
        book_id: str, role_id: str, workflow_id: str, ctx: Context
    ) -> dict[str, Any]:
        return await tools.assign_workflow_to_role(
            ctx_of(ctx),
            book_id=book_id,
            role_id=role_id,
            workflow_id=workflow_id,
        )

    @server.tool()
    async def assign_role_to_user(
        book_id: str, role_id: str, user_id: str, ctx: Context
    ) -> dict[str, Any]:
        return await tools.assign_role_to_user(
            ctx_of(ctx), book_id=book_id, role_id=role_id, user_id=user_id
        )

    return server


def main() -> None:
    create_server().run()


if __name__ == "__main__":
    main()
