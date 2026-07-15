"""MCP primitive implementations (Impl Spec §6.4).

Plain, `Session`-injected async functions — no MCP SDK import here, so
these are unit-testable against a fake client with no protocol machinery in
the way. `mcp/server.py` is the thin adapter that registers each of these
as an actual MCP tool; `cli/main.py`'s `run-tool` dispatches to them
directly for local testing.

Every primitive either calls the runtime backend's HTTP API or reads the
local dev artifact store (Impl Spec §7.4) — never a book file directly
(Axiom 12). `generate_workflow_definition`/`deploy_workflow_definition`/
`list_workflows`/`get_workflow_definition` are the AI-generation path
primitives (Impl Plan M8); the rest are the admin primitives from §6.4 that
already have a backend endpoint to call (sub-book, consolidation, and
reconciliation-explanation primitives are not implemented yet — those
features don't exist on the backend until M11/M12).
"""

from __future__ import annotations

import dataclasses
import json
import uuid
from typing import Any

from ..config import Config
from ..devtime.artifacts import prepare_artifact, workflow_dir
from ..devtime.generator import (
    FormField,
    GeneratedWorkflow,
    WorkflowGenerationRequest,
)
from ..devtime.generator import generate_workflow_definition as _generate
from ..runtime_client import RuntimeBackendClient


@dataclasses.dataclass
class Session:
    """The one thing every backend-touching primitive needs: an
    authenticated client and the local paths for artifact work."""

    client: RuntimeBackendClient
    config: Config


# ---------------------------------------------------------------------------
# AI-generation path (Impl Plan M8)
# ---------------------------------------------------------------------------


def _generated_to_dict(generated: GeneratedWorkflow) -> dict[str, Any]:
    return {
        "workflow_id": str(generated.workflow_id),
        "workflow_name": generated.workflow_name,
        "description": generated.description,
        "backend_api_calls": generated.backend_api_calls,
        "required_inputs": generated.required_inputs,
        "workflow_json": generated.workflow_json,
        "manifest_json": generated.manifest_json,
        "index_html": generated.index_html,
        "app_js": generated.app_js,
    }


def _dict_to_generated(data: dict[str, Any]) -> GeneratedWorkflow:
    return GeneratedWorkflow(
        workflow_id=uuid.UUID(data["workflow_id"]),
        workflow_name=data["workflow_name"],
        description=data["description"],
        backend_api_calls=list(data["backend_api_calls"]),
        required_inputs=dict(data["required_inputs"]),
        workflow_json=dict(data["workflow_json"]),
        manifest_json=dict(data["manifest_json"]),
        index_html=data["index_html"],
        app_js=data["app_js"],
    )


