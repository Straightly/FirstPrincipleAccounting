// LedgerZero workflow: "Recording startup expense" (Impl Spec §7.5.1).
// Hand-written, no AI, no build step — plain React.createElement so this
// bundle needs nothing beyond the two vendored React/ReactDOM files sitting
// next to it. A standalone app: it shares no JavaScript with the launcher
// or any other workflow (Impl Spec §7.1).
//
// workflow_id / workflow_deployment_id identify *this deployed artifact*
// and are fixed at authoring time (the engine cannot generate them, since
// this file must embed them before it is ever deployed). book_id and
// entity_id are supplied by whoever launches the workflow (the launcher
// passes them as URL query parameters) — the same code could in principle
// run against a different book/entity without changing a byte, so they are
// not baked in here.
const WORKFLOW_ID = "5230b634-7ad9-46fa-a069-979e6c658eb3";
const WORKFLOW_DEPLOYMENT_ID = "2ef2f432-a548-4f24-87a2-8521bde76af8";

const e = React.createElement;

function useQueryParam(name) {
  return new URLSearchParams(window.location.search).get(name);
}

async function api(path, options) {
  const response = await fetch(path, {
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  const body = await response.json().catch(() => ({}));
  return { ok: response.ok, status: response.status, body };
}

function todayIso() {
  return new Date().toISOString().slice(0, 10);
}

function App() {
  const bookId = useQueryParam("book_id");
  const entityId = useQueryParam("entity_id");

  const [me, setMe] = React.useState(null);
  const [authorized, setAuthorized] = React.useState(null);
  const [form, setForm] = React.useState({
    entryDate: todayIso(),
    description: "",
    amount: "",
    sourceAccountId: "",
    expenseAccountId: "",
    memo: "",
  });
  const [result, setResult] = React.useState(null);
  const [error, setError] = React.useState(null);
  const [submitting, setSubmitting] = React.useState(false);

  React.useEffect(() => {
    if (!bookId || !entityId) return;
    api("/api/auth/me").then((r) => setMe(r.ok ? r.body : null));
    // Spec §7.5.1: "verify that the user is authorized to run the workflow
    // for the selected entity and book." This is a UX pre-check only — the
    // backend re-verifies unconditionally on the actual post_entry call.
    api(`/api/books/${bookId}/workflows/mine?entity_id=${entityId}`).then(
      (r) => {
        setAuthorized(
          r.ok && r.body.some((w) => w.workflow_id === WORKFLOW_ID)
        );
      }
    );
  }, [bookId, entityId]);

  function setField(name) {
    return (ev) => setForm({ ...form, [name]: ev.target.value });
  }

  async function submit(ev) {
    ev.preventDefault();
    setError(null);
    setSubmitting(true);
    const executionId = crypto.randomUUID();
    const response = await api(`/api/books/${bookId}/entries`, {
      method: "POST",
      body: JSON.stringify({
        entry_id: crypto.randomUUID(),
        entity_id: entityId,
        entry_date: form.entryDate,
        description: form.description,
        source: "WORKFLOW",
        workflow: {
          workflow_id: WORKFLOW_ID,
          workflow_deployment_id: WORKFLOW_DEPLOYMENT_ID,
          workflow_execution_id: executionId,
        },
        lines: [
          {
            line_id: crypto.randomUUID(),
            account_id: form.expenseAccountId,
            debit_amount: form.amount,
            credit_amount: null,
            memo: form.memo || null,
          },
          {
            line_id: crypto.randomUUID(),
            account_id: form.sourceAccountId,
            debit_amount: null,
            credit_amount: form.amount,
            memo: form.memo || null,
          },
        ],
      }),
    });
    setSubmitting(false);
    if (response.ok) {
      setResult({ entryId: response.body.id, executionId });
      setForm({ ...form, description: "", amount: "", memo: "" });
    } else {
      setError(response.body);
    }
  }

  if (!bookId || !entityId) {
    return e(
      "div",
      { className: "box" },
      e("h1", null, "Recording startup expense"),
      e(
        "p",
        { className: "error" },
        "Open this workflow from the LedgerZero launcher's workflow menu " +
          "(it needs book_id and entity_id, missing from this URL)."
      )
    );
  }

  return e(
    "div",
    { className: "box" },
    e("h1", null, "Recording startup expense"),
    me && e("p", { className: "muted" }, `Signed in as ${me.user.email}`),
    authorized === false &&
      e(
        "p",
        { className: "error" },
        "You are not currently authorized to run this workflow for this " +
          "entity — the server will reject any submission."
      ),
    e(
      "form",
      { onSubmit: submit },
      e(
        "label",
        null,
        "Expense date",
        e("input", {
          type: "date",
          required: true,
          value: form.entryDate,
          onChange: setField("entryDate"),
        })
      ),
      e(
        "label",
        null,
        "Description",
        e("input", {
          type: "text",
          required: true,
          placeholder: "e.g. Laptop for new hire",
          value: form.description,
          onChange: setField("description"),
        })
      ),
      e(
        "label",
        null,
        "Amount",
        e("input", {
          type: "number",
          step: "0.01",
          min: "0.01",
          required: true,
          value: form.amount,
          onChange: setField("amount"),
        })
      ),
      e(
        "label",
        null,
        "Expense or asset account (account_id)",
        e("input", {
          type: "text",
          required: true,
          placeholder: "account UUID",
          value: form.expenseAccountId,
          onChange: setField("expenseAccountId"),
        })
      ),
      e(
        "label",
        null,
        "Paid from account_id (source)",
        e("input", {
          type: "text",
          required: true,
          placeholder: "account UUID",
          value: form.sourceAccountId,
          onChange: setField("sourceAccountId"),
        })
      ),
      e(
        "label",
        null,
        "Source document / memo (optional)",
        e("input", {
          type: "text",
          value: form.memo,
          onChange: setField("memo"),
        })
      ),
      e(
        "button",
        { type: "submit", disabled: submitting },
        submitting ? "Recording…" : "Record expense"
      )
    ),
    result &&
      e(
        "p",
        { className: "success" },
        `Posted entry ${result.entryId} (execution ${result.executionId}).`
      ),
    error &&
      e(
        "p",
        { className: "error" },
        `${error.error_code || "ERROR"}: ${error.message || "request failed"}`
      )
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(e(App));
