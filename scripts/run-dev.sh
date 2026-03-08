#!/usr/bin/env bash
# net-meter 개발 모드 실행 스크립트
# 프론트엔드를 정적 빌드하여 백엔드가 서빙 (dev server 없음)
#
# 사용법:
#   ./scripts/run-dev.sh                  # 기본 포트 9090
#   ./scripts/run-dev.sh --port 8080      # 포트 변경
#   ./scripts/run-dev.sh --no-build       # 엔진 빌드 생략
#   ./scripts/run-dev.sh --no-fe-build    # 프론트엔드 빌드 생략
#   sudo ./scripts/run-dev.sh             # namespace 모드 (CAP_NET_ADMIN 필요)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT=9090
NO_BUILD=false
NO_FE_BUILD=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --port)        PORT="$2"; shift 2 ;;
        --no-build)    NO_BUILD=true; shift ;;
        --no-fe-build) NO_FE_BUILD=true; shift ;;
        -h|--help)
            sed -n '2,10p' "$0" | sed 's/^# //'
            exit 0
            ;;
        *) echo "[ERROR] Unknown option: $1"; exit 1 ;;
    esac
done

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

echo "============================================"
echo "  net-meter (dev mode — static build)"
echo "============================================"

# 1. 프론트엔드 빌드 → static/
if $NO_FE_BUILD; then
    info "Skipping frontend build (--no-fe-build)"
else
    command -v node &>/dev/null || error "Node.js not found."
    info "Building frontend (dev mode — unminified, debug symbols)..."
    cd "$REPO_ROOT/frontend"
    npm run build -- --mode development --minify false
    info "Frontend built → engine/crates/control/static/"
    cd "$REPO_ROOT"
fi

STATIC_DIR="$REPO_ROOT/engine/crates/control/static"
[[ -d "$STATIC_DIR" ]] || error "Static dir not found: $STATIC_DIR\n  Run without --no-fe-build first."

# 2. Rust 엔진 빌드
if $NO_BUILD; then
    info "Skipping engine build (--no-build)"
else
    info "Building engine..."
    cd "$REPO_ROOT/engine"
    cargo build --bin net-meter
    info "Engine built: target/debug/net-meter"
    cd "$REPO_ROOT"
fi

BINARY="$REPO_ROOT/engine/target/debug/net-meter"
[[ -x "$BINARY" ]] || error "Binary not found: $BINARY\n  Run without --no-build first."

# 3. 백엔드 실행 (정적 파일 포함)
SERVER_PID=""
cleanup() {
    echo ""
    if [[ -n "$SERVER_PID" ]]; then
        info "Stopping backend (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null || true
    fi
    info "Done."
}
trap cleanup INT TERM

echo ""
echo "--------------------------------------------"
info "Starting backend on port $PORT"
info "UI:  http://localhost:$PORT/"
info "API: http://localhost:$PORT/api/health"
[[ "$EUID" -ne 0 ]] && warn "Running without root. Namespace mode requires sudo."
echo "--------------------------------------------"
echo ""

"$BINARY" --port "$PORT" --web-dir "$STATIC_DIR" &
SERVER_PID=$!
wait "$SERVER_PID"
cleanup
