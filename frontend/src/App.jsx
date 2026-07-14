import { useEffect, useState } from "react";

// LedgerZero launcher (M1): login, session, identity & authority.
// M5 adds the workflow menu: every deployed workflow the signed-in user
// holds a role for, navigating out to each workflow's own standalone route.

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
  const [bookId, setBookId] = useState("");
  const [entityId, setEntityId] = useState("");
  const [myWorkflows, setMyWorkflows] = useState(null);
  const [workflowsError, setWorkflowsError] = useState("");

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

  // Workflow menu (Impl Spec §7.1, Impl Plan M5): every deployed workflow
  // the signed-in user currently holds a role for, in the given book/entity.
  // No book-browser UI yet (M4 added the book APIs but not this screen) —
  // the owner tells collaborators the book_id/entity_id to use, matching
  // this milestone's "intentionally simple" frontend scope (Impl Spec §8.3).
  async function loadMyWorkflows(e) {
    e.preventDefault();
    setWorkflowsError("");
    setMyWorkflows(null);
    const r = await api(
      `/api/books/${bookId}/workflows/mine?entity_id=${entityId}`
    );
    if (r.ok) {
      setMyWorkflows(r.body);
    } else {
      setWorkflowsError(`${r.body.error_code}: ${r.body.message}`);
    }
  }

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
      <form onSubmit={loadMyWorkflows}>
        <input
          placeholder="book_id"
          value={bookId}
          onChange={(e) => setBookId(e.target.value)}
          style={{ padding: 8, width: "45%", marginRight: 4 }}
        />
        <input
          placeholder="entity_id"
          value={entityId}
          onChange={(e) => setEntityId(e.target.value)}
          style={{ padding: 8, width: "45%" }}
        />
        <button style={button} type="submit">
          Show my workflows
        </button>
      </form>
      {workflowsError && <p style={{ color: "#a00" }}>{workflowsError}</p>}
      {myWorkflows && myWorkflows.length === 0 && (
        <p style={{ color: "#666" }}>
          No workflows in this entity are assigned to you.
        </p>
      )}
      {myWorkflows && myWorkflows.length > 0 && (
        <ul>
          {myWorkflows.map((w) => (
            <li key={w.workflow_deployment_id}>
              <a
                href={`${w.frontend_route}?book_id=${bookId}&entity_id=${entityId}`}
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
