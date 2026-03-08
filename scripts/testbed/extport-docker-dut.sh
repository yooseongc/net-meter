#!/usr/bin/env bash
# net-meter — External Port 모드 Docker DUT 테스트베드
#
# Docker 컨테이너를 투명 L2 브릿지 DUT로 구성한다.
# 두 쌍의 veth를 생성해 호스트 쪽 끝을 net-meter 의 상단/하단 포트로 사용하고,
# 반대쪽 피어를 컨테이너 내부로 이동해 컨테이너 안에서 브릿지로 연결한다.
#
# 토폴로지:
#
#   [net-meter: generator]            [net-meter: responder]
#   veth-nm-up  10.10.1.1/24          10.10.1.2/24  veth-nm-dn
#        |                                                 |
#   veth-nm-up-p ─── [br-dut] ─── veth-nm-dn-p
#                  [Docker: nm-dut (alpine)]
#                  투명 L2 브릿지 DUT
#
# 트래픽 경로:
#   generator (10.10.1.1) → veth-nm-up → Docker br-dut → veth-nm-dn → responder (10.10.1.2)
#
# 참고:
#   - 동일 서브넷(10.10.1.0/24) 사용 → L2 브릿지로 양방향 포워딩
#   - rp_filter 비활성화 필요 (host의 two 인터페이스 간 비대칭 경로)
#   - net-meter external_port 모드는 promisc+MTU 설정 후 loopback 모드로 동작
#     → TestConfig의 server.ip 를 10.10.1.2 로 설정해 트래픽이 veth 경로를 통하게 함
#
# 사용법:
#   sudo ./scripts/testbed/extport-docker-dut.sh setup     # 인프라 생성 + Docker 컨테이너 시작
#   sudo ./scripts/testbed/extport-docker-dut.sh verify    # 연결 확인 (ping, bridge fdb)
#   sudo ./scripts/testbed/extport-docker-dut.sh run-nm    # net-meter external_port 모드 실행
#   sudo ./scripts/testbed/extport-docker-dut.sh teardown  # 정리
#
# 권한: root 또는 CAP_NET_ADMIN 필요, Docker 데몬 실행 중이어야 함

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

IFACE_UP="veth-nm-up"
IFACE_UP_P="veth-nm-up-p"
IFACE_DN="veth-nm-dn"
IFACE_DN_P="veth-nm-dn-p"
CONTAINER_NAME="nm-dut"
DOCKER_IMAGE="alpine:latest"
IP_UP="10.10.1.1"
IP_DN="10.10.1.2"
SUBNET=24
MTU=1500

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }
step()  { echo -e "${CYAN}[STEP]${NC} $*"; }

CMD="${1:-help}"

if [[ "$CMD" == "help" || "$CMD" == "-h" || "$CMD" == "--help" ]]; then
    echo "사용법: sudo $0 {setup|verify|run-nm|teardown}"
    echo ""
    echo "  setup     : veth pair 생성 + Docker DUT 컨테이너 시작 + 브릿지 구성"
    echo "  verify    : ping 으로 Docker 브릿지 통과 연결 확인"
    echo "  run-nm    : net-meter external_port 모드 실행"
    echo "  teardown  : 컨테이너 + veth 모두 정리"
    echo ""
    echo "토폴로지:"
    echo "  [net-meter]                             [Docker: $CONTAINER_NAME]"
    echo "  $IFACE_UP ($IP_UP) ─ $IFACE_UP_P ─── [br-dut] ─── $IFACE_DN_P ─ $IFACE_DN ($IP_DN)"
    exit 0
fi

[[ "$EUID" -eq 0 ]] || error "root 권한이 필요합니다. sudo 로 실행하세요."
command -v docker &>/dev/null || error "Docker가 설치되어 있지 않습니다."

