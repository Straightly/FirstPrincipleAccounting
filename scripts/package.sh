#!/usr/bin/env bash
# LedgerZero packaging: build a self-contained deployable artifact.
# Produces dist/ledgerzero-<version>.tar.gz containing the release binary,
# built frontend assets, and the example config. No git repo or toolchain is
# needed on the target machine.
# Run from the repository root: ./scripts/package.sh
set -euo pipefail

cd "$(dirname "$0")/.."

# Cargo may not be on the shell's PATH: rustup.rs installs to ~/.cargo/bin,
# Homebrew's keg-only rustup to /opt/homebrew/opt/rustup/bin.
if ! command -v cargo >/dev/null 2>&1; then
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  [ -x /opt/homebrew/opt/rustup/bin/cargo ] && PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found — install Rust: https://rustup.rs" >&2
  exit 1
fi

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
STAMP=$(date +%Y%m%d%H%M)
NAME="ledgerzero-${VERSION}-${STAMP}"
STAGE="dist/${NAME}"

echo "== Building release binary =="
cargo build --release -p ledgerzero-backend

echo "== Building frontend =="
(cd frontend && { [ -d node_modules ] || npm install --no-fund --no-audit; } && npm run build)

echo "== Staging ${STAGE} =="
rm -rf "$STAGE"
mkdir -p "$STAGE/frontend"
cp target/release/ledgerzero-backend "$STAGE/"
cp -R frontend/dist "$STAGE/frontend/dist"
cp server.config.example.toml "$STAGE/"
cp docs/LedgerZero_Run_and_Deploy.md "$STAGE/DEPLOY.md"

cat > "$STAGE/README.txt" <<EOF
LedgerZero ${VERSION} (packaged ${STAMP})

1. cp server.config.example.toml server.config.toml   # then edit
2. ./ledgerzero-backend server.config.toml
3. Verify: curl localhost:8080/api/health

See DEPLOY.md for monitoring, OAuth setup, and remote-deployment notes.
The relative paths in the example config (./frontend/dist, ./books,
./ops_audit.jsonl) match this directory layout — run from this directory,
or switch the config to absolute paths.
EOF

echo "== Creating tarball =="
tar -czf "dist/${NAME}.tar.gz" -C dist "$NAME"

echo
echo "Artifact: dist/${NAME}.tar.gz"
echo "Staged:   ${STAGE}/"
