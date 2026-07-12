# LedgerZero — Run, Monitor, and Deploy (as of M1)

What exists after M0+M1: a routing server (Axum) serving the launcher and the
auth/session API. No accounting features yet — those start at M2. This
document covers what you can run and see today.

## 1. Run locally

Prerequisites: Rust, Node.js 20+, Python 3.11+.

```bash
./scripts/check.sh                      # build + test everything (the M0/M1 exit gate)
(cd frontend && npm install && npm run build)
cargo run -p ledgerzero-backend         # http://localhost:8080
```

`server.config.toml` already has `bootstrap_owner_email` and
`[dev_login] enabled = true`, so no OAuth credentials are needed for a first
run. The server prints a warning when dev login is on.

## 2. What you can see in a browser

Open http://localhost:8080:

- Login page (launcher). With dev login enabled, log in by email; with OAuth
  configured, one button per `[[auth_providers]]` domain.
- After login as `zhian.job@gmail.com`: the identity/authority page ("who am
  I / what may I do"), owner status, session rotate and logout buttons.
- Log in as any other email: authenticated but not owner —
  `/api/admin/ping` returns 403.

## 3. Monitoring while it runs

| Signal | Where | What it tells you |
|---|---|---|
| `GET /api/health` | `curl localhost:8080/api/health` | `{"status":"ok","engine_version":...}` — liveness probe |
| stdout | terminal / service log | startup line, dev-login warning, bind errors |
| `ops_audit.jsonl` | file next to the binary (path in config) | every auth/authz denial, append-only JSON lines |

Example uptime check: `curl -fsS localhost:8080/api/health || alert`.

Not yet present (planned for the hardening milestone): request logging,
metrics, tracing. In M1 there is no per-request visibility.

## 4. Enable real Google login

1. Google Cloud Console → Credentials → OAuth 2.0 Client ID (Web application).
2. Authorized redirect URI: `http://localhost:8080/api/auth/google/callback`
   (or the https equivalent on your public hostname).
3. Fill `client_id` / `client_secret` in `server.config.toml`, restart.

## 5. Package a deployable artifact

Production machines never see the git repo or a toolchain. Build the artifact
on your development machine:

```bash
./scripts/package.sh    # → dist/ledgerzero-<version>-<stamp>.tar.gz
```

The tarball contains the release binary, the built frontend, the example
config, a README.txt, and this document as DEPLOY.md — everything the target
machine needs.

### Local deployment rehearsal

Prove the artifact is self-contained by "deploying" it to a fresh directory
on your own machine, outside the repo:

```bash
tar -xzf dist/ledgerzero-*.tar.gz -C /tmp
cd /tmp/ledgerzero-*/
cp server.config.example.toml server.config.toml
# edit server.config.toml:
#   listen_addr = "127.0.0.1:8081"        # avoid clashing with a dev instance
#   [dev_login] enabled = true            # fine locally; the rehearsal is about packaging
./ledgerzero-backend server.config.toml
```

Verify from another terminal:

```bash
curl localhost:8081/api/health            # {"status":"ok",...}
open http://localhost:8081                # launcher loads → assets packaged correctly
# log in, check /api/admin/ping gating, then:
ls books/ ops_audit.jsonl                 # created next to the binary, not in the repo
```

If all of that works from /tmp with no reference back to the repository, the
same tarball + config edit is exactly what you ship to a real server.

## 6. Deploy to a remote server

Copy the tarball, unpack, configure, run:

```bash
scp dist/ledgerzero-*.tar.gz you@server:/opt/
ssh you@server 'cd /opt && tar -xzf ledgerzero-*.tar.gz'
# on the server: cp server.config.example.toml server.config.toml, edit, then
./ledgerzero-backend server.config.toml
```

Note: the binary is built for your development machine's platform. A macOS
build will not run on a Linux server — build on a matching platform (or
cross-compile / build in CI) for real remote deployments.

Required config changes on a remote machine:

- `[dev_login] enabled = false` — **never** on anything network-reachable.
- `listen_addr = "127.0.0.1:8080"` and put nginx/Caddy in front for TLS.
  Google OAuth requires HTTPS redirect URIs on non-localhost hosts.
- `redirect_url` in each `[[auth_providers]]` block must match the public
  hostname, and the same URI must be authorized in the provider's console.
- `frontend_dist`, `books_dir`, `ops_audit_log`: absolute paths.

Run it under a supervisor (systemd unit, `Restart=always`) and probe
`/api/health`.

### M1 limitations to know about

- Users and sessions are **in-memory** (persistent storage arrives in M3):
  a restart logs everyone out; run a single instance only.
- Nothing accounting-related is reachable yet; the only owner-gated endpoint
  is the test endpoint `/api/admin/ping`.
- No TLS in the server itself — terminate HTTPS at the reverse proxy.

## 7. Quick API verification with curl

```bash
curl -i localhost:8080/api/admin/ping                 # 401 unauthenticated
# dev login (local only):
curl -s -X POST localhost:8080/api/auth/dev-login \
  -H 'Content-Type: application/json' -d '{"email":"other@example.com"}'
# use the returned session token:
curl -i -H "Authorization: Bearer <token>" localhost:8080/api/admin/ping   # 403 non-owner
# repeat with zhian.job@gmail.com → 200
cat ops_audit.jsonl                                   # denials recorded
```
