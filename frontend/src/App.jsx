import { useEffect, useState } from "react";

// LedgerZero launcher (M1): login, session, identity & authority.
// The workflow menu appears here from M5 on.

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

  if (authConfig === null) {
    return <div style={box}>Loading…</div>;
  }

  if (!me) {
    return (
      <div style={box}>
        <h1>LedgerZero</h1>
        <p>Sign in to continue.</p>
        {authConfig.google_configured && (
          <button style={button} onClick={() => (window.location.href = "/api/auth/google/login")}>
            Sign in with Google
          </button>
        )}
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
        {!authConfig.google_configured && !authConfig.dev_login_enabled && (
          <p style={{ color: "#a00" }}>
            No login method configured. Set [oauth.google] in server.config.toml.
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
      <p style={{ color: "#666" }}>Workflow menu arrives in milestone M5.</p>
    </div>
  );
}
