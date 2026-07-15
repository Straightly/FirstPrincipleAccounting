"""Workflow generation: the "LLM wrapping" seam (Impl Spec §6.2, §7.1).

v1 generates deterministically from a structured request instead of calling
a live model — the same starting point the original Project Plan called for
("starting with deterministic template generation before enabling real LLM
calls"). The public entry point, `generate_workflow_definition`, is the
whole seam: a real model call could replace its body later (turning a
natural-language prompt into the same `WorkflowGenerationRequest` this
module already consumes) without changing anything downstream — artifact
preparation and deployment neither know nor care how the definition was
produced.

Only synthetic/structural inputs reach this module (field names, labels,
account-selection roles) — never live accounting data, so there is nothing
private to redact or persist (Axiom 12).
"""

from __future__ import annotations

import dataclasses
import json
import uuid

_DEPLOYMENT_ID_PLACEHOLDER = "{{WORKFLOW_DEPLOYMENT_ID}}"


@dataclasses.dataclass(frozen=True)
class FormField:
    """One collected input, rendered as a labeled form control.

    `input_type` is one of "date", "text", "number", "account", or
    "select" (`options` required for "select" — a list of (value, label)
    pairs).
    """

    id: str
    label: str
    input_type: str
    required: bool = True
    placeholder: str | None = None
    options: tuple[tuple[str, str], ...] | None = None

    def __post_init__(self) -> None:
        if self.input_type == "select" and not self.options:
            raise ValueError(f"field {self.id!r}: select fields need options")


@dataclasses.dataclass(frozen=True)
class WorkflowGenerationRequest:
    """What a workflow author (human or, later, an LLM) supplies.

    Describes a balanced two-line entry: `primary_account_field` and
    `offset_account_field` name which fields hold the two account ids.
    Without `direction_field` the primary side is always debited (the
    "Recording startup expense" shape, Impl Spec §7.5.1). With
    `direction_field` set, its value flips which side is debited — the
    "Recording bank account transactions manually" shape (§7.5.2), where a
    deposit debits the bank account and a withdrawal credits it.
    """

    workflow_name: str
    description: str
    spec_reference: str
    fields: tuple[FormField, ...]
    date_field: str
    description_field: str
    amount_field: str
    primary_account_field: str
    offset_account_field: str
    submit_label: str
    memo_field: str | None = None
    direction_field: str | None = None
    direction_primary_debit_value: str | None = None
    backend_api_calls: tuple[str, ...] = ("post_entry",)

    def __post_init__(self) -> None:
        field_ids = {f.id for f in self.fields}
        for required_id in (
            self.date_field,
            self.description_field,
            self.amount_field,
            self.primary_account_field,
            self.offset_account_field,
        ):
            if required_id not in field_ids:
                raise ValueError(f"field {required_id!r} not declared in fields")
        if self.direction_field is not None:
            if self.direction_field not in field_ids:
                raise ValueError(
                    f"direction_field {self.direction_field!r} not declared in fields"
                )
            if self.direction_primary_debit_value is None:
                raise ValueError(
                    "direction_primary_debit_value is required when direction_field is set"
                )


@dataclasses.dataclass(frozen=True)
class GeneratedWorkflow:
    """A workflow definition ready for `prepare_artifact` (devtime/artifacts.py).

    `app_js`/`workflow_json`/`manifest_json` still carry the
    `{{WORKFLOW_DEPLOYMENT_ID}}` placeholder — the deployment id is minted
    at deploy time, not generation time, so the same generated definition
    can be redeployed with a fresh artifact identity (Impl Spec §2.9).
    """

    workflow_id: uuid.UUID
    workflow_name: str
    description: str
    backend_api_calls: list[str]
    required_inputs: dict[str, str]
    workflow_json: dict
    manifest_json: dict
    index_html: str
    app_js: str


def generate_workflow_definition(
    request: WorkflowGenerationRequest,
) -> GeneratedWorkflow:
    workflow_id = uuid.uuid4()
    required_inputs = {f.id: f.input_type for f in request.fields}
    collects = [
        f.label + (" (optional)" if not f.required else "")
        for f in request.fields
    ]
    workflow_json = {
        "workflow_name": request.workflow_name,
        "description": request.description,
        "steps": [
            {"kind": "form", "collects": collects},
            {"kind": "api_call", "backend_api": "post_entry"},
        ],
        "backend_api_calls": list(request.backend_api_calls),
        "required_inputs": required_inputs,
    }
    manifest_json = {
        "workflow_deployment_id": _DEPLOYMENT_ID_PLACEHOLDER,
        "workflow_id": str(workflow_id),
        "generator": "template:v1",
        "generated_by": "first_principle_accounting.devtime (Impl Plan M8)",
        "code_files": [
            "index.html",
            "app.js",
            "react.production.min.js",
            "react-dom.production.min.js",
        ],
        "notes": (
            "manifest_hash/code_hash are computed fresh from these files at "
            "deploy time (Impl Spec §7.4) — this manifest does not declare "
            "its own hash."
        ),
    }
    index_html = _render_index_html(request.workflow_name)
    app_js = _render_app_js(request, workflow_id)
    return GeneratedWorkflow(
        workflow_id=workflow_id,
        workflow_name=request.workflow_name,
        description=request.description,
        backend_api_calls=list(request.backend_api_calls),
        required_inputs=required_inputs,
        workflow_json=workflow_json,
        manifest_json=manifest_json,
        index_html=index_html,
        app_js=app_js,
    )


