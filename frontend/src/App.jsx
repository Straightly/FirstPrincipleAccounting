import { useEffect, useState } from "react";

// LedgerZero launcher (M1): login, session, identity & authority.
// M5 adds the workflow menu: every deployed workflow the signed-in user
// holds a role for, navigating out to each workflow's own standalone route.
// M6 replaces the M5 book_id/entity_id text inputs with a real bootstrapped
// picker, so a non-owner user with a role assignment never needs to already
// know a raw id. M7 constrains every book to exactly one entity (a book's
// key/owner is now also its accounting boundary), so the picker is a single
// book-selection step — the book's entity_id comes along for free.

const box = {
  maxWidth: 480,
  margin: "10vh auto",
  padding: 24,
  fontFamily: "system-ui, sans-serif",
  border: "1px solid #ddd",
  borderRadius: 8,
};
const button = {
  padding: "8px 16px",
  marginRight: 8,
  marginTop: 8,
  cursor: "pointer",
};

async function api(path, options) {
  const response = await fetch(path, options);
  const body = await response.json().catch(() => ({}));
  return { ok: response.ok, status: response.status, body };
}

export default function App() {
  const [authConfig, setAuthConfig] = useState(null);
  const [me, setMe] = useState(null);
  const [devEmail, setDevEmail] = useState("");
  const [message, setMessage] = useState("");
  const [books, setBooks] = useState(null);
  const [selectedBookId, setSelectedBookId] = useState("");
  const [myWorkflows, setMyWorkflows] = useState(null);
  const [pickerError, setPickerError] = useState("");

  async function loadMe() {
    const r = await api("/api/auth/me");
    setMe(r.ok ? r.body : null);
  }

  useEffect(() => {
    api("/api/auth/config").then((r) => setAuthConfig(r.ok ? r.body : {}));
    loadMe();
    if (new URLSearchParams(window.location.search).get("login_error")) {
      setMessage("Google login was cancelled or denied.");
    }
  }, []);

  async function devLogin(e) {
    e.preventDefault();
    const r = await api("/api/auth/dev-login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ email: devEmail }),
    });
    if (r.ok) {
      setMe(r.body);
      setMessage("");
    } else {
      setMessage(`${r.body.error_code}: ${r.body.message}`);
    }
  }

  async function adminPing() {
    const r = await api("/api/admin/ping");
    setMessage(
      r.ok
        ? `admin ping: ${r.body.message} (owner ${r.body.owner})`
        : `${r.body.error_code}: ${r.body.message}`
    );
  }

  async function refresh() {
    const r = await api("/api/auth/refresh", { method: "POST" });
    setMessage(r.ok ? "Session token rotated." : `${r.body.error_code}: ${r.body.message}`);
  }

  async function logout() {
    await api("/api/auth/logout", { method: "POST" });
    setMe(null);
    setMessage("");
  }

  // Book picker (Impl Spec §6.5/§7.1, Impl Plan M6/M7): a bootstrapped
  // launcher capability, the same kind as "Open book"/"Adding a workflow" —
  // not a deployed workflow artifact. The owner sees every book; any other
  // signed-in user sees only ones where they hold a workflow-granting role,
  // discovered purely from server-side role assignments. Each book already
  // carries its one entity_id (M7: a book has exactly one entity), so
  // selecting a book is the only step needed before the workflow menu.
  useEffect(() => {
    if (!me) return;
    setPickerError("");
    api("/api/books/mine").then((r) => {
      if (r.ok) setBooks(r.body);
      else setPickerError(`${r.body.error_code}: ${r.body.message}`);
    });
  }, [me]);

  const selectedBook = (books || []).find((b) => b.book_id === selectedBookId);

  // Impl Plan M9: a restored book's ledger can outlive its dev artifacts —
  // don't offer a link that can only 404 or serve stale code.
  const availableWorkflows = (myWorkflows || []).filter(
    (w) => w.artifact_available !== false
  );

  useEffect(() => {
    setMyWorkflows(null);
    if (!selectedBookId || !books) return;
    const book = books.find((b) => b.book_id === selectedBookId);
    if (!book) return;
    setPickerError("");
    api(
      `/api/books/${book.book_id}/workflows/mine?entity_id=${book.entity_id}`
    ).then((r) => {
      if (r.ok) setMyWorkflows(r.body);
      else setPickerError(`${r.body.error_code}: ${r.body.message}`);
    });
  }, [selectedBookId, books]);

  if (authConfig === null) {
    return <div style={box}>Loading…</div>;
  }

  if (!me) {
    return (
      <div style={box}>
        <h1>LedgerZero</h1>
        <p>Sign in to continue.</p>
        {(authConfig.providers || []).map((p) => (
          <button
            key={p.id}
            style={button}
            onClick={() => (window.location.href = `/api/auth/${p.id}/login`)}
          >
            Sign in with {p.display_name}
          </button>
        ))}
        {authConfig.dev_login_enabled && (
          <form onSubmit={devLogin}>
            <p style={{ color: "#a00" }}>Dev login (local development only):</p>
            <input
              type="email"
              placeholder="email"
              value={devEmail}
              onChange={(e) => setDevEmail(e.target.value)}
              style={{ padding: 8, width: "60%" }}
            />
            <button style={button} type="submit">
              Dev sign in
            </button>
          </form>
        )}
        {(authConfig.providers || []).length === 0 && !authConfig.dev_login_enabled && (
          <p style={{ color: "#a00" }}>
            No login method configured. Add an [[auth_providers]] block in
            server.config.toml.
          </p>
        )}
        {message && <p>{message}</p>}
      </div>
    );
  }

  return (
    <div style={box}>
      <h1>LedgerZero</h1>
      <p>
        Signed in as <strong>{me.user.display_name}</strong> ({me.user.email})
      </p>
      <p style={{ fontSize: 12, color: "#666" }}>user_id: {me.user.user_id}</p>
      <p>
        Bootstrap owner: <strong>{me.is_bootstrap_owner ? "yes" : "no"}</strong>
      </p>
      <p>
        Allowed actions:{" "}
        {me.allowed_actions.length > 0 ? me.allowed_actions.join(", ") : "none"}
      </p>
      <hr />
      <button style={button} onClick={adminPing}>
        Test owner-gated endpoint
      </button>
      <button style={button} onClick={refresh}>
        Rotate session
      </button>
      <button style={button} onClick={logout}>
        Sign out
      </button>
      {message && <p>{message}</p>}
      <hr />
      <h2 style={{ fontSize: 16 }}>My workflows</h2>
      {books === null && <p style={{ color: "#666" }}>Loading books…</p>}
      {books && books.length === 0 && (
        <p style={{ color: "#666" }}>No books available to you yet.</p>
      )}
      {books && books.length > 0 && (
        <label style={{ display: "block" }}>
          Book
          <select
            value={selectedBookId}
            onChange={(e) => setSelectedBookId(e.target.value)}
            style={{ display: "block", padding: 8, width: "100%", marginTop: 4 }}
          >
            <option value="">Choose a book…</option>
            {books.map((b) => (
              <option key={b.book_id} value={b.book_id}>
                {b.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {pickerError && <p style={{ color: "#a00" }}>{pickerError}</p>}
      {myWorkflows && availableWorkflows.length === 0 && (
        <p style={{ color: "#666" }}>
          No workflows in this book are assigned to you.
        </p>
      )}
      {myWorkflows && availableWorkflows.length > 0 && selectedBook && (
        <ul>
          {availableWorkflows.map((w) => (
            <li key={w.workflow_deployment_id}>
              <a
                href={`${w.frontend_route}?book_id=${selectedBook.book_id}&entity_id=${selectedBook.entity_id}`}
              >
                {w.workflow_name}
              </a>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