# ─── setup ────────────────────────────────────────────────────────────────────
setup() {
    echo "============================================"
    echo "  ExtPort Docker DUT Testbed — setup"
    echo "============================================"

    # 이전 잔여물 정리
    docker rm -f "$CONTAINER_NAME" 2>/dev/null && warn "기존 컨테이너 $CONTAINER_NAME 제거" || true
    ip link del "$IFACE_UP" 2>/dev/null && warn "기존 $IFACE_UP 제거" || true
    ip link del "$IFACE_DN" 2>/dev/null && warn "기존 $IFACE_DN 제거" || true

    # 1. veth pair 생성 (호스트 ↔ 컨테이너)
    step "veth pair 생성: $IFACE_UP ↔ $IFACE_UP_P"
    ip link add "$IFACE_UP" type veth peer name "$IFACE_UP_P"

    step "veth pair 생성: $IFACE_DN ↔ $IFACE_DN_P"
    ip link add "$IFACE_DN" type veth peer name "$IFACE_DN_P"

    # 2. 호스트 쪽 veth에 IP 설정 (net-meter가 바인딩할 주소)
    step "호스트 IP 설정: $IFACE_UP=$IP_UP/$SUBNET, $IFACE_DN=$IP_DN/$SUBNET"
    ip addr add "$IP_UP/$SUBNET" dev "$IFACE_UP"
    ip addr add "$IP_DN/$SUBNET" dev "$IFACE_DN"
    ip link set "$IFACE_UP" mtu "$MTU" up
    ip link set "$IFACE_DN" mtu "$MTU" up
    # 피어도 UP (netns 이동 전)
    ip link set "$IFACE_UP_P" mtu "$MTU" up
    ip link set "$IFACE_DN_P" mtu "$MTU" up

    # 3. rp_filter 비활성화 (비대칭 경로: up으로 보내고 dn으로 돌아옴)
    step "rp_filter 비활성화 ($IFACE_UP, $IFACE_DN)"
    sysctl -qw "net.ipv4.conf.${IFACE_UP}.rp_filter=0"
    sysctl -qw "net.ipv4.conf.${IFACE_DN}.rp_filter=0"
    sysctl -qw "net.ipv4.conf.all.rp_filter=0"

    # 4. Docker 컨테이너 시작 (--network=none: 기본 네트워크 없이)
    step "Docker 컨테이너 시작: $CONTAINER_NAME (image: $DOCKER_IMAGE)"
    docker pull "$DOCKER_IMAGE" -q
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network none \
        --cap-add NET_ADMIN \
        --privileged \
        "$DOCKER_IMAGE" \
        sleep infinity

    # 5. 컨테이너 PID 조회 후 veth 피어를 컨테이너 네임스페이스로 이동
    local CNT_PID
    CNT_PID=$(docker inspect -f '{{.State.Pid}}' "$CONTAINER_NAME")
    [[ -n "$CNT_PID" && "$CNT_PID" -gt 0 ]] || error "컨테이너 PID를 가져올 수 없습니다."

    step "veth 피어 이동 → 컨테이너 netns (PID=$CNT_PID)"
    ip link set "$IFACE_UP_P" netns "$CNT_PID"
    ip link set "$IFACE_DN_P" netns "$CNT_PID"

    # 6. 컨테이너 내부에서 브릿지 구성 (투명 L2 DUT)
    step "컨테이너 내부 브릿지 구성 (br-dut)"
    docker exec "$CONTAINER_NAME" sh -c "
        ip link add br-dut type bridge &&
        ip link set br-dut type bridge stp_state 0 &&
        ip link set ${IFACE_UP_P} master br-dut &&
        ip link set ${IFACE_DN_P} master br-dut &&
        ip link set ${IFACE_UP_P} up &&
        ip link set ${IFACE_DN_P} up &&
        ip link set br-dut up
    "

    echo ""
    info "========================================"
    info "  Docker DUT 설정 완료"
    info "========================================"
    info "  컨테이너 : $CONTAINER_NAME"
    info "  상단 포트: $IFACE_UP  → $IP_UP/$SUBNET"
    info "  하단 포트: $IFACE_DN  → $IP_DN/$SUBNET"
    info "  DUT 브릿지: br-dut (컨테이너 내부)"
    echo ""
    info "연결 확인  : sudo $0 verify"
    info "net-meter  : sudo $0 run-nm"
    info "정    리   : sudo $0 teardown"
}