def _render_index_html(workflow_name: str) -> str:
    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>{workflow_name} — LedgerZero</title>
    <style>
      body {{
        font-family: system-ui, sans-serif;
        background: #f7f7f7;
        margin: 0;
      }}
      .box {{
        max-width: 480px;
        margin: 8vh auto;
        padding: 24px;
        background: #fff;
        border: 1px solid #ddd;
        border-radius: 8px;
      }}
      form label {{
        display: block;
        margin-top: 12px;
        font-size: 14px;
        color: #333;
      }}
      form input, form select {{
        display: block;
        width: 100%;
        box-sizing: border-box;
        padding: 8px;
        margin-top: 4px;
        font-size: 14px;
      }}
      button {{
        margin-top: 16px;
        padding: 8px 16px;
        cursor: pointer;
      }}
      .muted {{
        color: #666;
        font-size: 13px;
      }}
      .error {{
        color: #a00;
      }}
      .success {{
        color: #060;
      }}
    </style>
  </head>
  <body>
    <div id="root"></div>
    <script src="react.production.min.js"></script>
    <script src="react-dom.production.min.js"></script>
    <script src="app.js"></script>
  </body>
</html>
"""


def _js_field_defs(fields: tuple[FormField, ...]) -> str:
    defs = []
    for f in fields:
        entry: dict[str, object] = {
            "id": f.id,
            "label": f.label,
            "type": f.input_type,
            "required": f.required,
        }
        if f.placeholder:
            entry["placeholder"] = f.placeholder
        if f.options:
            entry["options"] = [list(pair) for pair in f.options]
        defs.append(entry)
    return json.dumps(defs, indent=2)


def _js_initial_form_state(fields: tuple[FormField, ...], date_field: str) -> str:
    lines = ["{"]
    for f in fields:
        value = "todayIso()" if f.id == date_field else '""'
        lines.append(f"    {f.id}: {value},")
    lines.append("  }")
    return "\n".join(lines)


def _js_build_lines(request: WorkflowGenerationRequest) -> str:
    primary = request.primary_account_field
    offset = request.offset_account_field
    amount = request.amount_field
    memo_expr = f"form.{request.memo_field} || null" if request.memo_field else "null"

    if request.direction_field is None:
        return f"""function buildLines(form) {{
  return [
    {{
      line_id: crypto.randomUUID(),
      account_id: form.{primary},
      debit_amount: form.{amount},
      credit_amount: null,
      memo: {memo_expr},
    }},
    {{
      line_id: crypto.randomUUID(),
      account_id: form.{offset},
      debit_amount: null,
      credit_amount: form.{amount},
      memo: {memo_expr},
    }},
  ];
}}"""

    direction = request.direction_field
    debit_value = json.dumps(request.direction_primary_debit_value)
    return f"""function buildLines(form) {{
  const primaryIsDebit = form.{direction} === {debit_value};
  return [
    {{
      line_id: crypto.randomUUID(),
      account_id: form.{primary},
      debit_amount: primaryIsDebit ? form.{amount} : null,
      credit_amount: primaryIsDebit ? null : form.{amount},
      memo: {memo_expr},
    }},
    {{
      line_id: crypto.randomUUID(),
      account_id: form.{offset},
      debit_amount: primaryIsDebit ? null : form.{amount},
      credit_amount: primaryIsDebit ? form.{amount} : null,
      memo: {memo_expr},
    }},
  ];
}}"""


def _render_app_js(
    request: WorkflowGenerationRequest, workflow_id: uuid.UUID
) -> str:
    field_defs = _js_field_defs(request.fields)
    initial_state = _js_initial_form_state(request.fields, request.date_field)
    build_lines = _js_build_lines(request)
    # Matches the hand-written precedent (M5): clear only the free-text
    # fields after a successful submit — date and account selections stay,
    # since consecutive entries commonly reuse them.
    reset_field_ids = [
        fid
        for fid in (request.description_field, request.amount_field, request.memo_field)
        if fid
    ]
    reset_after_submit = ", ".join(f'{fid}: ""' for fid in reset_field_ids)

    header = f"""// LedgerZero workflow: "{request.workflow_name}" ({request.spec_reference}).
