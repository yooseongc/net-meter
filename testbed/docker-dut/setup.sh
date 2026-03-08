#!/bin/bash
# Docker DUT 테스트 셋업 스크립트
#
# 토폴로지:
#   [client-ns] -- veth-c0 -- [nm-client-br] -- DUT -- [nm-server-br] -- veth-s0 -- [server-ns]
#
# WSL2 + Docker Desktop 환경 지원:
#   브릿지를 WSL2에서 직접 생성한 후 Docker에게 해당 브릿지 이름을 사용하도록 한다.
#
# 사전 요건:
#   - Docker / docker compose 설치
#   - net-meter 바이너리 빌드 완료 (engine/target/debug/net-meter)
#   - root 또는 CAP_NET_ADMIN 권한

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
NET_METER_BIN="${REPO_ROOT}/engine/target/debug/net-meter"
NET_METER_PORT=9090

# Docker에게 이 이름의 브릿지를 사용하도록 지정 (docker-compose.yml과 일치해야 함)
CLIENT_BR="nm-client-br"
SERVER_BR="nm-server-br"

CLIENT_SUBNET="172.20.1.0/24"
SERVER_SUBNET="172.20.2.0/24"
CLIENT_BR_IP="172.20.1.1/24"
SERVER_BR_IP="172.20.2.1/24"
DUT_CLIENT_IP="172.20.1.10"
DUT_SERVER_IP="172.20.2.10"


# ──────────────────────────────────────────────
# 0. 사전 확인
# ──────────────────────────────────────────────
echo "[0/7] Checking prerequisites..."

if [[ $EUID -ne 0 ]]; then
    echo "ERROR: This script must be run as root (sudo)."
    exit 1
fi

if ! command -v docker &>/dev/null; then
    echo "ERROR: docker is not installed."
    exit 1
fi

if [[ ! -f "${NET_METER_BIN}" ]]; then
    echo "ERROR: net-meter binary not found at ${NET_METER_BIN}"
    echo "       Run: cd ${REPO_ROOT}/engine && cargo build --bin net-meter"
    exit 1
fi

echo "  net-meter: ${NET_METER_BIN}"

# ──────────────────────────────────────────────
# 1. 브릿지 직접 생성 (WSL2에서 먼저 만든다)
# ──────────────────────────────────────────────
echo ""
echo "[1/7] Creating bridge interfaces on host..."

for BR in "${CLIENT_BR}" "${SERVER_BR}"; do
    if ip link show "${BR}" &>/dev/null; then
        echo "  ${BR} already exists, skipping."
    else
        ip link add "${BR}" type bridge
        echo "  Created bridge: ${BR}"
    fi
    ip link set "${BR}" up
done

# 브릿지에 게이트웨이 IP 부여 (Docker가 gateway로 사용)
ip addr add "${CLIENT_BR_IP}" dev "${CLIENT_BR}" 2>/dev/null || true
ip addr add "${SERVER_BR_IP}" dev "${SERVER_BR}" 2>/dev/null || true
echo "  ${CLIENT_BR}: ${CLIENT_BR_IP}"
echo "  ${SERVER_BR}: ${SERVER_BR_IP}"

# ──────────────────────────────────────────────
# 2. DUT 컨테이너 시작
# ──────────────────────────────────────────────
echo ""
echo "[2/7] Starting DUT container..."
cd "${SCRIPT_DIR}"
docker compose up -d --build

echo "  Waiting for DUT to be ready..."
for i in $(seq 1 15); do
    if docker inspect nm-dut --format '{{.State.Running}}' 2>/dev/null | grep -q true; then
        break
    fi
    sleep 1
done

if ! docker inspect nm-dut --format '{{.State.Running}}' 2>/dev/null | grep -q true; then
    echo "ERROR: DUT container failed to start."
    docker compose logs dut
    exit 1
fi
echo "  DUT container is running."

# ──────────────────────────────────────────────
# 3. 브릿지가 호스트에서 보이는지 확인
# ──────────────────────────────────────────────
echo ""
echo "[3/7] Verifying bridge interfaces..."

for BR in "${CLIENT_BR}" "${SERVER_BR}"; do
    if ip link show "${BR}" &>/dev/null; then
        echo "  ${BR}: OK"
    else
        echo "ERROR: Bridge ${BR} not found."
        echo "  Docker Desktop이 별도 VM에서 실행 중이라면 native Docker 사용을 권장합니다."
        exit 1
    fi
done

# ──────────────────────────────────────────────
# 4. net-meter 시작 (NS 모드, 백그라운드)
# ──────────────────────────────────────────────
echo ""
echo "[4/7] Starting net-meter in namespace mode..."

"${NET_METER_BIN}" \
    --mode external_port \
    --upper-iface "${CLIENT_BR}" \
    --lower-iface "${SERVER_BR}" \
    --port "${NET_METER_PORT}" &
NET_METER_PID=$!
echo "${NET_METER_PID}" > /tmp/net-meter.pid
echo "  net-meter PID: ${NET_METER_PID}"
sleep 1

# ──────────────────────────────────────────────
# 5. 호스트에서 DUT로 연결성 확인
# ──────────────────────────────────────────────
echo ""
echo "[5/7] Verifying host → DUT connectivity..."

if ping -c 1 -W 2 "${DUT_CLIENT_IP}" &>/dev/null; then
    echo "  host → DUT client side (${DUT_CLIENT_IP}): OK"
else
    echo "  WARNING: host → DUT client side ping failed"
fi

if ping -c 1 -W 2 "${DUT_SERVER_IP}" &>/dev/null; then
    echo "  host → DUT server side (${DUT_SERVER_IP}): OK"
else
    echo "  WARNING: host → DUT server side ping failed"
fi

# 6, 7번 스텝은 external_port 모드에서 불필요 (namespace 없음)
echo ""
echo "[6/7] Skipped (no namespaces in external_port mode)"
echo "[7/7] Skipped"

# ──────────────────────────────────────────────
echo ""
echo "========================================================"
echo "  Setup complete!"
echo ""
echo "  DUT: nm-dut"
echo "    client side: ${DUT_CLIENT_IP} (${CLIENT_BR})"
echo "    server side: ${DUT_SERVER_IP} (${SERVER_BR})"
echo ""
echo "  net-meter: http://localhost:${NET_METER_PORT}"
echo "  net-meter PID: ${NET_METER_PID}"
echo ""
echo "  TestConfig 예시:"
echo "    network.mode: \"external_port\""
echo "    clients: [{ cidr: \"172.20.1.100/24\", count: 10 }]"
echo "    servers: [{ ip: \"172.20.2.200\", port: 8080, protocol: \"http1\" }]"
echo ""
echo "  종료: sudo ./teardown.sh"
echo "========================================================"
