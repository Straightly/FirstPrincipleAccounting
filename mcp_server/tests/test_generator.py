"""Template generation tests (Impl Plan M8): the deterministic "LLM
wrapping" implementation must produce valid, correctly-parameterized
artifacts for both the fixed-direction shape (M5's "Recording startup
expense") and the direction-toggle shape (M8's "Recording bank account
transactions manually") without a live model.
"""

import json
import sys
import unittest
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / "src"
sys.path.insert(0, str(SRC))

from first_principle_accounting.devtime.generator import (  # noqa: E402
    FormField,
    WorkflowGenerationRequest,
    generate_workflow_definition,
)


def _fixed_direction_request() -> WorkflowGenerationRequest:
    return WorkflowGenerationRequest(
        workflow_name="Recording startup expense",
        description="Records a startup expense.",
        spec_reference="Impl Spec §7.5.1",
        fields=(
            FormField("entryDate", "Expense date", "date"),
            FormField("description", "Description", "text"),
            FormField("amount", "Amount", "number"),
            FormField("expenseAccountId", "Expense account", "account"),
            FormField("sourceAccountId", "Source account", "account"),
            FormField("memo", "Memo", "text", required=False),
        ),
        date_field="entryDate",
        description_field="description",
        amount_field="amount",
        primary_account_field="expenseAccountId",
        offset_account_field="sourceAccountId",
        memo_field="memo",
        submit_label="Record expense",
    )


def _direction_toggle_request() -> WorkflowGenerationRequest:
    return WorkflowGenerationRequest(
        workflow_name="Recording bank account transactions manually",
        description="Records bank activity.",
        spec_reference="Impl Spec §7.5.2",
        fields=(
            FormField("transactionDate", "Transaction date", "date"),
            FormField("description", "Description", "text"),
            FormField("amount", "Amount", "number"),
            FormField(
                "direction",
                "Direction",
                "select",
                options=(
                    ("deposit", "Deposit (money in)"),
                    ("withdrawal", "Withdrawal (money out)"),
                ),
            ),
            FormField("bankAccountId", "Bank account", "account"),
            FormField("offsetAccountId", "Offset account", "account"),
            FormField("reference", "Reference", "text", required=False),
        ),
        date_field="transactionDate",
        description_field="description",
        amount_field="amount",
        primary_account_field="bankAccountId",
        offset_account_field="offsetAccountId",
        memo_field="reference",
        direction_field="direction",
        direction_primary_debit_value="deposit",
        submit_label="Record transaction",
    )


class TestFormField(unittest.TestCase):
    def test_select_requires_options(self):
        with self.assertRaises(ValueError):
            FormField("direction", "Direction", "select")


class TestWorkflowGenerationRequest(unittest.TestCase):
    def test_rejects_unknown_field_reference(self):
        with self.assertRaises(ValueError):
            WorkflowGenerationRequest(
                workflow_name="x",
                description="y",
                spec_reference="z",
                fields=(FormField("a", "A", "text"),),
                date_field="not_declared",
                description_field="a",
                amount_field="a",
                primary_account_field="a",
                offset_account_field="a",
                submit_label="Go",
            )

    def test_direction_field_requires_debit_value(self):
        with self.assertRaises(ValueError):
            WorkflowGenerationRequest(
                workflow_name="x",
                description="y",
                spec_reference="z",
                fields=(
                    FormField("d", "D", "date"),
                    FormField("desc", "Desc", "text"),
                    FormField("amount", "Amount", "number"),
                    FormField("a1", "A1", "account"),
                    FormField("a2", "A2", "account"),
                    FormField(
                        "dir", "Dir", "select", options=(("a", "A"), ("b", "B"))
                    ),
                ),
                date_field="d",
                description_field="desc",
                amount_field="amount",
                primary_account_field="a1",
                offset_account_field="a2",
                submit_label="Go",
                direction_field="dir",
            )


class TestGenerateWorkflowDefinition(unittest.TestCase):
    def test_fixed_direction_shape(self):
        generated = generate_workflow_definition(_fixed_direction_request())
        self.assertEqual(generated.workflow_name, "Recording startup expense")
        self.assertEqual(generated.backend_api_calls, ["post_entry"])
        self.assertEqual(
            generated.required_inputs,
            {
                "entryDate": "date",
                "description": "text",
                "amount": "number",
                "expenseAccountId": "account",
                "sourceAccountId": "account",
                "memo": "text",
            },
        )
        # Optional field marked in the human-readable collects list.
        self.assertIn("Memo (optional)", generated.workflow_json["steps"][0]["collects"])
        self.assertNotIn("primaryIsDebit", generated.app_js)
        self.assertIn(str(generated.workflow_id), generated.app_js)
        self.assertIn("{{WORKFLOW_DEPLOYMENT_ID}}", generated.app_js)
        self.assertIn("{{WORKFLOW_DEPLOYMENT_ID}}", generated.manifest_json["workflow_deployment_id"])

    def test_direction_toggle_shape(self):
        generated = generate_workflow_definition(_direction_toggle_request())
        self.assertIn("primaryIsDebit", generated.app_js)
        self.assertIn('form.direction === "deposit"', generated.app_js)
        # Both accounts appear on both sides of the ternary (debit/credit
        # genuinely swap, neither account is hard-wired to one side).
        self.assertIn("account_id: form.bankAccountId", generated.app_js)
        self.assertIn("account_id: form.offsetAccountId", generated.app_js)

    def test_generated_json_documents_are_valid_json(self):
        generated = generate_workflow_definition(_direction_toggle_request())
        # workflow_json/manifest_json are already dicts; round-trip through
        # json to catch any non-serializable value (e.g. a stray FormField).
        json.dumps(generated.workflow_json)
        json.dumps(generated.manifest_json)

    def test_each_call_mints_a_fresh_workflow_id(self):
        a = generate_workflow_definition(_direction_toggle_request())
        b = generate_workflow_definition(_direction_toggle_request())
        self.assertNotEqual(a.workflow_id, b.workflow_id)

    def test_post_submit_reset_clears_free_text_fields_only(self):
        generated = generate_workflow_definition(_direction_toggle_request())
        # Description/amount/memo(reference) clear after submit; accounts
        # and date persist (matches the hand-written M5 precedent).
        self.assertIn('description: "", amount: "", reference: ""', generated.app_js)
        reset_block = generated.app_js.split("setForm({ ...form,", 1)[1].split("})", 1)[0]
        self.assertNotIn("bankAccountId", reset_block)
        self.assertNotIn("offsetAccountId", reset_block)
        self.assertNotIn("transactionDate", reset_block)


if __name__ == "__main__":
    unittest.main()