# ─── verify ───────────────────────────────────────────────────────────────────
verify() {
    echo "============================================"
    echo "  ExtPort Docker DUT Testbed — 연결 확인"
    echo "============================================"

    # 컨테이너 상태 확인
    docker inspect "$CONTAINER_NAME" --format '{{.State.Status}}' 2>/dev/null | grep -q running \
        || error "컨테이너 $CONTAINER_NAME 가 실행 중이지 않습니다. setup 먼저 실행하세요."

    step "컨테이너 내부 브릿지 상태"
    docker exec "$CONTAINER_NAME" ip link show br-dut 2>/dev/null || warn "br-dut 없음"

    step "컨테이너 내부 인터페이스 목록"
    docker exec "$CONTAINER_NAME" ip link show

    echo ""
    step "호스트 → $IP_DN ping via $IFACE_UP (Docker 브릿지 통과)"
    # -I veth-nm-up: 강제로 veth-nm-up 경유 → Docker 브릿지 통과 → veth-nm-dn 도착
    if ping -c4 -W2 -I "$IFACE_UP" "$IP_DN"; then
        info "SUCCESS: $IP_UP → (Docker br-dut) → $IP_DN"
    else
        warn "ping 실패. 라우팅/rp_filter 설정을 확인하세요."
        info "힌트: sysctl net.ipv4.conf.all.rp_filter=0"
    fi

    echo ""
    step "컨테이너 내부 MAC 테이블"
    docker exec "$CONTAINER_NAME" bridge fdb show 2>/dev/null || warn "bridge 명령 없음 (정상)"
}

# ─── run-nm ───────────────────────────────────────────────────────────────────
run_nm() {
    echo "============================================"
    echo "  net-meter External Port 모드 실행"
    echo "============================================"

    # 컨테이너 실행 중 확인
    docker inspect "$CONTAINER_NAME" --format '{{.State.Status}}' 2>/dev/null | grep -q running \
        || error "컨테이너 $CONTAINER_NAME 가 실행 중이지 않습니다. setup 먼저 실행하세요."

    info "external_port 모드로 net-meter를 실행합니다."
    info "  상단 NIC: $IFACE_UP  (promisc + MTU $MTU 설정됨)"
    info "  하단 NIC: $IFACE_DN  (promisc + MTU $MTU 설정됨)"
    echo ""
    info "TestConfig 설정 안내:"
    info "  - server.ip = \"$IP_DN\"  (responder가 $IP_DN 에 바인딩)"
    info "  - client cidr = \"$IP_UP/32\" (generator가 $IP_UP 에서 발신)"
    info "  → 트래픽 경로: $IP_UP → veth-nm-up → Docker br-dut → veth-nm-dn → $IP_DN"
    echo ""

    "$REPO_ROOT/scripts/run-dev.sh" \
        --mode external_port \
        --upper-iface "$IFACE_UP" \
        --lower-iface "$IFACE_DN" \
        --mtu "$MTU"
}

# ─── teardown ─────────────────────────────────────────────────────────────────
teardown() {
    echo "============================================"
    echo "  ExtPort Docker DUT Testbed — teardown"
    echo "============================================"

    step "Docker 컨테이너 제거: $CONTAINER_NAME"
    docker rm -f "$CONTAINER_NAME" 2>/dev/null && info "Removed $CONTAINER_NAME" || warn "$CONTAINER_NAME not found"

    # 컨테이너 제거 시 내부 veth 피어도 함께 제거됨
    # 호스트 쪽 veth 제거
    step "호스트 veth 제거: $IFACE_UP, $IFACE_DN"
    ip link del "$IFACE_UP" 2>/dev/null && info "Deleted $IFACE_UP" || warn "$IFACE_UP not found"
    ip link del "$IFACE_DN" 2>/dev/null && info "Deleted $IFACE_DN" || warn "$IFACE_DN not found"

    step "rp_filter 복원"
    sysctl -qw "net.ipv4.conf.all.rp_filter=1" 2>/dev/null || true

    info "정리 완료"
}

# ─── main ─────────────────────────────────────────────────────────────────────
case "$CMD" in
    setup)    setup ;;
    verify)   verify ;;
    run-nm)   run_nm ;;
    teardown) teardown ;;
    *)
        echo "알 수 없는 명령: $CMD"
        echo "사용법: sudo $0 {setup|verify|run-nm|teardown|help}"
        exit 1
        ;;
esac
