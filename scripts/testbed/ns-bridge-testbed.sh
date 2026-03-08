#!/usr/bin/env bash
# net-meter — Namespace 모드 브릿지 테스트베드
#
# 두 개의 네임스페이스(nm-client, nm-server)를 생성하고 호스트에 Linux 브릿지
# (br-nm-dut)를 배치하여 두 NS 사이에 투명 L2 DUT를 시뮬레이션한다.
#
# 토폴로지:
#
#   [nm-client NS]                     [nm-server NS]
#   veth-nm-c-p  10.10.1.1/24          10.10.1.2/24  veth-nm-s-p
#        |                                                  |
#   veth-nm-c ──────────── br-nm-dut ────────────── veth-nm-s
#                         (L2 bridge)
#                       [HOST 네임스페이스]
#
# net-meter NS 모드와의 차이:
#   - net-meter NS 모드는 10.10.1.x ↔ 10.20.1.x (다른 서브넷 + IP 포워딩)
#   - 이 testbed는 동일 서브넷 + L2 브릿지 → 실제 스위치/DUT 삽입 시나리오 검증
#
# 사용법:
#   sudo ./scripts/testbed/ns-bridge-testbed.sh setup     # 토폴로지 생성
#   sudo ./scripts/testbed/ns-bridge-testbed.sh verify    # 연결 확인 (ping)
#   sudo ./scripts/testbed/ns-bridge-testbed.sh teardown  # 정리
#   sudo ./scripts/testbed/ns-bridge-testbed.sh run-nm    # net-meter 실행 (별도 NS 모드)
#
# 권한: root 또는 CAP_NET_ADMIN 필요

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

IFACE_C="veth-nm-c"
IFACE_S="veth-nm-s"
IFACE_C_P="veth-nm-c-p"
IFACE_S_P="veth-nm-s-p"
NS_CLIENT="nm-client"
NS_SERVER="nm-server"
BRIDGE="br-nm-dut"
IP_CLIENT="10.10.1.1"
IP_SERVER="10.10.1.2"
SUBNET=24

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }
step()  { echo -e "${CYAN}[STEP]${NC} $*"; }

CMD="${1:-help}"

# help는 권한 없이 실행
if [[ "$CMD" == "help" || "$CMD" == "-h" || "$CMD" == "--help" ]]; then
    echo "사용법: sudo $0 {setup|verify|teardown|run-nm}"
    echo ""
    echo "  setup     : 네임스페이스 + veth + 브릿지 토폴로지 생성"
    echo "  verify    : ping 으로 L2 브릿지 연결 확인"
    echo "  teardown  : 모든 리소스 정리"
    echo "  run-nm    : net-meter NS 모드 실행 안내"
    exit 0
fi

[[ "$EUID" -eq 0 ]] || error "root 권한이 필요합니다. sudo 로 실행하세요."

# ─── setup ────────────────────────────────────────────────────────────────────
setup() {
    echo "============================================"
    echo "  NS Bridge Testbed — setup"
    echo "============================================"

    step "네임스페이스 생성: $NS_CLIENT, $NS_SERVER"
    ip netns add "$NS_CLIENT"
    ip netns add "$NS_SERVER"

    # client 측 veth pair
    step "veth pair 생성: $IFACE_C ↔ $IFACE_C_P"
    ip link add "$IFACE_C" type veth peer name "$IFACE_C_P"
    ip link set "$IFACE_C_P" netns "$NS_CLIENT"
    ip netns exec "$NS_CLIENT" ip addr add "$IP_CLIENT/$SUBNET" dev "$IFACE_C_P"
    ip netns exec "$NS_CLIENT" ip link set "$IFACE_C_P" up
    ip netns exec "$NS_CLIENT" ip link set lo up

    # server 측 veth pair
    step "veth pair 생성: $IFACE_S ↔ $IFACE_S_P"
    ip link add "$IFACE_S" type veth peer name "$IFACE_S_P"
    ip link set "$IFACE_S_P" netns "$NS_SERVER"
    ip netns exec "$NS_SERVER" ip addr add "$IP_SERVER/$SUBNET" dev "$IFACE_S_P"
    ip netns exec "$NS_SERVER" ip link set "$IFACE_S_P" up
    ip netns exec "$NS_SERVER" ip link set lo up

    # 브릿지 생성 (DUT 시뮬레이션)
    step "브릿지 생성: $BRIDGE (STP 비활성)"
    ip link add "$BRIDGE" type bridge
    ip link set "$BRIDGE" type bridge stp_state 0   # STP 끄기 (빠른 포워딩)
    ip link set "$IFACE_C" master "$BRIDGE"
    ip link set "$IFACE_S" master "$BRIDGE"
    ip link set "$IFACE_C" up
    ip link set "$IFACE_S" up
    ip link set "$BRIDGE" up

    echo ""
    info "토폴로지 생성 완료"
    info "  $NS_CLIENT NS : $IP_CLIENT/$SUBNET on $IFACE_C_P"
    info "  $NS_SERVER NS : $IP_SERVER/$SUBNET on $IFACE_S_P"
    info "  Bridge (DUT)  : $BRIDGE (members: $IFACE_C, $IFACE_S)"
    echo ""
    info "연결 확인: sudo $0 verify"
    info "정   리  : sudo $0 teardown"
}

