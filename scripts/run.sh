#!/usr/bin/env bash
# net-meter 빌드 + 실행 통합 스크립트
#
# 사용법:
#   ./scripts/run.sh                            # debug 빌드 후 loopback 모드로 실행
#   ./scripts/run.sh --mode namespace           # Namespace 모드 (CAP_NET_ADMIN 필요)
#   ./scripts/run.sh --mode external_port \
#     --upper-iface eth1 --lower-iface eth2     # External Port 모드 (CAP_NET_ADMIN 필요)
#   ./scripts/run.sh --port 8080               # 포트 변경
#   ./scripts/run.sh --release                 # release 빌드
#   ./scripts/run.sh --skip-frontend           # 프론트엔드 재빌드 생략
#   ./scripts/run.sh --no-build                # 빌드 생략, 기존 바이너리 바로 실행
#
# 네트워크 모드:
#   loopback      기본값. 권한 불필요. localhost로 시험.
#   namespace     Linux netns + veth pair 자동 생성. CAP_NET_ADMIN 필요.
#                 --upper-iface (기본: veth-c0) : 호스트 클라이언트측 veth 이름
#                 --lower-iface (기본: veth-s0) : 호스트 서버측 veth 이름
#                 --ns-prefix   (기본: nm)      : NS 이름 prefix (nm-client, nm-server)
#   external_port 물리 NIC에 promisc + MTU 설정. CAP_NET_ADMIN 필요.
#                 --upper-iface (필수) : 클라이언트측 물리 NIC 이름 (예: eth1)
#                 --lower-iface (필수) : 서버측 물리 NIC 이름 (예: eth2)
#                 --mtu         (기본: 1500) : MTU 값

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 기본값
PORT=9090
RELEASE=false
SKIP_FRONTEND=false
NO_BUILD=false
MODE="loopback"
UPPER_IFACE="veth-c0"
LOWER_IFACE="veth-s0"
MTU=1500
NS_PREFIX="nm"

# 인자 파싱
while [[ $# -gt 0 ]]; do
    case "$1" in
        --port)         PORT="$2";         shift 2 ;;
        --release)      RELEASE=true;      shift   ;;
        --skip-frontend) SKIP_FRONTEND=true; shift ;;
        --no-build)     NO_BUILD=true;     shift   ;;
        --mode)         MODE="$2";         shift 2 ;;
        --upper-iface)  UPPER_IFACE="$2";  shift 2 ;;
        --lower-iface)  LOWER_IFACE="$2";  shift 2 ;;
        --mtu)          MTU="$2";          shift 2 ;;
        --ns-prefix)    NS_PREFIX="$2";    shift 2 ;;
        -h|--help)
            sed -n '2,26p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "[ERROR] Unknown option: $1"; exit 1 ;;
    esac
done

# 색상
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }
step()  { echo -e "${CYAN}[STEP]${NC} $*"; }

# 모드별 권한 및 인자 검증
if [[ "$MODE" == "namespace" || "$MODE" == "external_port" ]]; then
    if [[ "$EUID" -ne 0 ]]; then
        error "Mode '$MODE' requires root (CAP_NET_ADMIN).\n  Run: sudo $0 $*"
    fi
fi

if [[ "$MODE" == "external_port" ]]; then
    if [[ "$UPPER_IFACE" == "veth-c0" && "$LOWER_IFACE" == "veth-s0" ]]; then
        warn "external_port 모드에서 기본 veth 이름이 사용됩니다."
        warn "--upper-iface 와 --lower-iface 로 물리 NIC 이름을 지정하세요. (예: eth1, eth2)"
    fi
fi

echo "============================================"
echo "  net-meter"
echo "============================================"
info "Mode:        $MODE"
if [[ "$MODE" != "loopback" ]]; then
    info "Upper iface: $UPPER_IFACE"
    info "Lower iface: $LOWER_IFACE"
fi
if [[ "$MODE" == "namespace" ]]; then
    info "NS prefix:   $NS_PREFIX"
fi
if [[ "$MODE" == "external_port" ]]; then
    info "MTU:         $MTU"
fi
echo ""

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
            step "Building frontend..."
            cd "$REPO_ROOT/frontend"
            npm run build
            info "Frontend built → engine/crates/control/static/"
            cd "$REPO_ROOT"
        fi
    fi

    # 2. Rust 엔진 빌드
    step "Building engine..."
    cd "$REPO_ROOT/engine"
    if $RELEASE; then
        cargo build --release --bin net-meter
        BINARY="$REPO_ROOT/engine/target/release/net-meter"
    else
        cargo build --bin net-meter
        BINARY="$REPO_ROOT/engine/target/debug/net-meter"
    fi
    info "Engine built: $BINARY"
    cd "$REPO_ROOT"
fi

# 바이너리 경로 결정
if $RELEASE; then
    BINARY="$REPO_ROOT/engine/target/release/net-meter"
else
    BINARY="$REPO_ROOT/engine/target/debug/net-meter"
fi

[[ -x "$BINARY" ]] || error "Binary not found: $BINARY\n  Run without --no-build first."

WEB_DIR="$REPO_ROOT/engine/crates/control/static"
[[ -d "$WEB_DIR" ]] || warn "Frontend static dir not found: $WEB_DIR (only API will be served)"

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

# 3. 서버 실행 인자 조립
RUN_ARGS=(
    --port "$PORT"
    --mode "$MODE"
    --upper-iface "$UPPER_IFACE"
    --lower-iface "$LOWER_IFACE"
    --mtu "$MTU"
    --ns-prefix "$NS_PREFIX"
)
[[ -d "$WEB_DIR" ]] && RUN_ARGS+=(--web-dir "$WEB_DIR")

echo ""
echo "--------------------------------------------"
info "Starting net-meter on port $PORT"
[[ -d "$WEB_DIR" ]] && info "Frontend: http://localhost:$PORT/"
info "API:      http://localhost:$PORT/api/health"
echo "--------------------------------------------"
echo ""

"$BINARY" "${RUN_ARGS[@]}" &
SERVER_PID=$!

wait "$SERVER_PID"
