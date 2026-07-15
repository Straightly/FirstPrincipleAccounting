"""MCP primitive tests (Impl Plan M8), against a fake client so no live
backend is needed. Covers the generate -> deploy composition and that admin
primitives translate string ids to UUIDs and forward to the right backend
call.
"""

import asyncio
import sys
import tempfile
import unittest
import uuid
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / "src"
sys.path.insert(0, str(SRC))

from first_principle_accounting.config import Config  # noqa: E402
from first_principle_accounting.mcp import tools  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
REAL_VENDOR_DIR = REPO_ROOT / "frontend" / "node_modules"


class FakeRuntimeBackendClient:
    """Records every call instead of making one, and returns canned/echoed
    results — enough for tools.py's primitives to exercise their own
    argument-marshalling logic without a live backend."""

    def __init__(self):
        self.calls: list[tuple[str, tuple, dict]] = []

    def _record(self, name, args, kwargs):
        self.calls.append((name, args, kwargs))

    async def deploy_workflow(self, book_id, **kwargs):
        self._record("deploy_workflow", (book_id,), kwargs)
        return {"id": str(uuid.uuid4())}

    async def create_accounting_book(self, name, passphrase):
        self._record("create_accounting_book", (name, passphrase), {})
        return {
            "book_id": str(uuid.uuid4()),
            "name": name,
            "entity_id": str(uuid.uuid4()),
        }

    async def create_chart(self, book_id, **kwargs):
        self._record("create_chart", (book_id,), kwargs)
        return {"id": str(uuid.uuid4())}

    async def create_role(self, book_id, **kwargs):
        self._record("create_role", (book_id,), kwargs)
        return {"id": str(uuid.uuid4())}

    async def assign_role_to_user(self, book_id, role_id, user_id):
        self._record("assign_role_to_user", (book_id, role_id, user_id), {})
        return {"id": str(uuid.uuid4())}


def _generation_kwargs(**overrides):
    base = dict(
        workflow_name="Test",
        description="d",
        spec_reference="s",
        fields=[
            {"id": "d", "label": "Date", "input_type": "date"},
            {"id": "desc", "label": "Description", "input_type": "text"},
            {"id": "amount", "label": "Amount", "input_type": "number"},
            {"id": "a1", "label": "A1", "input_type": "account"},
            {"id": "a2", "label": "A2", "input_type": "account"},
        ],
        date_field="d",
        description_field="desc",
        amount_field="amount",
        primary_account_field="a1",
        offset_account_field="a2",
        submit_label="Go",
    )
    base.update(overrides)
    return base


class TestGenerateAndDeploy(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.config = Config(
            dev_artifacts_dir=Path(self.tmp.name),
            react_vendor_dir=REAL_VENDOR_DIR,
        )
        self.client = FakeRuntimeBackendClient()
        self.session = tools.Session(client=self.client, config=self.config)

    def test_generate_then_deploy_round_trip(self):
        async def run():
            generated = await tools.generate_workflow_definition(
                **_generation_kwargs()
            )
            book_id = str(uuid.uuid4())
            entity_id = str(uuid.uuid4())
            result = await tools.deploy_workflow_definition(
                self.session,
                generated=generated,
                book_id=book_id,
                entity_id=entity_id,
            )
            return generated, result

        generated, result = asyncio.run(run())

        self.assertEqual(result["workflow_id"], generated["workflow_id"])
        self.assertTrue(result["workflow_deployment_id"])
        self.assertIn(result["workflow_deployment_id"], result["frontend_route"])

        [call] = self.client.calls
        name, args, kwargs = call
        self.assertEqual(name, "deploy_workflow")
        self.assertEqual(kwargs["workflow_id"], uuid.UUID(generated["workflow_id"]))
        self.assertEqual(
            str(kwargs["workflow_deployment_id"]), result["workflow_deployment_id"]
        )

        # The artifact actually landed on disk with the real deployment id.
        artifact_dir = (
            self.config.dev_artifacts_dir
            / "workflows"
            / result["workflow_deployment_id"]
        )
        self.assertTrue((artifact_dir / "code" / "app.js").is_file())

    def test_get_workflow_definition_reads_back_what_deploy_wrote(self):
        async def run():
            generated = await tools.generate_workflow_definition(
                **_generation_kwargs()
            )
            deployed = await tools.deploy_workflow_definition(
                self.session,
                generated=generated,
                book_id=str(uuid.uuid4()),
                entity_id=str(uuid.uuid4()),
            )
            fetched = await tools.get_workflow_definition(
                self.session,
                workflow_deployment_id=deployed["workflow_deployment_id"],
            )
            return deployed, fetched

        deployed, fetched = asyncio.run(run())
        self.assertEqual(
            fetched["workflow_deployment_id"], deployed["workflow_deployment_id"]
        )
        self.assertEqual(fetched["workflow"]["workflow_name"], "Test")
        self.assertIsNotNone(fetched["manifest"])

    def test_get_workflow_definition_missing_deployment_raises(self):
        async def run():
            await tools.get_workflow_definition(
                self.session, workflow_deployment_id=str(uuid.uuid4())
            )

        with self.assertRaises(FileNotFoundError):
            asyncio.run(run())


class TestAdminPrimitives(unittest.TestCase):
    def setUp(self):
        self.client = FakeRuntimeBackendClient()
        self.session = tools.Session(
            client=self.client, config=Config(dev_artifacts_dir=Path("/nonexistent"))
        )

    def test_create_accounting_book_forwards_args(self):
        result = asyncio.run(
            tools.create_accounting_book(
                self.session, name="Acme", passphrase="correct horse battery"
            )
        )
        self.assertEqual(result["name"], "Acme")
        [call] = self.client.calls
        self.assertEqual(call[0], "create_accounting_book")
        self.assertEqual(call[1], ("Acme", "correct horse battery"))

    def test_create_chart_converts_string_ids_to_uuid(self):
        book_id = str(uuid.uuid4())
        entity_id = str(uuid.uuid4())
        asyncio.run(
            tools.create_chart(
                self.session, book_id=book_id, entity_id=entity_id, name="Main"
            )
        )
        [call] = self.client.calls
        _, args, kwargs = call
        self.assertEqual(args, (uuid.UUID(book_id),))
        self.assertEqual(kwargs["entity_id"], uuid.UUID(entity_id))

    def test_assign_role_to_user_converts_all_three_ids(self):
        book_id, role_id, user_id = (str(uuid.uuid4()) for _ in range(3))
        asyncio.run(
            tools.assign_role_to_user(
                self.session, book_id=book_id, role_id=role_id, user_id=user_id
            )
        )
        [call] = self.client.calls
        _, args, _ = call
        self.assertEqual(
            args, (uuid.UUID(book_id), uuid.UUID(role_id), uuid.UUID(user_id))
        )


if __name__ == "__main__":
    unittest.main()
