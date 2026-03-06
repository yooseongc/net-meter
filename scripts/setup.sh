#!/usr/bin/env bash
# net-meter 개발 환경 초기 설정 스크립트

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "=== net-meter setup ==="
echo "Repo root: $REPO_ROOT"

# --- Rust 체크 ---
if ! command -v cargo &>/dev/null; then
    echo "[ERROR] Rust/cargo not found. Install from https://rustup.rs"
    exit 1
fi

RUST_VERSION=$(rustc --version)
echo "[OK] Rust: $RUST_VERSION"

# musl 타깃 추가 (정적 빌드용)
if ! rustup target list --installed | grep -q "x86_64-unknown-linux-musl"; then
    echo "[INFO] Adding musl target..."
    rustup target add x86_64-unknown-linux-musl
fi
echo "[OK] musl target: x86_64-unknown-linux-musl"

# --- Node.js 체크 ---
if ! command -v node &>/dev/null; then
    echo "[WARN] Node.js not found. Frontend build will not be available."
    echo "       Install from https://nodejs.org or use nvm"
else
    NODE_VERSION=$(node --version)
    echo "[OK] Node.js: $NODE_VERSION"

    # npm 의존성 설치
    echo "[INFO] Installing frontend dependencies..."
    cd "$REPO_ROOT/frontend"
    npm install
    cd "$REPO_ROOT"
    echo "[OK] Frontend dependencies installed"
fi

# --- 권한 체크 (namespace 관리용) ---
if [ "$EUID" -ne 0 ]; then
    echo "[WARN] Not running as root. Namespace management (Phase 4) requires CAP_NET_ADMIN or root."
    echo "       For development, tests can run without namespaces (localhost mode)."
fi

# --- 디렉터리 생성 ---
mkdir -p "$REPO_ROOT/engine/crates/control/static"

echo ""
echo "=== Setup complete ==="
echo ""
echo "Next steps:"
echo "  # Engine 빌드 (glibc):"
echo "  cd engine && cargo build"
echo ""
echo "  # Engine 빌드 (musl - 정적 바이너리):"
echo "  cd engine && cargo build --target x86_64-unknown-linux-musl"
echo ""
echo "  # Control 서버 실행:"
echo "  cd engine && cargo run --bin net-meter -- --port 9090"
echo ""
echo "  # 프론트엔드 개발 서버:"
echo "  cd frontend && npm run dev"
