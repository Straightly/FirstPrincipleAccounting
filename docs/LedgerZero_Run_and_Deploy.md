# LedgerZero — Run, Monitor, and Deploy (as of M10, Phase 1 complete)

Everything through Phase 1 exists now: authentication/session, the encrypted
per-book ledger with the full accounting API, hand-built and AI-generated
workflows with role-based authorization, the launcher's book/workflow picker,
backup/close/restore, and (this milestone) structured request logging. This
document covers running it locally, packaging it, deploying it to a real
server — including an Oracle Cloud VM — and the operational runbook you need
once real books are involved. Phase 2 (periods-in-practice/reconciliation,
sub-books/consolidation — Impl Plan M11/M12) is deferred and not covered here.

## 1. Run locally

Prerequisites: Rust, Node.js 20+, Python 3.11+.

```bash
./scripts/check.sh                      # build + test everything (fmt, clippy, full suite, frontend build, Python tests)
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
- After login: a book picker (`GET /api/books/mine`) — the owner sees every
  book that has ever existed on this server; anyone else sees only books
  where they hold at least one workflow-granting role. Selecting a book
  loads that book's workflow menu (`workflows/mine`); each entry navigates
  to a self-contained workflow app.
- The bootstrap owner (`bootstrap_owner_email` in config) can additionally
  reach `/api/admin/ping` and every backup/close/restore/create-book
  endpoint; anyone else gets `403`.

`./scripts/demo_seed.sh` seeds a book, chart, accounts, an open period, a
hand-built workflow, and a second ("employee") user with a role, so there's
something to click through immediately —
`docs/LedgerZero_Manual_Verification.md` walks the whole thing end to end,
including the AI-generation (M8) and backup/restore (M9) flows via curl.

## 3. Monitoring while it runs

| Signal | Where | What it tells you |
|---|---|---|
| `GET /api/health` | `curl localhost:8080/api/health` | `{"status":"ok","engine_version":...}` — liveness probe |
| Request log | stdout (or `journalctl -u ledgerzero` under systemd), when `RUST_LOG=info` or higher | One line per request: `method`, `uri`, `status`, `latency_ms` — **never** headers, cookies, or request bodies, so session tokens and passphrases (login, `create_accounting_book`, `open_book`) never reach the log stream (`backend/src/app.rs`, Impl Plan M10) |
| `ops_audit.jsonl` | file next to the binary (path in config) | every **authorization** denial (authenticated but not permitted — `403`), append-only JSON lines; this is the security-relevant log, distinct from the operational request log above. An unauthenticated request (`401`, no/invalid session) is rejected before it reaches any authorization check, so it isn't in here — that's visible in the request log as a `401` instead |

Example uptime check: `curl -fsS localhost:8080/api/health || alert`.
Example log volume control: `RUST_LOG=warn` for a quiet log with only
warnings/errors; unset entirely for silence (the server still runs — the
subscriber just discards below-default levels).

**Still deferred beyond v1:** metrics (Prometheus-style counters/histograms)
and distributed tracing (OpenTelemetry spans across MCP → backend). The
request log above and `ops_audit.jsonl` are what v1 ships; revisit if a real
deployment turns out to need more than "is it up" and "who got denied what."

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
- `frontend_dist`, `books_dir`, `ops_audit_log`, `dev_artifacts_dir`:
  absolute paths, or run with `WorkingDirectory` set to match the relative
  ones (the packaged tarball's example config assumes the latter).

Run it under a supervisor (systemd unit, `Restart=always`) and probe
`/api/health`. §7 below has a ready-to-copy systemd unit and the Oracle
Cloud specifics; §8 covers MFA and ingress restrictions, which the original
spec requires for any non-local deployment (§5.5).

## 7. Deploying on an Oracle Cloud VM (or any systemd-managed Linux host)

These steps are Oracle-Cloud-flavored but apply to any VM running systemd;
substitute your provider's equivalent of a "security list"/"network security
group" where noted.

1. **Provision the VM.** Any shape with 1 OCPU / 6+ GB RAM is comfortable
   for a single-instance deployment (Oracle's Always Free ARM shape works).
   Pick an OS with systemd (Oracle Linux and Ubuntu images both qualify).
2. **Open ingress at the network layer, not just the OS.** Oracle Cloud's
   default VCN Security List (or a Network Security Group, if you're using
   one) denies all inbound traffic except SSH until you add rules —
   allowing port 443 (and 80, if using an ACME HTTP challenge) here is a
   **separate step from** the instance's own firewall below. Missing this
   is the most common "it works locally but I can't reach it" mistake on
   Oracle Cloud specifically: the packets never arrive at the instance at
   all, so nothing in `iptables`/`firewalld` or the app's own logs will show
   anything.
3. **Open ingress at the OS layer.** Oracle Linux images ship `firewalld`
   enabled by default:
   ```bash
   sudo firewall-cmd --permanent --add-service=https
   sudo firewall-cmd --permanent --add-service=http   # only if using an ACME HTTP challenge
   sudo firewall-cmd --reload
   ```
   Do **not** open the LedgerZero port (8080 by default) itself — it stays
   bound to `127.0.0.1`, reachable only through the reverse proxy (§8 covers
   why: TLS termination and the ability to add an IP allowlist in one place).
4. **Put a reverse proxy in front for TLS.** Caddy is the least fuss (it
   handles ACME/Let's Encrypt automatically):
   ```bash
   sudo dnf install -y caddy   # or: apt install caddy
   ```
   Minimal `/etc/caddy/Caddyfile`:
   ```caddyfile
   ledgerzero.example.com {
       reverse_proxy 127.0.0.1:8080
   }
   ```
   `sudo systemctl enable --now caddy` — this is also where an IP allowlist
   goes if you're using one (§8).
5. **Unpack the artifact and install the systemd unit.**
   ```bash
   sudo useradd --system --no-create-home ledgerzero
   sudo mkdir -p /opt/ledgerzero
   sudo tar -xzf ledgerzero-*.tar.gz -C /opt/ledgerzero --strip-components=1
   sudo cp /opt/ledgerzero/server.config.example.toml /opt/ledgerzero/server.config.toml
   sudo $EDITOR /opt/ledgerzero/server.config.toml   # dev_login=false, redirect_url, listen_addr=127.0.0.1:8080
   sudo chown -R ledgerzero:ledgerzero /opt/ledgerzero
   sudo cp scripts/ledgerzero.service.example /etc/systemd/system/ledgerzero.service
   sudo $EDITOR /etc/systemd/system/ledgerzero.service   # confirm paths match /opt/ledgerzero
   sudo systemctl daemon-reload
   sudo systemctl enable --now ledgerzero
   ```
6. **Verify.**
   ```bash
   sudo systemctl status ledgerzero
   journalctl -u ledgerzero -f              # request log (§3) as it happens
   curl -fsS https://ledgerzero.example.com/api/health
   ```

### On-premise (no cloud provider)

The same steps apply minus the cloud-specific security list (step 2): open
whatever perimeter firewall sits in front of the machine (or none, if it's
already on a private network with no public exposure — the strongest ingress
restriction there is), then follow steps 3–6 as written.

## 8. MFA and ingress restrictions for non-local deployments (spec §5.5)

The original spec requires MFA and network restrictions for any deployment
reachable beyond a local/private machine. LedgerZero doesn't implement MFA
itself — authentication is delegated entirely to whichever OAuth provider is
configured (§4), so MFA is enforced **there**, not in this application:

- **Google Workspace**: a workspace admin can require 2-Step Verification
  for the whole domain (Admin console → Security → 2-Step Verification →
  Enforcement). This covers every user who logs into LedgerZero through
  that domain's Google accounts with no LedgerZero-side changes.
  Personal Google accounts: the account owner enables 2FA on their own
  Google account settings — same effect, just not admin-enforceable.
  When adding a different `[[auth_providers]]` domain (Microsoft, an
  enterprise IdP), the same principle holds: enforce MFA at the identity
  provider, since LedgerZero trusts whatever identity the provider vouches
  for and has no visibility into how that identity was authenticated.
- **Never rely on `dev_login` as a substitute.** It bypasses the identity
  provider (and therefore any MFA it enforces) entirely — confirm
  `[dev_login] enabled = false` on anything network-reachable; the server
  logs a warning at startup if it's on, precisely so this isn't silent.

Ingress restrictions, layered (cheapest/most-effective first):

1. **Cloud-provider network layer** — Oracle Cloud Security List / Network
   Security Group (§7 step 2), AWS Security Group, GCP firewall rule, etc.
   Restrict to known IP ranges (office/VPN egress) if the user base is
   small and fixed; this is the single highest-leverage control since
   traffic is dropped before it reaches the instance at all.
2. **Reverse proxy** — an IP allowlist in Caddy/nginx (in front of the
   backend, §7 step 4) if the network layer above isn't fine-grained enough,
   or as defense in depth alongside it.
3. **Private network only** — if every user is already on a VPN or
   corporate network, skip public exposure entirely: bind the reverse proxy
   (or the backend directly, still behind TLS) to the private interface
   only. This is the strongest restriction because there's no public
   ingress path to restrict.
4. **The backend's own `listen_addr`** stays `127.0.0.1` regardless of which
   of the above you use (§6/§7) — it should never be the thing directly
   exposed to any network, public or private, so TLS termination and any
   IP allowlisting stay in one place (the reverse proxy) instead of
   duplicated into the application.

## 9. Operational runbook

Day-to-day and incident-response procedures for someone operating a real
deployment, not just developing against one.

### Bootstrap (first run on a fresh install)

1. Set `bootstrap_owner_email` in `server.config.toml` to the identity that
   should hold owner authority — on a fresh install (no books yet), only
   this identity may call `create_accounting_book`, `open_book`, or any of
   `backup_book`/`close_book`/`restore_book` (Impl Spec §5.3; these three
   are bootstrap-owner-gated specifically because they may need to act
   before a book is open at all, Impl Plan M9).
2. Start the server, sign in as that identity (§2), confirm
   `is_bootstrap_owner: true` and the expected `allowed_actions` list.
3. `create_accounting_book` — this also creates the book's one entity
   (Impl Plan M7), so there's no separate entity-setup step.

### Open-book (routine operation)

- The owner reaches every book that has ever existed on this server through
  the picker (`list_my_books`); anyone else only sees books where they hold
  a role. A restart clears the in-memory open-books map (not the data —
  that's durable on disk, §3.1) — the next `open_book` call re-derives the
  key from the passphrase and reloads the log; nothing about the book
  changes across a restart.
- `close_book` releases a book from memory without touching its files —
  useful before a restore (below), or simply to stop holding a derived key
  in memory for a book nobody is actively using.

### Backup/push (routine + off-machine redundancy)

- `backup_book(book_id, location)` copies `book.json`/`book.data.enc`/
  `book.keystore.json` verbatim to an operator-chosen filesystem location —
  no passphrase involved at any point (Impl Spec §7.3). Run this on a
  schedule (cron/systemd timer calling the API) to a location on different
  physical storage than `books_dir`.
- Separately, every book folder is its own local git repository that the
  backend commits to after each mutation (Impl Spec §3.3) — this is
  point-in-time recovery on the *same* machine, not off-machine redundancy.
  **Push it to a remote** for real redundancy: `git -C books/<book_id> remote
  add origin <url>` once, then `git -C books/<book_id> push` on a schedule.
  The backend never stores remote credentials beside the book (§3.3) — set
  up push credentials (SSH key, credential helper) for whatever user account
  runs the push, entirely outside LedgerZero's own config.
- Either mechanism is best-effort: `git_commit` in `engine/src/storage.rs`
  deliberately never fails a mutation if git itself is unavailable
  (durability already happened in `book.data.enc` via atomic file
  replacement, §3.1) — a backup schedule is what actually protects against
  the whole `books_dir` disappearing.

### Ownership transfer

**Corrected in this milestone (Impl Spec §3.3, resolution R4) — read this
before doing a transfer.** Backup/restore never re-encrypt anything, and
there is no change-passphrase primitive in v1: the same passphrase keeps
working across any number of backups and restores, forever. Practically:

- Anyone who has ever known a book's passphrase can decrypt any copy of it
  they later obtain, by any means — a `backup_book` output, a raw folder
  copy, an old off-machine git push — indefinitely. Handing over a book via
  `backup_book`/`restore_book` does **not** revoke the previous owner's
  access.
- A `restore_book`-based handoff is still better than a raw `cp -r` of the
  book folder: restore only ever moves the three portable files, so the
  receiving side starts with no git history of its own (no commit
  timestamps/cadence carried over) — but that protects operational
  metadata, not the data itself.
- If revoking the old party's access is actually required, v1 has no
  built-in primitive for it: stand up a brand-new book with an
  independently generated passphrase and move the data across some other
  way. This is a real v1 limitation, not an oversight — track it if it
  becomes a real need.

### Restore runbook (disaster recovery)

1. Identify the most recent good `backup_book` location for the affected
   `book_id` (or the book's own git history, §3.3's Git policy, if no
   external backup was taken — `git -C books/<book_id> log` and check out
   an earlier commit into a scratch directory first if you need to go back
   further than the latest state).
2. If the book is currently open in this server process, `close_book` it
   first — `restore_book` refuses with `BOOK_STILL_OPEN` otherwise
   (Impl Plan M9's structural guarantee against overwriting a book still
   loaded in memory).
3. `restore_book(location)` — the target `book_id` is read from the
   location's own `book.json`, never caller-supplied, and the copy is
   wipe-and-replace: it succeeds whether or not a (possibly corrupted) book
   already exists on disk at that id.
4. `open_book` with the *original* passphrase — nothing about it changed
   through any of this. Confirm the balance/history look right before
   resuming normal operation.
5. Nothing in the ledger marks that a restore happened (by design, Impl
   Spec §7.3) — if you need a record that this occurred, note it externally
   (an incident log, this runbook's own audit trail), not inside the book.

## 10. Quick API verification with curl

```bash
curl -i localhost:8080/api/admin/ping                 # 401 unauthenticated
# dev login (local only):
curl -s -X POST localhost:8080/api/auth/dev-login \
  -H 'Content-Type: application/json' -d '{"email":"other@example.com"}'
# use the returned session token:
curl -i -H "Authorization: Bearer <token>" localhost:8080/api/admin/ping   # 403 non-owner
# repeat with the bootstrap_owner_email → 200
cat ops_audit.jsonl                                   # denials recorded
```
