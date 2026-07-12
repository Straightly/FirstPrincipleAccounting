#!/usr/bin/env bash
# LedgerZero full check: build + test every component (Impl Plan working rules).
# Run from the repository root: ./scripts/check.sh
set -uo pipefail

cd "$(dirname "$0")/.."
FAILED=0

# Cargo may not be on the shell's PATH: rustup.rs installs to ~/.cargo/bin,
# Homebrew's keg-only rustup to /opt/homebrew/opt/rustup/bin.
if ! command -v cargo >/dev/null 2>&1; then
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  [ -x /opt/homebrew/opt/rustup/bin/cargo ] && PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

step() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }
fail() { printf '\033[31mFAILED: %s\033[0m\n' "$1"; FAILED=1; }

if command -v cargo >/dev/null 2>&1; then
  step "Rust: format"
  cargo fmt --all || fail "cargo fmt"
  step "Rust: clippy"
  cargo clippy --workspace --all-targets || fail "cargo clippy"
  step "Rust: tests"
  cargo test --workspace || fail "cargo test"
else
  fail "cargo not found — install Rust: https://rustup.rs"
fi

if command -v npm >/dev/null 2>&1; then
  step "Frontend: install + build"
  (cd frontend && { [ -d node_modules ] || npm install --no-fund --no-audit; } && npm run build) \
    || fail "frontend build"
else
  fail "npm not found — install Node.js"
fi

if command -v python3 >/dev/null 2>&1; then
  step "Python: mcp_server tests"
  (cd mcp_server && python3 -m unittest discover -s tests -v) || fail "python tests"
else
  fail "python3 not found"
fi

step "Result"
if [ "$FAILED" -eq 0 ]; then
  echo "All checks passed."
else
  echo "One or more checks FAILED."
  exit 1
fi
