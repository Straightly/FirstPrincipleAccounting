# LedgerZero Manual Verification (M0–M9)

This is the human-in-the-loop counterpart to the automated test suite (70+
tests across `engine/`/`backend/` plus 25 more in `mcp_server/`, all
passing via `./scripts/check.sh`). Automated tests prove the code does
what it claims; this walkthrough is
where you look at the actual running system and judge whether it does what
*you* want. It also doubles as an early draft of the "scripted demo" M10
(hardening) calls for — now the last milestone before Phase 1 ships.

Everything here is safe to run repeatedly — `scripts/demo_seed.sh` creates a
fresh demo book every time (a new random id), so nothing you do here can
corrupt a real book you create later.

## Before you start

1. Build everything once: `./scripts/check.sh` (fmt/clippy/tests, frontend
   build, Python tests — if this doesn't pass, nothing below will work
   either).
2. Start the server from the repo root: `cargo run -p ledgerzero-backend`
   (uses `server.config.toml`; `dev_login` is enabled there, so nothing
   below needs your real Google credentials — though you're welcome to use
   real Google login wherever a step says "sign in").
3. Open **http://localhost:8080** in a browser.

Keep the server running in a terminal for everything below; watch that
terminal's output too — it prints a line for every request and will show
any panics.

---

## Part 1 — Login and identity (M1)

1. You should land on a page titled **LedgerZero** with "Sign in to
   continue." and, since dev login is on, an email box + **Dev sign in**
   button (plus a **Sign in with Google** button if you've configured real
   OAuth credentials).
2. Type your bootstrap owner email (`zhian.job@gmail.com` unless you've
   changed `server.config.toml`) and click **Dev sign in** — or use the
   Google button and sign in for real.
3. You should now see:
   - "Signed in as **\<your name\>** (\<your email\>)"
   - a `user_id:` line (a UUID — this is now your permanent LedgerZero
     identity, independent of which login method you used)
   - "Bootstrap owner: **yes**"
   - "Allowed actions: create_accounting_book, open_book, list_books,
     book_api, admin_ping"
4. Click **Test owner-gated endpoint** — expect a message like `admin ping:
   pong (owner zhian.job@gmail.com)`. This is the M1 walking-skeleton proof
   that routing → session → authorization → a real backend check all work.
5. Click **Rotate session** — expect "Session token rotated." Nothing
   visibly changes, but your session cookie was swapped server-side (you
   can watch this in the server log: a `rotate` line).
6. Click **Sign out** — you should land back on the sign-in screen.
7. Sign back in (dev-login is fine) before continuing.

## Part 2 — Seed a demo book (setup for M4/M5/M6)

There is deliberately no "create a book" button in the launcher yet (M4
added the API, not a UI for it — a real gap, tracked, not something to
worry about now). Seed one from the command line instead:

```sh
./scripts/demo_seed.sh
```

Read its output — it prints the `book_id`/`entity_id` it created and
confirms it deployed the hand-built workflow and assigned it to a second
("employee") user. You don't need to copy those ids down; the picker in
Part 4 finds them for you.

If you want a specific owner/employee email or passphrase instead of the
defaults, set env vars before running it, e.g.:

```sh
LZ_EMPLOYEE_EMAIL=you+employee@example.com ./scripts/demo_seed.sh
```

## Part 3 — What actually landed on disk (M3)

The book the script just created is real, encrypted, and version-controlled
on your machine. Look at it:

```sh
ls books/                         # one folder, named by book_id
ls books/<book_id>/               # book.json, book.data.enc, book.keystore.json
file books/<book_id>/book.data.enc      # binary data, not JSON/text
cat books/<book_id>/book.keystore.json  # readable JSON — but no plaintext key,
                                         # only Argon2id params + a wrapped key
git -C books/<book_id> log --oneline    # one commit per mutation batch,
                                         # e.g. "book created", "mutation batch: <event ids>"
```

That git log is the concrete evidence for M3's durability story: every
change to the book is both durably written to the encrypted file *and*
checkpointed into its own local git history, independent of this
repository's own git history.

## Part 4 — Book picker, as the owner (M6 + M7)

