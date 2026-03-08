#!/bin/sh
set -e

echo "[DUT] Starting..."
echo "[DUT] ip_forward = $(cat /proc/sys/net/ipv4/ip_forward)"
echo "[DUT] eth0 = $(ip addr show eth0 | grep 'inet ' | awk '{print $2}')"
echo "[DUT] eth1 = $(ip addr show eth1 | grep 'inet ' | awk '{print $2}')"
echo "[DUT] Routes:"
ip route show

# ---------- 여기에 DUT 동작 추가 가능 ----------
#
# [예1] 대역폭 제한 (100Mbps)
# tc qdisc add dev eth0 root tbf rate 100mbit burst 32kbit latency 400ms
#
# [예2] 지연/패킷손실 시뮬레이션
# tc qdisc add dev eth0 root netem delay 10ms loss 1%
#
# [예3] 특정 포트 차단
# iptables -A FORWARD -p tcp --dport 443 -j DROP
#
# -----------------------------------------------

echo "[DUT] Ready (L3 forwarder, ip_forward=1)"
exec sleep infinity