# ─── verify ───────────────────────────────────────────────────────────────────
verify() {
    echo "============================================"
    echo "  NS Bridge Testbed — 연결 확인"
    echo "============================================"

    step "$NS_CLIENT → $NS_SERVER ping ($IP_CLIENT → $IP_SERVER)"
    if ip netns exec "$NS_CLIENT" ping -c4 -W2 "$IP_SERVER"; then
        info "SUCCESS: client → server 연결 OK"
    else
        error "FAIL: client → server 연결 실패"
    fi

    echo ""
    step "$NS_SERVER → $NS_CLIENT ping ($IP_SERVER → $IP_CLIENT)"
    if ip netns exec "$NS_SERVER" ping -c4 -W2 "$IP_CLIENT"; then
        info "SUCCESS: server → client 연결 OK"
    else
        error "FAIL: server → client 연결 실패"
    fi

    echo ""
    step "브릿지 MAC 테이블 확인"
    bridge fdb show dev "$IFACE_C" 2>/dev/null || true
    bridge fdb show dev "$IFACE_S" 2>/dev/null || true
}

# ─── teardown ─────────────────────────────────────────────────────────────────
teardown() {
    echo "============================================"
    echo "  NS Bridge Testbed — teardown"
    echo "============================================"

    step "브릿지 삭제: $BRIDGE"
    ip link del "$BRIDGE" 2>/dev/null && info "Deleted $BRIDGE" || warn "$BRIDGE not found"

    step "veth 삭제: $IFACE_C, $IFACE_S (peer는 NS 삭제 시 자동 제거)"
    ip link del "$IFACE_C" 2>/dev/null && info "Deleted $IFACE_C" || warn "$IFACE_C not found"
    ip link del "$IFACE_S" 2>/dev/null && info "Deleted $IFACE_S" || warn "$IFACE_S not found"

    step "네임스페이스 삭제: $NS_CLIENT, $NS_SERVER"
    ip netns del "$NS_CLIENT" 2>/dev/null && info "Deleted $NS_CLIENT" || warn "$NS_CLIENT not found"
    ip netns del "$NS_SERVER" 2>/dev/null && info "Deleted $NS_SERVER" || warn "$NS_SERVER not found"

    info "정리 완료"
}

# ─── run-nm ───────────────────────────────────────────────────────────────────
run_nm() {
    echo "============================================"
    echo "  net-meter NS 모드 실행"
    echo "============================================"
    warn "이 testbed와 별개로 net-meter NS 모드를 실행합니다."
    warn "net-meter는 독자적인 NS(nm-client, nm-server)와 veth(veth-c0, veth-s0)를"
    warn "새로 생성하므로, 이 testbed의 nm-client/nm-server와 충돌하지 않습니다."
    echo ""
    info "net-meter NS 모드 실행 (다른 NS prefix / iface 사용):"
    echo "  sudo $REPO_ROOT/scripts/run-dev.sh --mode namespace \\"
    echo "    --ns-prefix nmt --upper-iface veth-c0 --lower-iface veth-s0"
    echo ""
    info "이 testbed의 브릿지를 DUT로 삼으려면 동일 서브넷 설정이 필요합니다."
    info "현재 net-meter NS 모드는 10.10.1.x ↔ 10.20.1.x (IP 포워딩) 방식을 사용합니다."
}

# ─── main ─────────────────────────────────────────────────────────────────────
case "$CMD" in
    setup)    setup ;;
    verify)   verify ;;
    teardown) teardown ;;
    run-nm)   run_nm ;;
    *)
        echo "사용법: sudo $0 {setup|verify|teardown|run-nm}"
        echo ""
        echo "  setup     : 네임스페이스 + veth + 브릿지 토폴로지 생성"
        echo "  verify    : ping 으로 L2 브릿지 연결 확인"
        echo "  teardown  : 모든 리소스 정리"
        echo "  run-nm    : net-meter NS 모드 실행 안내"
        ;;
esac
