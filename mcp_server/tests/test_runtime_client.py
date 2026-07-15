"""RuntimeBackendClient tests, against a mocked transport — no live
backend needed (Impl Plan M8, Project Plan #11: "MCP never reads or writes
book files directly; MCP calls runtime backend APIs for runtime facts").
"""

import asyncio
import sys
import unittest
import uuid
from pathlib import Path

import httpx

SRC = Path(__file__).resolve().parents[1] / "src"
sys.path.insert(0, str(SRC))

from first_principle_accounting.errors import BackendApiError  # noqa: E402
from first_principle_accounting.runtime_client import (  # noqa: E402
    RuntimeBackendClient,
)


def _client(handler) -> RuntimeBackendClient:
    transport = httpx.MockTransport(handler)
    http_client = httpx.AsyncClient(
        transport=transport, base_url="http://backend.local"
    )
    return RuntimeBackendClient("http://backend.local", client=http_client)


class TestRuntimeBackendClient(unittest.TestCase):
    def test_dev_login_posts_email(self):
        seen = {}

        def handler(request: httpx.Request) -> httpx.Response:
            seen["method"] = request.method
            seen["url"] = str(request.url)
            seen["body"] = request.read()
            return httpx.Response(
                200, json={"user": {"user_id": str(uuid.uuid4())}}
            )

        client = _client(handler)
        result = asyncio.run(client.dev_login("owner@example.com"))
        self.assertEqual(seen["method"], "POST")
        self.assertTrue(seen["url"].endswith("/api/auth/dev-login"))
        self.assertIn(b"owner@example.com", seen["body"])
        self.assertIn("user", result)

    def test_error_response_raises_backend_api_error_with_parsed_fields(self):
        def handler(_request: httpx.Request) -> httpx.Response:
            return httpx.Response(
                409,
                json={
                    "error_code": "IDEMPOTENCY_CONFLICT",
                    "message": "tampered payload",
                },
            )

        client = _client(handler)
        with self.assertRaises(BackendApiError) as ctx:
            asyncio.run(
                client.create_accounting_book("Acme", "correct horse battery")
            )
        self.assertEqual(ctx.exception.status, 409)
        self.assertEqual(ctx.exception.error_code, "IDEMPOTENCY_CONFLICT")
        self.assertEqual(ctx.exception.message, "tampered payload")

    def test_error_response_with_non_json_body_still_raises(self):
        def handler(_request: httpx.Request) -> httpx.Response:
            return httpx.Response(500, text="internal server error")

        client = _client(handler)
        with self.assertRaises(BackendApiError) as ctx:
            asyncio.run(
                client.create_accounting_book("Acme", "correct horse battery")
            )
        self.assertEqual(ctx.exception.status, 500)
        self.assertEqual(ctx.exception.error_code, "UNKNOWN_ERROR")

    def test_create_accounting_book_request_shape(self):
        seen = {}

        def handler(request: httpx.Request) -> httpx.Response:
            seen["json"] = request.read()
            return httpx.Response(
                200,
                json={
                    "book_id": str(uuid.uuid4()),
                    "name": "Acme",
                    "entity_id": str(uuid.uuid4()),
                },
            )

        client = _client(handler)
        result = asyncio.run(
            client.create_accounting_book("Acme", "correct horse battery staple")
        )
        self.assertIn(b'"name":"Acme"', seen["json"])
        self.assertEqual(result["name"], "Acme")
        self.assertIn("entity_id", result)

    def test_deploy_workflow_serializes_uuids_as_strings(self):
        seen = {}

        def handler(request: httpx.Request) -> httpx.Response:
            seen["json"] = request.read()
            return httpx.Response(200, json={"id": str(uuid.uuid4())})

        client = _client(handler)
        book_id = uuid.uuid4()
        deployment_id = uuid.uuid4()
        workflow_id = uuid.uuid4()
        entity_id = uuid.uuid4()
        asyncio.run(
            client.deploy_workflow(
                book_id,
                workflow_deployment_id=deployment_id,
                workflow_id=workflow_id,
                entity_id=entity_id,
                workflow_name="Test",
                description=None,
                backend_api_calls=["post_entry"],
            )
        )
        body = seen["json"].decode()
        self.assertIn(str(deployment_id), body)
        self.assertIn(str(workflow_id), body)
        self.assertIn(str(entity_id), body)


if __name__ == "__main__":
    unittest.main()