async def generate_workflow_definition(
    *,
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
    """Deterministic template generation (v1's "LLM wrapping", see
    devtime/generator.py). Needs no backend session — the only primitive
    here that doesn't. Returns the full generated definition; pass it
    straight through to `deploy_workflow_definition` to deploy it.
    """
    form_fields = tuple(
        FormField(
            id=f["id"],
            label=f["label"],
            input_type=f["input_type"],
            required=f.get("required", True),
            placeholder=f.get("placeholder"),
            options=tuple(tuple(o) for o in f["options"])
            if f.get("options")
            else None,
        )
        for f in fields
    )
    request = WorkflowGenerationRequest(
        workflow_name=workflow_name,
        description=description,
        spec_reference=spec_reference,
        fields=form_fields,
        date_field=date_field,
        description_field=description_field,
        amount_field=amount_field,
        primary_account_field=primary_account_field,
        offset_account_field=offset_account_field,
        submit_label=submit_label,
        memo_field=memo_field,
        direction_field=direction_field,
        direction_primary_debit_value=direction_primary_debit_value,
        backend_api_calls=tuple(backend_api_calls or ("post_entry",)),
    )
    return _generated_to_dict(_generate(request))


async def deploy_workflow_definition(
    session: Session,
    *,
    generated: dict[str, Any],
    book_id: str,
    entity_id: str,
) -> dict[str, Any]:
    """Writes the artifact (Impl Spec §7.4) and registers the deployment.
    Each call mints a fresh `workflow_deployment_id`, so redeploying the
    same generated definition produces a new, independently auditable
    artifact rather than mutating one in place.
    """
    gen = _dict_to_generated(generated)
    deployment_id = uuid.uuid4()
    prepare_artifact(gen, deployment_id, session.config)
    # The response's `id` is the deployment id again (deploy_workflow's
    # idempotency key), not a separate handle — deploying also auto-creates
    # a same-named role (Impl Spec §2.9); discover its id via list_roles.
    await session.client.deploy_workflow(
        uuid.UUID(book_id),
        workflow_deployment_id=deployment_id,
        workflow_id=gen.workflow_id,
        entity_id=uuid.UUID(entity_id),
        workflow_name=gen.workflow_name,
        description=gen.description,
        backend_api_calls=gen.backend_api_calls,
        required_inputs=gen.required_inputs,
    )
    return {
        "workflow_deployment_id": str(deployment_id),
        "workflow_id": str(gen.workflow_id),
        "frontend_route": f"/workflows/{deployment_id}/code/index.html",
    }


async def list_workflows(
    session: Session, *, book_id: str, entity_id: str
) -> list[dict[str, Any]]:
    return await session.client.list_workflows(
        uuid.UUID(book_id), uuid.UUID(entity_id)
    )


async def get_workflow_definition(
    session: Session, *, workflow_deployment_id: str
) -> dict[str, Any]:
    """Reads `workflow.json`/`manifest.json` back from the dev artifact
    store (Impl Spec §7.4: "inspectable by authorized users") — this is
    dev-time tooling data, not accounting data, so reading it directly off
    disk does not cross the Axiom 12 boundary the runtime backend guards.
    """
    dep_id = uuid.UUID(workflow_deployment_id)
    directory = workflow_dir(session.config, dep_id)
    workflow_path = directory / "workflow.json"
    manifest_path = directory / "manifest.json"
    if not workflow_path.is_file():
        raise FileNotFoundError(
            f"no workflow.json for deployment {workflow_deployment_id} in {directory}"
        )
    return {
        "workflow_deployment_id": str(dep_id),
        "workflow": json.loads(workflow_path.read_text()),
        "manifest": json.loads(manifest_path.read_text())
        if manifest_path.is_file()
        else None,
    }


# ---------------------------------------------------------------------------
# Admin primitives with an existing backend endpoint (Impl Spec §6.4)
# ---------------------------------------------------------------------------


async def create_accounting_book(
    session: Session, *, name: str, passphrase: str
) -> dict[str, Any]:
    return await session.client.create_accounting_book(name, passphrase)


async def create_resource_type(
    session: Session,
    *,
    book_id: str,
    name: str,
    kind: str,
    code: str,
    unit_of_measure: str,
    precision: int,
    metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return await session.client.create_resource_type(
        uuid.UUID(book_id),
        name=name,
        kind=kind,
        code=code,
        unit_of_measure=unit_of_measure,
        precision=precision,
        metadata=metadata,
    )


async def create_chart(
    session: Session,
    *,
    book_id: str,
    entity_id: str,
    name: str,
    description: str | None = None,
    activate: bool = True,
) -> dict[str, Any]:
    return await session.client.create_chart(
        uuid.UUID(book_id),
        entity_id=uuid.UUID(entity_id),
        name=name,
        description=description,
        activate=activate,
    )


async def copy_chart(
    session: Session,
    *,
    book_id: str,
    chart_id: str,
    name: str,
    description: str | None = None,
    activate: bool = False,
) -> dict[str, Any]:
    return await session.client.copy_chart(
        uuid.UUID(book_id),
        uuid.UUID(chart_id),
        name=name,
        description=description,
        activate=activate,
    )


async def create_account(
    session: Session,
    *,
    book_id: str,
    chart_id: str,
    name: str,
    account_type: str,
    resource_type_id: str,
    code: str | None = None,
    parent_account_id: str | None = None,
    validation_rules: dict[str, Any] | None = None,
    metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return await session.client.create_account(
        uuid.UUID(book_id),
        chart_id=uuid.UUID(chart_id),
        name=name,
        account_type=account_type,
        resource_type_id=uuid.UUID(resource_type_id),
        code=code,
        parent_account_id=uuid.UUID(parent_account_id)
        if parent_account_id
        else None,
        validation_rules=validation_rules,
        metadata=metadata,
    )


async def create_period(
    session: Session,
    *,
    book_id: str,
    entity_id: str,
    name: str,
    start_date: str,
    end_date: str,
) -> dict[str, Any]:
    return await session.client.create_period(
        uuid.UUID(book_id),
        entity_id=uuid.UUID(entity_id),
        name=name,
        start_date=start_date,
        end_date=end_date,
    )


async def close_period(
    session: Session, *, book_id: str, period_id: str
) -> dict[str, Any]:
    return await session.client.close_period(
        uuid.UUID(book_id), uuid.UUID(period_id)
    )


async def reopen_period(
    session: Session, *, book_id: str, period_id: str
) -> dict[str, Any]:
    return await session.client.reopen_period(
        uuid.UUID(book_id), uuid.UUID(period_id)
    )


async def create_role(
    session: Session,
    *,
    book_id: str,
    entity_id: str,
    name: str,
    description: str | None = None,
) -> dict[str, Any]:
    return await session.client.create_role(
        uuid.UUID(book_id),
        entity_id=uuid.UUID(entity_id),
        name=name,
        description=description,
    )


async def assign_workflow_to_role(
    session: Session,
    *,
    book_id: str,
    role_id: str,
    workflow_id: str,
) -> dict[str, Any]:
    return await session.client.assign_workflow_to_role(
        uuid.UUID(book_id), uuid.UUID(role_id), uuid.UUID(workflow_id)
    )


async def assign_role_to_user(
    session: Session,
    *,
    book_id: str,
    role_id: str,
    user_id: str,
) -> dict[str, Any]:
    return await session.client.assign_role_to_user(
        uuid.UUID(book_id), uuid.UUID(role_id), uuid.UUID(user_id)
    )
