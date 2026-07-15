"""Artifact preparation tests (Impl Plan M8, Project Plan #12): correct
on-disk layout, no overwriting an existing deployment, and no half-written
directory left behind after a failure (Project Plan #12's
"failed-generation cleanup" acceptance criterion).
"""

import json
import sys
import tempfile
import unittest
import uuid
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / "src"
sys.path.insert(0, str(SRC))

from first_principle_accounting.config import Config  # noqa: E402
from first_principle_accounting.devtime.artifacts import (  # noqa: E402
    ArtifactPreparationError,
    prepare_artifact,
    workflow_dir,
)
from first_principle_accounting.devtime.generator import (  # noqa: E402
    FormField,
    WorkflowGenerationRequest,
    generate_workflow_definition,
)


def _request() -> WorkflowGenerationRequest:
    return WorkflowGenerationRequest(
        workflow_name="Test workflow",
        description="d",
        spec_reference="s",
        fields=(
            FormField("d", "Date", "date"),
            FormField("desc", "Description", "text"),
            FormField("amount", "Amount", "number"),
            FormField("a1", "A1", "account"),
            FormField("a2", "A2", "account"),
        ),
        date_field="d",
        description_field="desc",
        amount_field="amount",
        primary_account_field="a1",
        offset_account_field="a2",
        submit_label="Go",
    )


REPO_ROOT = Path(__file__).resolve().parents[2]
REAL_VENDOR_DIR = REPO_ROOT / "frontend" / "node_modules"


class TestPrepareArtifact(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.config = Config(
            dev_artifacts_dir=Path(self.tmp.name),
            react_vendor_dir=REAL_VENDOR_DIR,
        )

    def test_writes_expected_layout(self):
        generated = generate_workflow_definition(_request())
        deployment_id = uuid.uuid4()
        result_dir = prepare_artifact(generated, deployment_id, self.config)

        self.assertEqual(result_dir, workflow_dir(self.config, deployment_id))
        expected = {
            "workflow.json",
            "manifest.json",
            "code/index.html",
            "code/app.js",
            "code/react.production.min.js",
            "code/react-dom.production.min.js",
            "signatures/.gitkeep",
        }
        actual = {
            str(p.relative_to(result_dir))
            for p in result_dir.rglob("*")
            if p.is_file()
        }
        self.assertEqual(actual, expected)

    def test_manifest_and_code_embed_the_real_deployment_id(self):
        generated = generate_workflow_definition(_request())
        deployment_id = uuid.uuid4()
        result_dir = prepare_artifact(generated, deployment_id, self.config)

        manifest = json.loads((result_dir / "manifest.json").read_text())
        self.assertEqual(manifest["workflow_deployment_id"], str(deployment_id))
        app_js = (result_dir / "code" / "app.js").read_text()
        self.assertIn(str(deployment_id), app_js)
        self.assertNotIn("{{WORKFLOW_DEPLOYMENT_ID}}", app_js)

    def test_refuses_to_overwrite_an_existing_deployment(self):
        generated = generate_workflow_definition(_request())
        deployment_id = uuid.uuid4()
        prepare_artifact(generated, deployment_id, self.config)
        with self.assertRaises(ArtifactPreparationError):
            prepare_artifact(generated, deployment_id, self.config)

    def test_missing_vendor_files_leave_no_partial_directory(self):
        generated = generate_workflow_definition(_request())
        deployment_id = uuid.uuid4()
        broken_config = Config(
            dev_artifacts_dir=Path(self.tmp.name),
            react_vendor_dir=Path(self.tmp.name) / "nonexistent",
        )
        with self.assertRaises(ArtifactPreparationError):
            prepare_artifact(generated, deployment_id, broken_config)
        self.assertFalse(workflow_dir(broken_config, deployment_id).exists())


if __name__ == "__main__":
    unittest.main()
