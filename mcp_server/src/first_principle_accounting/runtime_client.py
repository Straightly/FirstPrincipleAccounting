"""HTTP client for the Rust runtime backend (Impl Spec §7.1, §6.5).

This is the *only* way anything in this package touches accounting data —
by calling the same authenticated HTTP API a browser would, and never by
opening a book file directly (Axiom 12: no storage credentials, no
persisted private accounting context). Every method here maps 1:1 onto an
existing backend endpoint; nothing here invents new authority.
"""

from __future__ import annotations

import uuid
from typing import Any

import httpx

from .errors import BackendApiError


class RuntimeBackendClient:
    """A logged-in session against one running backend instance.

    Not thread-safe beyond what `httpx.AsyncClient` itself guarantees;
    intended for one dev-time/MCP process talking to one backend.
    """

    def __init__(self, base_url: str, client: httpx.AsyncClient | None = None) -> None:
        self._client = client or httpx.AsyncClient(base_url=base_url, timeout=30.0)

    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "RuntimeBackendClient":
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.aclose()

    async def _request(
        self, method: str, path: str, *, json: dict[str, Any] | None = None
    ) -> Any:
        response = await self._client.request(method, path, json=json)
        if response.status_code >= 400:
            body: dict[str, Any] = {}
            try:
                body = response.json()
            except ValueError:
                pass
            raise BackendApiError(
                status=response.status_code,
                error_code=body.get("error_code", "UNKNOWN_ERROR"),
                message=body.get("message", response.text),
                details=body.get("details"),
            )
        if response.status_code == 204 or not response.content:
            return None
        return response.json()

    # -- Auth -----------------------------------------------------------

    async def dev_login(self, email: str) -> dict[str, Any]:
        """Dev-only login (Impl Spec §5.2); the deploy authority in v1 is
        this one developer identity (Impl Spec §6.2)."""
        return await self._request(
            "POST", "/api/auth/dev-login", json={"email": email}
        )

    async def me(self) -> dict[str, Any]:
        return await self._request("GET", "/api/auth/me")

    # -- Books ------------------------------------------------------------

    async def create_accounting_book(
        self, name: str, passphrase: str
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            "/api/books",
            json={"name": name, "passphrase": passphrase},
        )

    async def open_book(self, book_id: uuid.UUID, passphrase: str) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/open",
            json={"passphrase": passphrase},
        )

    async def list_my_books(self) -> list[dict[str, Any]]:
        return await self._request("GET", "/api/books/mine")

    # -- Reference ----------------------------------------------------------

    async def create_resource_type(
        self,
        book_id: uuid.UUID,
        *,
        name: str,
        kind: str,
        code: str,
        unit_of_measure: str,
        precision: int,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/resource-types",
            json={
                "op_id": str(uuid.uuid4()),
                "name": name,
                "kind": kind,
                "code": code,
                "unit_of_measure": unit_of_measure,
                "precision": precision,
                "metadata": metadata or {},
            },
        )

    async def list_resource_types(self, book_id: uuid.UUID) -> list[dict[str, Any]]:
        return await self._request("GET", f"/api/books/{book_id}/resource-types")

    async def create_chart(
        self,
        book_id: uuid.UUID,
        *,
        entity_id: uuid.UUID,
        name: str,
        description: str | None = None,
        activate: bool = True,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/charts",
            json={
                "op_id": str(uuid.uuid4()),
                "entity_id": str(entity_id),
                "name": name,
                "description": description,
                "activate": activate,
            },
        )

    async def copy_chart(
        self,
        book_id: uuid.UUID,
        chart_id: uuid.UUID,
        *,
        name: str,
        description: str | None = None,
        activate: bool = False,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/charts/{chart_id}/copy",
            json={
                "op_id": str(uuid.uuid4()),
                "name": name,
                "description": description,
                "activate": activate,
            },
        )

    async def list_charts(
        self, book_id: uuid.UUID, entity_id: uuid.UUID
    ) -> list[dict[str, Any]]:
        return await self._request(
            "GET", f"/api/books/{book_id}/charts?entity_id={entity_id}"
        )

    async def create_account(
        self,
        book_id: uuid.UUID,
        *,
        chart_id: uuid.UUID,
        name: str,
        account_type: str,
        resource_type_id: uuid.UUID,
        code: str | None = None,
        parent_account_id: uuid.UUID | None = None,
        validation_rules: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/accounts",
            json={
                "op_id": str(uuid.uuid4()),
                "chart_id": str(chart_id),
                "name": name,
                "code": code,
                "account_type": account_type,
                "resource_type_id": str(resource_type_id),
                "parent_account_id": str(parent_account_id)
                if parent_account_id
                else None,
                "validation_rules": validation_rules or {},
                "metadata": metadata or {},
            },
        )

    async def list_accounts(
        self, book_id: uuid.UUID, chart_id: uuid.UUID
    ) -> list[dict[str, Any]]:
        return await self._request(
            "GET", f"/api/books/{book_id}/accounts?chart_id={chart_id}"
        )

    async def create_period(
        self,
        book_id: uuid.UUID,
        *,
        entity_id: uuid.UUID,
        name: str,
        start_date: str,
        end_date: str,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/periods",
            json={
                "op_id": str(uuid.uuid4()),
                "entity_id": str(entity_id),
                "name": name,
                "start_date": start_date,
                "end_date": end_date,
            },
        )

    async def close_period(
        self, book_id: uuid.UUID, period_id: uuid.UUID
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/periods/{period_id}/close",
            json={"op_id": str(uuid.uuid4())},
        )

    async def reopen_period(
        self, book_id: uuid.UUID, period_id: uuid.UUID
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/periods/{period_id}/reopen",
            json={"op_id": str(uuid.uuid4())},
        )

    async def list_periods(
        self, book_id: uuid.UUID, entity_id: uuid.UUID
    ) -> list[dict[str, Any]]:
        return await self._request(
            "GET", f"/api/books/{book_id}/periods?entity_id={entity_id}"
        )

    # -- Workflows and roles (Impl Plan M5) --------------------------------

    async def deploy_workflow(
        self,
        book_id: uuid.UUID,
        *,
        workflow_deployment_id: uuid.UUID,
        workflow_id: uuid.UUID,
        entity_id: uuid.UUID,
        workflow_name: str,
        description: str | None,
        backend_api_calls: list[str],
        required_inputs: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/workflows/deploy",
            json={
                "workflow_deployment_id": str(workflow_deployment_id),
                "workflow_id": str(workflow_id),
                "entity_id": str(entity_id),
                "workflow_name": workflow_name,
                "description": description,
                "backend_api_calls": backend_api_calls,
                "required_inputs": required_inputs or {},
                "metadata": metadata or {},
            },
        )

    async def list_workflows(
        self, book_id: uuid.UUID, entity_id: uuid.UUID
    ) -> list[dict[str, Any]]:
        return await self._request(
            "GET", f"/api/books/{book_id}/workflows?entity_id={entity_id}"
        )

    async def create_role(
        self,
        book_id: uuid.UUID,
        *,
        entity_id: uuid.UUID,
        name: str,
        description: str | None = None,
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/roles",
            json={
                "op_id": str(uuid.uuid4()),
                "entity_id": str(entity_id),
                "name": name,
                "description": description,
            },
        )

    async def list_roles(
        self, book_id: uuid.UUID, entity_id: uuid.UUID
    ) -> list[dict[str, Any]]:
        return await self._request(
            "GET", f"/api/books/{book_id}/roles?entity_id={entity_id}"
        )

    async def assign_workflow_to_role(
        self, book_id: uuid.UUID, role_id: uuid.UUID, workflow_id: uuid.UUID
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/roles/{role_id}/workflows",
            json={"op_id": str(uuid.uuid4()), "workflow_id": str(workflow_id)},
        )

    async def assign_role_to_user(
        self, book_id: uuid.UUID, role_id: uuid.UUID, user_id: uuid.UUID
    ) -> dict[str, Any]:
        return await self._request(
            "POST",
            f"/api/books/{book_id}/roles/{role_id}/users",
            json={"op_id": str(uuid.uuid4()), "user_id": str(user_id)},
        )
