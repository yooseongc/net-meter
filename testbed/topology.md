# Testbed 네트워크 토폴로지

## Phase 1: 로컬호스트 모드 (NS 없음)

```
[Generator Task]  ---TCP---> localhost:8080 [Responder Task]
        \                           /
         \                         /
      [Metrics Collector (atomic)]
                   |
         [Control API :9090]
                   |
         [Frontend :3000]
```

## Phase 4+: Namespace 모드

```
┌─────────────────────────────────────────────────────────────┐
│                        HOST NAMESPACE                        │
│                                                             │
│   ┌─────────────┐    veth-c0          veth-s0    ┌───────┐  │
│   │  Control    │   10.10.0.1/30    10.20.0.1/30 │       │  │
│   │  :9090      │        |                |       │       │  │
│   │  (metrics,  │        |                |       │       │  │
│   │   orchestr) │        |                |       │       │  │
│   └─────────────┘   [veth pair]      [veth pair]  │       │  │
│                          |                |       │       │  │
└──────────────────────────|────────────────|───────┘       │  │
                           |                |                │
┌──────────────────────────┘                └──────────────┐ │
│          CLIENT NAMESPACE                  SERVER NS      │ │
│                                                           │ │
│   ┌─────────────┐  veth-c1             veth-s1           │ │
│   │  Generator  │  10.10.0.2/30        10.20.0.2/30      │ │
│   │  (HTTP/1,2) │                      ┌──────────┐      │ │
│   └─────────────┘                      │ Responder│      │ │
│                                        │ (HTTP/1,2│      │ │
└────────────────────────────────────────┘──────────┘──────┘
```

## IP 주소 배정

| 인터페이스 | 네임스페이스 | IP | 설명 |
|-----------|------------|-----|------|
| veth-c0   | host       | 10.10.0.1/30 | client 측 host end |
| veth-c1   | nm-client  | 10.10.0.2/30 | client NS end |
| veth-s0   | host       | 10.20.0.1/30 | server 측 host end |
| veth-s1   | nm-server  | 10.20.0.2/30 | server NS end |

## 트래픽 흐름

```
Generator (10.10.0.2) --[SYN]--> 10.20.0.2:8080 (Responder)
                      <--[SYN-ACK]--
                      --[ACK]-->
                      --[HTTP GET /]-->
                      <--[HTTP 200 OK]--
                      --[FIN]-->   (CPS 시험: connection close)
```

## 수동 설정 (테스트용)

```bash
# namespace 생성
sudo ip netns add nm-client
sudo ip netns add nm-server

# client 측 veth
sudo ip link add veth-c0 type veth peer name veth-c1
sudo ip link set veth-c1 netns nm-client
sudo ip addr add 10.10.0.1/30 dev veth-c0
sudo ip netns exec nm-client ip addr add 10.10.0.2/30 dev veth-c1
sudo ip link set veth-c0 up
sudo ip netns exec nm-client ip link set veth-c1 up
sudo ip netns exec nm-client ip link set lo up

# server 측 veth
sudo ip link add veth-s0 type veth peer name veth-s1
sudo ip link set veth-s1 netns nm-server
sudo ip addr add 10.20.0.1/30 dev veth-s0
sudo ip netns exec nm-server ip addr add 10.20.0.2/30 dev veth-s1
sudo ip link set veth-s0 up
sudo ip netns exec nm-server ip link set veth-s1 up
sudo ip netns exec nm-server ip link set lo up

# 라우팅 (client -> server 방향, host 경유)
sudo ip netns exec nm-client ip route add 10.20.0.0/30 via 10.10.0.1
sudo ip netns exec nm-server ip route add 10.10.0.0/30 via 10.20.0.1

# IP 포워딩 활성화
sudo sysctl -w net.ipv4.ip_forward=1

# 정리
sudo ip netns del nm-client
sudo ip netns del nm-server
sudo ip link del veth-c0   # peer도 자동 삭제
sudo ip link del veth-s0
```