Since Impl Plan M7, a book has exactly one entity — auto-created with the
book itself, no separate entity-selection step. Selecting a book is now the
only thing the picker needs from you.

1. Back in the browser, make sure you're signed in as the owner (Part 1).
2. Scroll to **My workflows**. The **Book** dropdown should populate with
   at least "Manual Verification Demo" (and any earlier demo runs, or real
   books you've made).
3. Select it — a link appears: **Recording startup expense**. As the
   owner, you can see this book/workflow because owners see every book,
   not because you hold a role for it (you don't — check Part 6.1 to see
   the difference).

## Part 5 — Book picker + running the workflow, as a non-owner (M5 + M6 + M7)

This is the part worth taking time over: it's the actual point of M5, M6,
and M7 combined — a real employee, with no special authority, discovering
and running exactly the one thing they're allowed to.

1. Click **Sign out**, then **Dev sign in** as `demo.employee@example.com`
   (or whatever `LZ_EMPLOYEE_EMAIL` you used).
2. Notice "Bootstrap owner: **no**" and "Allowed actions: **none**" — this
   user has zero blanket authority.
3. Under **My workflows**, the **Book** dropdown should show *exactly one*
   book — "Manual Verification Demo" — even if other books exist. That
   scoping is `entities_with_workflows_for_user` doing its job (M6).
4. Select it → the **Recording startup expense** link appears.
5. Click it. You should land on a *different-looking* page — this is a
   completely standalone app (its own React copy, no shared code with the
   launcher — M5's "self-contained artifact" requirement) with the title
   "Recording startup expense" and a line "Signed in as
   demo.employee@example.com".
6. Fill in the form:
   - **Expense date**: leave as today, or pick any date within ~60 days
   - **Description**: anything, e.g. "Laptop for new hire"
   - **Amount**: e.g. `1299.00`
   - **Expense or asset account (account_id)**: paste the `expense
     account:` id the seed script printed
   - **Paid from account_id (source)**: paste the `cash account:` id
   - Memo: optional
7. Click **Record expense**. You should see a green confirmation line:
   `Posted entry <uuid> (execution <uuid>).`

That confirmation means a real, balanced, double-entry journal entry was
posted — by an employee with no blanket authority — purely because they
hold a role granting exactly this workflow.

## Part 6 — Prove the negative cases too

A system that only shows you the happy path hasn't shown you much. These
take two minutes and are the most convincing part of the whole exercise.

### 6.1 — The owner can't run a workflow they aren't assigned to

1. Sign out, sign back in as the **owner**.
2. Navigate directly to the workflow URL you used in Part 5 (copy it from
   your browser history, or revisit it via the picker).
3. You should see a red warning: "You are not currently authorized to run
   this workflow for this entity — the server will reject any submission."
4. Fill in the form anyway and submit it. It should fail. Open your
   browser's network/dev tools (or just trust the UI) — the request comes
   back `403 Forbidden`, `{"error_code":"UNAUTHORIZED_WORKFLOW", ...}`.
   The book's *owner* — who created it — still cannot post through a
   workflow they hold no role for. Authorization here is genuinely
   workflow-scoped, not an owner bypass with extra steps.

### 6.2 — A stranger with no assignment anywhere sees nothing

1. Dev-sign-in as a brand-new email, e.g. `nobody@example.com`.
2. Under **My workflows**, the **Book** dropdown should be empty — "No
   books available to you yet." Not an error, just nothing to show,
   because this user has no role anywhere.

### 6.3 — A wrong passphrase is rejected

A book already open in the server's memory answers `open_book` without
even checking the passphrase (that's the M4 idempotent-open behavior) — so
this check needs a book the *running process* hasn't opened yet. Stop the
server (Ctrl-C), start it again (`cargo run -p ledgerzero-backend`), then
run this from a terminal — no browser cookie-copying needed, it logs in
for itself:

```sh
BOOK_ID=$(ls books/ | head -1)
COOKIE=$(mktemp)
curl -s -c "$COOKIE" -H 'Content-Type: application/json' \
  -d '{"email":"zhian.job@gmail.com"}' \
  http://localhost:8080/api/auth/dev-login >/dev/null
curl -s -b "$COOKIE" -X POST \
  -H 'Content-Type: application/json' \
  -d '{"passphrase":"definitely wrong"}' \
  http://localhost:8080/api/books/$BOOK_ID/open
rm -f "$COOKIE"
```

Expect `{"error_code":"WRONG_PASSPHRASE", ...}` (HTTP 401). Re-run with
`"passphrase":"demo passphrase, change me"` (or whatever `LZ_PASSPHRASE`
you used) and it should succeed instead.

### 6.4 — A non-owner can't create books or see the admin endpoint

Signed in as the employee or stranger from above, click **Test
owner-gated endpoint** — expect `UNAUTHORIZED_API: user is not authorized
for 'admin_ping' ...`.

---

## Part 7 — AI-generated workflow, no hand-edits (M8)

M8 adds a second way to get a workflow deployed: instead of hand-writing
the React artifact (as `demo_seed.sh` does for M5), a Python dev-time
process generates it from a structured request and deploys it through the
same backend API a human developer would use. This part drives that path
yourself from a terminal, then verifies the result in the browser exactly
like Part 4/5 verified the hand-built one.

1. One-time setup, from the repo root:
   ```sh
   cd mcp_server
   python3 -m venv .venv
   .venv/bin/pip install -e .
   ```
2. With the server running (`cargo run -p ledgerzero-backend`), create a
   fresh demo book and its supporting chart/accounts/period — the same
   shape `demo_seed.sh` builds, but driven through the MCP admin
   primitives themselves (`run-tool create_accounting_book`, then
   `create_resource_type`, `create_chart`, `create_account` ×2 for a bank
   account and an offset account, `create_period`). Each command prints
   the id you need for the next one; see `mcp_server/README.md` for the
   full command shapes.
3. Generate the workflow: `run-tool generate_workflow_definition --json
   '{...}'` with the field list from Impl Spec §7.5.2 (bank account,
   transaction date, amount, direction, description, offset account,
   optional reference) — this returns a `workflow_id` and the full
   generated `app.js`/`index.html`, no LLM call needed (v1's "LLM
   wrapping" is deterministic template generation).
4. Deploy it: `run-tool deploy_workflow_definition --json '{"generated":
   <output of step 3>, "book_id": "...", "entity_id": "..."}'` — this
   writes the artifact to `dev_artifacts/workflows/<new id>/` and
   registers the deployment, exactly like `POST
   .../workflows/deploy` does for a hand-built one.
5. Back in the browser: sign in as an employee assigned the auto-created
   role (same `create_role`/`assign_role_to_user` pattern as Part 5,
   here driven by `run-tool assign_role_to_user` instead of curl), select
   the book, and the generated workflow's name appears in **My
   workflows** — no different from a hand-built one.
6. Run it. Post one **Deposit** and one **Withdrawal** against the same
   two accounts, then check `GET /api/books/:id/accounts/:id/balance` on
   the bank account: `debit_total` should reflect the deposit,
   `credit_total` the withdrawal — proof the generated form's
   direction-toggle logic swaps which account is debited correctly both
   ways, not just on the happy path.
7. Repeat Part 6.1's negative case against this workflow's URL, signed in
   as the owner: same client-side warning, same server-side `403
   UNAUTHORIZED_WORKFLOW` — the backend's authorization machinery treats a
   generated artifact identically to a hand-built one.

## Part 8 — Backup and restore (M9)

This is a disaster-recovery drill, not a data-export feature: an operator
moves a book's files to a folder and back, never seeing a passphrase or
touching any key, and the owner reopens the restored book with the *exact
same passphrase as always* — like restoring a `git` checkout somewhere
else. (Impl Spec §7.3 originally speced this as a reader-passphrase
*export*; that was corrected — Appendix A, resolution R3 — once it became
clear v1's actual need is recovery, not sharing a snapshot with someone
who shouldn't have ongoing access.)

1. With the server running, seed a book (Part 2), or reuse an existing
   one — note its `book_id`, one account id, and **the passphrase you
   created it with** (you'll need it again at the end, unchanged).
2. Check a balance before backup, so you have something to compare:
   ```sh
   BOOK_ID=<your book_id>
   ACCOUNT_ID=<your account_id>
   COOKIE=$(mktemp)
   curl -s -c "$COOKIE" -H 'Content-Type: application/json' \
     -d '{"email":"zhian.job@gmail.com"}' \
     http://localhost:8080/api/auth/dev-login >/dev/null
   curl -s -b "$COOKIE" \
     "http://localhost:8080/api/books/$BOOK_ID/accounts/$ACCOUNT_ID/balance"
   ```
3. Back it up to a folder of your choosing — a server-side filesystem
   path, not a browser download:
   ```sh
   LOCATION=/tmp/lz-backup-demo
   curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
     -d "{\"location\":\"$LOCATION\"}" \
     "http://localhost:8080/api/books/$BOOK_ID/backup"
   ls "$LOCATION"   # book.json, book.data.enc, book.keystore.json
   ```
   Open `$LOCATION/book.json` — it's plain, readable JSON (`book_id`,
   `name`, `owner_email`, `entity_id`). `file $LOCATION/book.data.enc`
   reports `data` — opaque, unreadable without the passphrase, which
   never appeared anywhere in this step.
4. Try to restore right now — it should be **refused**, because the book
   is still open in this server process:
   ```sh
   curl -s -w '\nHTTP %{http_code}\n' -b "$COOKIE" -H 'Content-Type: application/json' \
     -d "{\"location\":\"$LOCATION\"}" \
     http://localhost:8080/api/books/restore
   ```
   Expect `409` / `BOOK_STILL_OPEN`.
5. Close it, then simulate an actual disaster — delete the book's real
   folder entirely:
   ```sh
   curl -s -b "$COOKIE" -X POST "http://localhost:8080/api/books/$BOOK_ID/close"
   rm -rf "books/$BOOK_ID"   # from the repo root; the book is now gone
   ```
6. Restore it from the backup:
   ```sh
   curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
     -d "{\"location\":\"$LOCATION\"}" \
     http://localhost:8080/api/books/restore
   ls "books/$BOOK_ID"   # the three files are back
   ```
   Expect the same `book_id`/`name`/`entity_id` back. The restored book is
   left **closed** — restore never decrypted anything, so nothing could be
   loaded into memory even if it wanted to.
7. Open it with a *wrong* passphrase first — expect `401
   WRONG_PASSPHRASE` — then with **the exact passphrase you created it
   with in step 1**:
   ```sh
   curl -s -w '\nHTTP %{http_code}\n' -b "$COOKIE" -H 'Content-Type: application/json' \
     -d '{"passphrase":"<your original passphrase>"}' \
     "http://localhost:8080/api/books/$BOOK_ID/open"
   ```
   Nothing about the passphrase ever changed — that's the whole point.
8. Check the balance from step 2 again — unchanged — then post a new
   entry through it. It should succeed: a restored book is a live
   operational starting point, not a read-only snapshot.
9. `GET /api/books/:id/audit-log` — you will **not** find any kind of
   "restore" event in it. That's deliberate (resolution R3): restore never
   decrypts the log, so nothing could append to it even in principle. The
   fact a restore happened is visible at the filesystem level (the files'
   mtimes, or your own record of running step 6), not inside the ledger.

## What you're *not* expected to check here

- Anything in `engine/` or the storage/idempotency internals — that's what
  the 40+ engine tests are for (`cargo test -p ledgerzero-engine`).
- Anything in `mcp_server/` beyond Part 7 above — the 25 Python tests
  (`cd mcp_server && .venv/bin/python -m unittest discover -s tests`)
  cover the generator/artifact/client/tools logic directly.
- Hardening (M10) — the last milestone before Phase 1 ships, still coming.
- Periods/reconciliation-as-workflow (M11) or sub-books/consolidation
  (M12) — deferred to Phase 2 (Impl Spec Appendix A, resolution R2) until
  Phase 1 has been in real use for a while; not because the design is
  incomplete.
- Cross-browser/mobile rendering — the launcher and workflow artifacts are
  intentionally minimal, unstyled-beyond-basics HTML in this phase.

## If something doesn't match this document

That's exactly what this exercise is for — tell me what you saw instead
and we'll figure out whether it's a bug, a stale doc, or a misunderstanding
before moving on to M10.
