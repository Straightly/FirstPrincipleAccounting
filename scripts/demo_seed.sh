#!/usr/bin/env bash
# LedgerZero manual-verification seed: creates a demo book with an entity,
# chart, two accounts, an open period, deploys the hand-built "Recording
# startup expense" workflow (M5), and assigns its auto-role to a second
# ("employee") user (M6) — everything docs/LedgerZero_Manual_Verification.md
# needs to walk through in a browser. Requires a running server
# (./scripts/check.sh then `cargo run -p ledgerzero-backend`), curl, python3.
#
# Run from the repository root: ./scripts/demo_seed.sh
# Each run creates a brand-new demo book (fresh UUID) — old ones are
# harmless leftovers; delete a book folder under books_dir anytime.
set -euo pipefail

BASE="${LZ_BASE:-http://127.0.0.1:8080}/api"
OWNER_EMAIL="${LZ_OWNER_EMAIL:-zhian.job@gmail.com}"
EMPLOYEE_EMAIL="${LZ_EMPLOYEE_EMAIL:-demo.employee@example.com}"
PASSPHRASE="${LZ_PASSPHRASE:-demo passphrase, change me}"

if ! curl -s -o /dev/null "$BASE/health"; then
  echo "Cannot reach $BASE/health — is the server running? (cargo run -p ledgerzero-backend)" >&2
  exit 1
fi

COOKIE=$(mktemp)
EMP_COOKIE=$(mktemp)
trap 'rm -f "$COOKIE" "$EMP_COOKIE"' EXIT

uuid() { python3 -c 'import uuid; print(uuid.uuid4())'; }
get() { python3 -c "import sys, json; print(json.load(sys.stdin)$1)"; }

echo "== dev-login as owner ($OWNER_EMAIL) =="
curl -s -c "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$OWNER_EMAIL\"}" "$BASE/auth/dev-login" >/dev/null

echo "== create book =="
BOOK_ID=$(curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"name\":\"Manual Verification Demo\",\"passphrase\":\"$PASSPHRASE\"}" \
  "$BASE/books" | get '["book_id"]')

echo "== create entity =="
ENTITY_ID=$(curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"name\":\"Acme Demo LLC\"}" \
  "$BASE/books/$BOOK_ID/entities" | get '["id"]')

echo "== create USD resource type =="
USD_ID=$(curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"name\":\"US Dollar\",\"kind\":\"CURRENCY\",\"code\":\"USD\",\"unit_of_measure\":\"USD\",\"precision\":2}" \
  "$BASE/books/$BOOK_ID/resource-types" | get '["id"]')

echo "== create chart =="
CHART_ID=$(curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"entity_id\":\"$ENTITY_ID\",\"name\":\"Main\",\"description\":null,\"activate\":true}" \
  "$BASE/books/$BOOK_ID/charts" | get '["id"]')

echo "== create Cash and Startup Expense accounts =="
CASH_ID=$(curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"chart_id\":\"$CHART_ID\",\"name\":\"Cash\",\"code\":null,\"account_type\":\"ASSET\",\"resource_type_id\":\"$USD_ID\",\"parent_account_id\":null,\"validation_rules\":null,\"metadata\":null}" \
  "$BASE/books/$BOOK_ID/accounts" | get '["id"]')
RENT_ID=$(curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"chart_id\":\"$CHART_ID\",\"name\":\"Startup Expense\",\"code\":null,\"account_type\":\"EXPENSE\",\"resource_type_id\":\"$USD_ID\",\"parent_account_id\":null,\"validation_rules\":null,\"metadata\":null}" \
  "$BASE/books/$BOOK_ID/accounts" | get '["id"]')

echo "== create an open period covering today =="
START=$(python3 -c 'import datetime; print((datetime.date.today()-datetime.timedelta(days=1)).isoformat())')
END=$(python3 -c 'import datetime; print((datetime.date.today()+datetime.timedelta(days=60)).isoformat())')
curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"entity_id\":\"$ENTITY_ID\",\"name\":\"Demo period\",\"start_date\":\"$START\",\"end_date\":\"$END\"}" \
  "$BASE/books/$BOOK_ID/periods" >/dev/null

echo "== deploy the hand-built 'Recording startup expense' workflow (M5) =="
curl -s -b "$COOKIE" -H 'Content-Type: application/json' -d "{
  \"workflow_deployment_id\": \"2ef2f432-a548-4f24-87a2-8521bde76af8\",
  \"workflow_id\": \"5230b634-7ad9-46fa-a069-979e6c658eb3\",
  \"entity_id\": \"$ENTITY_ID\",
  \"workflow_name\": \"Recording startup expense\",
  \"description\": \"Hand-built reference workflow (M5)\",
  \"backend_api_calls\": [\"post_entry\"]
}" "$BASE/books/$BOOK_ID/workflows/deploy" >/dev/null

ROLE_ID=$(curl -s -b "$COOKIE" "$BASE/books/$BOOK_ID/roles?entity_id=$ENTITY_ID" | get '[0]["role_id"]')

echo "== dev-login as employee ($EMPLOYEE_EMAIL) and assign the workflow's role (M6) =="
EMP_ID=$(curl -s -c "$EMP_COOKIE" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$EMPLOYEE_EMAIL\"}" "$BASE/auth/dev-login" | get '["user"]["user_id"]')
curl -s -b "$COOKIE" -H 'Content-Type: application/json' \
  -d "{\"op_id\":\"$(uuid)\",\"user_id\":\"$EMP_ID\"}" \
  "$BASE/books/$BOOK_ID/roles/$ROLE_ID/users" >/dev/null

cat <<SUMMARY

============================================================
Demo book ready — see docs/LedgerZero_Manual_Verification.md.

  book_id:          $BOOK_ID
  entity_id:        $ENTITY_ID
  cash account:      $CASH_ID
  expense account:   $RENT_ID
  owner:    $OWNER_EMAIL   (bootstrap owner — sees everything)
  employee: $EMPLOYEE_EMAIL   (assigned only "Recording startup expense")
============================================================
SUMMARY