// Generated by the dev-time backend from a deterministic template
// (Impl Plan M8, devtime/generator.py) — no hand-edits. Same standalone-
// artifact shape as every hand-written workflow (Impl Spec §7.1): its own
// vendored React copy, no shared JavaScript with the launcher or any other
// workflow. book_id/entity_id come from the URL query string (the launcher
// supplies them); workflow_id/workflow_deployment_id identify this
// deployed artifact and are fixed at generation/deploy time.
const WORKFLOW_ID = "{workflow_id}";
const WORKFLOW_DEPLOYMENT_ID = "{_DEPLOYMENT_ID_PLACEHOLDER}";

const e = React.createElement;

function useQueryParam(name) {{
  return new URLSearchParams(window.location.search).get(name);
}}

async function api(path, options) {{
  const response = await fetch(path, {{
    credentials: "same-origin",
    headers: {{ "Content-Type": "application/json" }},
    ...options,
  }});
  const body = await response.json().catch(() => ({{}}));
  return {{ ok: response.ok, status: response.status, body }};
}}

function todayIso() {{
  return new Date().toISOString().slice(0, 10);
}}

const FIELDS = {field_defs};

{build_lines}

function renderField(f, form, setField) {{
  const commonProps = {{
    required: f.required,
    value: form[f.id],
    onChange: setField(f.id),
  }};
  const input =
    f.type === "select"
      ? e(
          "select",
          commonProps,
          e("option", {{ value: "" }}, "Choose…"),
          ...f.options.map(([value, label]) =>
            e("option", {{ key: value, value }}, label)
          )
        )
      : e("input", {{
          type: f.type === "account" ? "text" : f.type,
          placeholder: f.placeholder || undefined,
          step: f.type === "number" ? "0.01" : undefined,
          min: f.type === "number" ? "0.01" : undefined,
          ...commonProps,
        }});
  return e("label", {{ key: f.id }}, f.label, input);
}}

function App() {{
  const bookId = useQueryParam("book_id");
  const entityId = useQueryParam("entity_id");

  const [me, setMe] = React.useState(null);
  const [authorized, setAuthorized] = React.useState(null);
  const [form, setForm] = React.useState({initial_state});
  const [result, setResult] = React.useState(null);
  const [error, setError] = React.useState(null);
  const [submitting, setSubmitting] = React.useState(false);

  React.useEffect(() => {{
    if (!bookId || !entityId) return;
    api("/api/auth/me").then((r) => setMe(r.ok ? r.body : null));
    api(`/api/books/${{bookId}}/workflows/mine?entity_id=${{entityId}}`).then(
      (r) => {{
        setAuthorized(
          r.ok && r.body.some((w) => w.workflow_id === WORKFLOW_ID)
        );
      }}
    );
  }}, [bookId, entityId]);

  function setField(name) {{
    return (ev) => setForm({{ ...form, [name]: ev.target.value }});
  }}

  async function submit(ev) {{
    ev.preventDefault();
    setError(null);
    setSubmitting(true);
    const executionId = crypto.randomUUID();
    const response = await api(`/api/books/${{bookId}}/entries`, {{
      method: "POST",
      body: JSON.stringify({{
        entry_id: crypto.randomUUID(),
        entity_id: entityId,
        entry_date: form.{request.date_field},
        description: form.{request.description_field},
        source: "WORKFLOW",
        workflow: {{
          workflow_id: WORKFLOW_ID,
          workflow_deployment_id: WORKFLOW_DEPLOYMENT_ID,
          workflow_execution_id: executionId,
        }},
        lines: buildLines(form),
      }}),
    }});
    setSubmitting(false);
    if (response.ok) {{
      setResult({{ entryId: response.body.id, executionId }});
      setForm({{ ...form, {reset_after_submit} }});
    }} else {{
      setError(response.body);
    }}
  }}

  if (!bookId || !entityId) {{
    return e(
      "div",
      {{ className: "box" }},
      e("h1", null, "{request.workflow_name}"),
      e(
        "p",
        {{ className: "error" }},
        "Open this workflow from the LedgerZero launcher's workflow menu " +
          "(it needs book_id and entity_id, missing from this URL)."
      )
    );
  }}

  return e(
    "div",
    {{ className: "box" }},
    e("h1", null, "{request.workflow_name}"),
    me && e("p", {{ className: "muted" }}, `Signed in as ${{me.user.email}}`),
    authorized === false &&
      e(
        "p",
        {{ className: "error" }},
        "You are not currently authorized to run this workflow for this " +
          "entity — the server will reject any submission."
      ),
    e(
      "form",
      {{ onSubmit: submit }},
      ...FIELDS.map((f) => renderField(f, form, setField)),
      e(
        "button",
        {{ type: "submit", disabled: submitting }},
        submitting ? "Recording…" : "{request.submit_label}"
      )
    ),
    result &&
      e(
        "p",
        {{ className: "success" }},
        `Posted entry ${{result.entryId}} (execution ${{result.executionId}}).`
      ),
    error &&
      e(
        "p",
        {{ className: "error" }},
        `${{error.error_code || "ERROR"}}: ${{error.message || "request failed"}}`
      )
  );
}}

ReactDOM.createRoot(document.getElementById("root")).render(e(App));
"""
    return header
