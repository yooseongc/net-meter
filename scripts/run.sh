#!/usr/bin/env bash
# net-meter 빌드 + 실행 통합 스크립트
#
# 사용법:
#   ./scripts/run.sh                  # debug 빌드 후 포트 9090으로 실행
#   ./scripts/run.sh --port 8080      # 포트 변경
#   ./scripts/run.sh --release        # release 빌드 (느리지만 최적화)
#   ./scripts/run.sh --skip-frontend  # 프론트엔드 재빌드 생략
#   ./scripts/run.sh --no-build       # 빌드 생략, 기존 바이너리 바로 실행
#   sudo ./scripts/run.sh             # namespace 모드 시험 (CAP_NET_ADMIN 필요)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT=9090
RELEASE=false
SKIP_FRONTEND=false
NO_BUILD=false

# 인자 파싱
while [[ $# -gt 0 ]]; do
    case "$1" in
        --port)       PORT="$2";        shift 2 ;;
        --release)    RELEASE=true;     shift   ;;
        --skip-frontend) SKIP_FRONTEND=true; shift ;;
        --no-build)   NO_BUILD=true;    shift   ;;
        -h|--help)
            sed -n '2,11p' "$0" | sed 's/^# //'
            exit 0
            ;;
        *) echo "[ERROR] Unknown option: $1"; exit 1 ;;
    esac
done

# 색상
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

echo "============================================"
echo "  net-meter"
echo "============================================"

if $NO_BUILD; then
    info "Skipping build (--no-build)"
else
    # 1. 프론트엔드 빌드
    if $SKIP_FRONTEND; then
        warn "Skipping frontend build (--skip-frontend)"
    else
        if ! command -v node &>/dev/null; then
            warn "Node.js not found. Skipping frontend build."
        else
            info "Building frontend..."
            cd "$REPO_ROOT/frontend"
            npm run build
            info "Frontend built -> engine/crates/control/static/"
            cd "$REPO_ROOT"
        fi
    fi

    # 2. Rust 엔진 빌드
    info "Building engine..."
    cd "$REPO_ROOT/engine"
    if $RELEASE; then
        cargo build --release --bin net-meter
        BINARY="$REPO_ROOT/engine/target/release/net-meter"
    else
        cargo build --bin net-meter
        BINARY="$REPO_ROOT/engine/target/debug/net-meter"
    fi
    info "Engine built: $BINARY"
fi

# 바이너리 경로 결정
if $RELEASE; then
    BINARY="$REPO_ROOT/engine/target/release/net-meter"
else
    BINARY="$REPO_ROOT/engine/target/debug/net-meter"
fi

[[ -x "$BINARY" ]] || error "Binary not found: $BINARY\n  Run without --no-build first."

WEB_DIR="$REPO_ROOT/engine/crates/control/static"
[[ -d "$WEB_DIR" ]] || warn "Frontend static dir not found: $WEB_DIR\n  Only API will be served."

# Ctrl+C 시 서버 정리
SERVER_PID=""
cleanup() {
    echo ""
    if [[ -n "$SERVER_PID" ]]; then
        info "Stopping server (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null || true
    fi
    info "Done."
}
trap cleanup INT TERM

# 3. 서버 실행
echo ""
echo "--------------------------------------------"
info "Starting net-meter on port $PORT"
[[ -d "$WEB_DIR" ]] && info "Frontend: http://localhost:$PORT/"
info "API:      http://localhost:$PORT/api/health"
[[ "$EUID" -ne 0 ]] && warn "Running without root. Namespace mode requires sudo."
echo "--------------------------------------------"
echo ""

"$BINARY" --port "$PORT" --web-dir "$WEB_DIR" &
SERVER_PID=$!

wait "$SERVER_PID"
