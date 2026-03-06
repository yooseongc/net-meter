#!/usr/bin/env bash
# net-meter 빌드 스크립트

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-glibc}"  # glibc | musl

echo "=== net-meter build (target: $TARGET) ==="

# 프론트엔드 빌드
if command -v node &>/dev/null; then
    echo "[INFO] Building frontend..."
    cd "$REPO_ROOT/frontend"
    npm run build
    echo "[OK] Frontend built -> engine/crates/control/static/"
    cd "$REPO_ROOT"
fi

# 엔진 빌드
echo "[INFO] Building engine..."
cd "$REPO_ROOT/engine"

if [ "$TARGET" = "musl" ]; then
    cargo build --release --target x86_64-unknown-linux-musl
    echo "[OK] Engine built: engine/target/x86_64-unknown-linux-musl/release/net-meter"
else
    cargo build --release
    echo "[OK] Engine built: engine/target/release/net-meter"
fi

echo ""
echo "=== Build complete ==="
