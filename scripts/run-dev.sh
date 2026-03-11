#!/usr/bin/env bash
# net-meter 개발 모드 실행 스크립트 (프론트엔드 unminified 빌드)
#
# 사용법:
#   ./scripts/run-dev.sh                            # loopback 모드, 포트 9090
#   ./scripts/run-dev.sh --config config/namespace.runtime.yaml
#   ./scripts/run-dev.sh --mode namespace           # Namespace 모드 (sudo 필요)
#   ./scripts/run-dev.sh --mode external_port \
#     --upper-iface eth1 --lower-iface eth2         # External Port 모드 (sudo 필요)
#   ./scripts/run-dev.sh --port 8080               # 포트 변경
#   ./scripts/run-dev.sh --no-build                # 엔진 빌드 생략
#   ./scripts/run-dev.sh --no-fe-build             # 프론트엔드 빌드 생략
#
# 네트워크 모드:
#   loopback      기본값. 권한 불필요.
#   namespace     Linux netns 격리. CAP_NET_ADMIN 필요.
#                 --upper-iface (기본: veth-c0) : 호스트 클라이언트측 veth 이름
#                 --lower-iface (기본: veth-s0) : 호스트 서버측 veth 이름
#                 --ns-prefix   (기본: nm)      : NS 이름 prefix
#   external_port 물리 NIC promisc + MTU 설정. CAP_NET_ADMIN 필요.
#                 --upper-iface (필수) : 클라이언트측 물리 NIC (예: eth1)
#                 --lower-iface (필수) : 서버측 물리 NIC (예: eth2)
#                 --mtu         (기본: 1500)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 기본값
PORT=9090
NO_BUILD=false
NO_FE_BUILD=false
CONFIG_PATH=""
MODE="loopback"
UPPER_IFACE="veth-c0"
LOWER_IFACE="veth-s0"
MTU=1500
NS_PREFIX="nm"
FILE_MODE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --config)       CONFIG_PATH="$2";  shift 2 ;;
        --port)         PORT="$2";         shift 2 ;;
        --no-build)     NO_BUILD=true;     shift   ;;
        --no-fe-build)  NO_FE_BUILD=true;  shift   ;;
        --mode)         MODE="$2";         shift 2 ;;
        --upper-iface)  UPPER_IFACE="$2";  shift 2 ;;
        --lower-iface)  LOWER_IFACE="$2";  shift 2 ;;
        --mtu)          MTU="$2";          shift 2 ;;
        --ns-prefix)    NS_PREFIX="$2";    shift 2 ;;
        -h|--help)
            sed -n '2,25p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "[ERROR] Unknown option: $1"; exit 1 ;;
    esac
done

if [[ -n "$CONFIG_PATH" && -f "$CONFIG_PATH" ]]; then
    FILE_MODE="$(sed -n 's/^[[:space:]]*mode:[[:space:]]*//p' "$CONFIG_PATH" | head -n 1 | tr -d '"' | tr -d "'")"
fi

EFFECTIVE_MODE="$MODE"
if [[ -n "$FILE_MODE" && "$MODE" == "loopback" ]]; then
    EFFECTIVE_MODE="$FILE_MODE"
fi

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }
step()  { echo -e "${CYAN}[STEP]${NC} $*"; }

# 모드별 권한 검증
if [[ "$EFFECTIVE_MODE" == "namespace" || "$EFFECTIVE_MODE" == "external_port" ]]; then
    if [[ "$EUID" -ne 0 ]]; then
        error "Mode '$EFFECTIVE_MODE' requires root (CAP_NET_ADMIN).\n  Run: sudo $0 $*"
    fi
fi

echo "============================================"
echo "  net-meter (dev mode — unminified build)"
echo "============================================"
info "Mode:        $EFFECTIVE_MODE"
[[ -n "$CONFIG_PATH" ]] && info "Config:      $CONFIG_PATH"
if [[ "$MODE" != "loopback" ]]; then
    info "Upper iface: $UPPER_IFACE"
    info "Lower iface: $LOWER_IFACE"
fi
[[ "$MODE" == "namespace" ]]     && info "NS prefix:   $NS_PREFIX"
[[ "$MODE" == "external_port" ]] && info "MTU:         $MTU"
echo ""

# 1. 프론트엔드 빌드 (unminified)
if $NO_FE_BUILD; then
    info "Skipping frontend build (--no-fe-build)"
else
    command -v node &>/dev/null || error "Node.js not found."
    step "Building frontend (dev — unminified)..."
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
    step "Building engine..."
    cd "$REPO_ROOT/engine"
    cargo build --bin net-meter
    info "Engine built: target/debug/net-meter"
    cd "$REPO_ROOT"
fi

BINARY="$REPO_ROOT/engine/target/debug/net-meter"
[[ -x "$BINARY" ]] || error "Binary not found: $BINARY\n  Run without --no-build first."

# Ctrl+C 시 정리
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

# 3. 서버 실행
RUN_ARGS=(
    --port "$PORT"
    --web-dir "$STATIC_DIR"
)

if [[ -n "$CONFIG_PATH" ]]; then
    RUN_ARGS+=(--config "$CONFIG_PATH")
fi

RUN_ARGS+=(
    --mode "$MODE"
    --upper-iface "$UPPER_IFACE"
    --lower-iface "$LOWER_IFACE"
    --mtu "$MTU"
    --ns-prefix "$NS_PREFIX"
)

echo ""
echo "--------------------------------------------"
info "Starting backend on port $PORT"
info "UI:  http://localhost:$PORT/"
info "API: http://localhost:$PORT/api/health"
echo "--------------------------------------------"
echo ""

"$BINARY" "${RUN_ARGS[@]}" &
SERVER_PID=$!
wait "$SERVER_PID"
cleanup
