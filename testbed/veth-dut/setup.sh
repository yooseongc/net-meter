#!/usr/bin/env bash
# veth-dut 테스트 스크립트
#
# 토폴로지:
#   veth-c0 (upper) ←── veth-c1 ──┐
#                                   br-dut  (L2 bridge, DUT 시뮬레이션)
#   veth-s0 (lower) ←── veth-s1 ──┘
#
# 사용법:
#   sudo ./setup.sh                     # 기본 (포트 9090)
#   sudo ./setup.sh --port 8080
#   sudo ./setup.sh --no-build          # 엔진 빌드 생략
#   sudo ./setup.sh --no-fe-build       # 프론트엔드 빌드 생략
#
# 종료: Ctrl+C → net-meter 중지 + veth/bridge 자동 정리

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

PORT=9090
NO_BUILD=false
NO_FE_BUILD=false

UPPER_IFACE="veth-c0"
LOWER_IFACE="veth-s0"
UPPER_PEER="veth-c1"
LOWER_PEER="veth-s1"
BRIDGE="br-dut"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --port)        PORT="$2";        shift 2 ;;
        --no-build)    NO_BUILD=true;    shift   ;;
        --no-fe-build) NO_FE_BUILD=true; shift   ;;
        -h|--help)
            sed -n '2,15p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "[ERROR] Unknown option: $1"; exit 1 ;;
    esac
done

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }
step()  { echo -e "${CYAN}[STEP]${NC} $*"; }

# ──────────────────────────────────────────────
# 0. 권한 확인
# ──────────────────────────────────────────────
if [[ $EUID -ne 0 ]]; then
    error "This script requires root (CAP_NET_ADMIN).\n  Run: sudo env PATH=\"\$PATH\" $0 $*"
fi

echo "============================================"
echo "  net-meter veth-dut (external_port mode)"
echo "============================================"
info "Upper iface: ${UPPER_IFACE}  (client side)"
info "Lower iface: ${LOWER_IFACE}  (server side)"
info "Bridge:      ${BRIDGE}       (DUT simulation)"
info "Port:        ${PORT}"
echo ""

# ──────────────────────────────────────────────
# 1. 프론트엔드 빌드
# ──────────────────────────────────────────────
STATIC_DIR="${REPO_ROOT}/engine/crates/control/static"

if $NO_FE_BUILD; then
    info "Skipping frontend build (--no-fe-build)"
else
    command -v node &>/dev/null || error "Node.js not found.\n  Try: sudo env PATH=\"\$PATH\" $0 $*"
    step "Building frontend (dev — unminified)..."
    cd "${REPO_ROOT}/frontend"
    npm run build -- --mode development --minify false
    info "Frontend built → engine/crates/control/static/"
    cd "${REPO_ROOT}"
fi

[[ -d "${STATIC_DIR}" ]] || error "Static dir not found: ${STATIC_DIR}\n  Run: cd ${REPO_ROOT}/frontend && npm run build"

# ──────────────────────────────────────────────
# 2. 엔진 빌드
# ──────────────────────────────────────────────
if $NO_BUILD; then
    info "Skipping engine build (--no-build)"
else
    step "Building engine..."
    cd "${REPO_ROOT}/engine"
    cargo build --bin net-meter
    info "Engine built: target/debug/net-meter"
    cd "${REPO_ROOT}"
fi

BINARY="${REPO_ROOT}/engine/target/debug/net-meter"
[[ -x "${BINARY}" ]] || error "Binary not found: ${BINARY}\n  Run without --no-build first."

# ──────────────────────────────────────────────
# 3. veth pair 및 브릿지 생성
# ──────────────────────────────────────────────
step "Creating veth pairs and bridge..."

if ! ip link show "${BRIDGE}" &>/dev/null; then
    ip link add "${BRIDGE}" type bridge
    info "Created bridge: ${BRIDGE}"
fi

if ! ip link show "${UPPER_IFACE}" &>/dev/null; then
    ip link add "${UPPER_IFACE}" type veth peer name "${UPPER_PEER}"
    info "Created veth pair: ${UPPER_IFACE} ↔ ${UPPER_PEER}"
fi

if ! ip link show "${LOWER_IFACE}" &>/dev/null; then
    ip link add "${LOWER_IFACE}" type veth peer name "${LOWER_PEER}"
    info "Created veth pair: ${LOWER_IFACE} ↔ ${LOWER_PEER}"
fi

ip link set "${UPPER_PEER}" master "${BRIDGE}" 2>/dev/null || true
ip link set "${LOWER_PEER}" master "${BRIDGE}" 2>/dev/null || true

for iface in "${UPPER_IFACE}" "${UPPER_PEER}" "${LOWER_IFACE}" "${LOWER_PEER}" "${BRIDGE}"; do
    ip link set "${iface}" up
done
info "Topology ready."

# ──────────────────────────────────────────────
# 4. Ctrl+C 핸들러 (net-meter 중지 + 토폴로지 정리)
# ──────────────────────────────────────────────
SERVER_PID=""

cleanup() {
    echo ""
    if [[ -n "${SERVER_PID}" ]]; then
        info "Stopping net-meter (PID ${SERVER_PID})..."
        kill "${SERVER_PID}" 2>/dev/null && wait "${SERVER_PID}" 2>/dev/null || true
    fi

    info "Removing veth pairs and bridge..."
    for iface in "${UPPER_IFACE}" "${LOWER_IFACE}"; do
        ip link del "${iface}" 2>/dev/null && echo "  Removed: ${iface}" || true
    done
    if ip link show "${BRIDGE}" &>/dev/null; then
        ip link set "${BRIDGE}" down
        ip link del "${BRIDGE}" 2>/dev/null && echo "  Removed: ${BRIDGE}" || true
    fi

    info "Done."
}
trap cleanup INT TERM

# ──────────────────────────────────────────────
# 5. net-meter 실행
# ──────────────────────────────────────────────
echo ""
echo "--------------------------------------------"
info "Starting net-meter on port ${PORT}"
info "UI:  http://localhost:${PORT}/"
info "API: http://localhost:${PORT}/api/health"
info ""
info "TestConfig 예시:"
info "  network.mode: \"external_port\""
info "  clients: [{ cidr: \"192.168.1.100/24\", count: 10 }]"
info "  servers: [{ ip: \"192.168.2.200\", port: 8080, protocol: \"http1\" }]"
echo "--------------------------------------------"
echo ""

"${BINARY}" \
    --port "${PORT}" \
    --web-dir "${STATIC_DIR}" \
    --mode external_port \
    --upper-iface "${UPPER_IFACE}" \
    --lower-iface "${LOWER_IFACE}" &
SERVER_PID=$!
wait "${SERVER_PID}"
cleanup
