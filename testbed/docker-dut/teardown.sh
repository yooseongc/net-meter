#!/bin/bash
# Docker DUT 테스트 정리 스크립트

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CLIENT_BR="nm-client-br"
SERVER_BR="nm-server-br"

echo "[1/4] Stopping net-meter..."
if [[ -f /tmp/net-meter.pid ]]; then
    PID=$(cat /tmp/net-meter.pid)
    kill "${PID}" 2>/dev/null && echo "  Sent SIGTERM to PID ${PID}" || echo "  Already stopped."
    rm -f /tmp/net-meter.pid
else
    pkill -f "net-meter" 2>/dev/null && echo "  Stopped net-meter." || echo "  Not running."
fi
sleep 2

echo ""
echo "[2/4] Stopping DUT container..."
cd "${SCRIPT_DIR}"
docker compose down
echo "  DUT container stopped."

echo ""
echo "[3/4] Cleaning up namespaces and veth pairs..."
for ns in nm-client nm-server; do
    if ip netns show 2>/dev/null | grep -q "^${ns}"; then
        ip netns del "${ns}" 2>/dev/null && echo "  Removed namespace: ${ns}" || true
    fi
done
for iface in veth-c0 veth-s0; do
    if ip link show "${iface}" &>/dev/null; then
        ip link del "${iface}" 2>/dev/null && echo "  Removed interface: ${iface}" || true
    fi
done

echo ""
echo "[4/4] Removing bridge interfaces..."
for BR in "${CLIENT_BR}" "${SERVER_BR}"; do
    if ip link show "${BR}" &>/dev/null; then
        ip link set "${BR}" down
        ip link del "${BR}" 2>/dev/null && echo "  Removed bridge: ${BR}" || true
    fi
done

echo ""
echo "Teardown complete."
