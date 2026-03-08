#!/bin/bash
# veth-dut 테스트 정리 스크립트

set -e

UPPER_IFACE="veth-c0"
LOWER_IFACE="veth-s0"
UPPER_PEER="veth-c1"
LOWER_PEER="veth-s1"
BRIDGE="br-dut"

echo "[1/3] Stopping net-meter..."
if [[ -f /tmp/net-meter.pid ]]; then
    PID=$(cat /tmp/net-meter.pid)
    kill "${PID}" 2>/dev/null && echo "  Sent SIGTERM to PID ${PID}." || echo "  Already stopped."
    rm -f /tmp/net-meter.pid
else
    pkill -f "net-meter" 2>/dev/null && echo "  Stopped net-meter." || echo "  Not running."
fi
sleep 1

echo ""
echo "[2/3] Removing veth pairs..."
for iface in "${UPPER_IFACE}" "${LOWER_IFACE}"; do
    if ip link show "${iface}" &>/dev/null; then
        ip link del "${iface}" 2>/dev/null && echo "  Removed: ${iface} (peer auto-removed)" || true
    fi
done

echo ""
echo "[3/3] Removing bridge..."
if ip link show "${BRIDGE}" &>/dev/null; then
    ip link set "${BRIDGE}" down
    ip link del "${BRIDGE}" 2>/dev/null && echo "  Removed: ${BRIDGE}" || true
fi

echo ""
echo "Teardown complete."
