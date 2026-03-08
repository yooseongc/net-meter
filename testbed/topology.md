# Testbed 네트워크 토폴로지

## 1. Loopback 모드 (기본, 권한 불필요)

```
[Generator Task]  ───TCP───▶ localhost:port [Responder Task]
        \                              /
         \                            /
      [Metrics Collector (atomic)]
                   │
         [Control API :9090]
                   │
         [Frontend :9090/]
```

- 별도 네트워크 설정 불필요.
- clients[].cidr은 127.x.x.x 대역 권장.

---

## 2. Namespace 모드 (CAP_NET_ADMIN 필요)

현재 구현: routed 토폴로지 (point-to-point 링크 + IP forwarding)

```
┌─────────────────────────────────────────────────────────────┐
│                        HOST NAMESPACE                        │
│                                                             │
│   ┌─────────────┐    veth-c0              veth-s0           │
│   │  Control    │   10.255.1.1/30       10.255.2.1/30       │
│   │  :9090      │        │                    │             │
│   │  (metrics,  │   [veth pair]          [veth pair]        │
│   │   orchestr) │        │                    │             │
│   └─────────────┘        │                    │             │
│                      ip_forward=1              │             │
└──────────────────────────│────────────────────│─────────────┘
                           │                    │
              ┌────────────┘                    └────────────┐
              │                                              │
┌─────────────┴──────────┐              ┌────────────────────┴──┐
│      CLIENT NAMESPACE   │              │     SERVER NAMESPACE   │
│                         │              │                        │
│  veth-c1                │              │  veth-s1               │
│  10.255.1.2/30          │              │  10.255.2.2/30         │
│  default gw: 10.255.1.1 │              │  default gw: 10.255.2.1│
│  client CIDRs (alias)   │              │  server IPs (/32)      │
│  [Generator]            │              │  [Responder]           │
└─────────────────────────┘              └────────────────────────┘
```

### IP 주소 배정

| 인터페이스 | 네임스페이스 | IP | 설명 |
|-----------|------------|-----|------|
| veth-c0   | host       | 10.255.1.1/30 | client 측 host end |
| veth-c1   | nm-client  | 10.255.1.2/30 | client NS link IP |
| veth-s0   | host       | 10.255.2.1/30 | server 측 host end |
| veth-s1   | nm-server  | 10.255.2.2/30 | server NS link IP |
| (alias)   | nm-client  | ClientDef.cidr | 클라이언트 IP 대역 |
| (alias)   | nm-server  | ServerDef.ip/32 | 서버 IP |

### 트래픽 흐름

```
Generator (client CIDR src) ──→ 10.255.1.1 (host/veth-c0) ──→ 10.255.2.2 (veth-s1) ──→ Responder
```

### 수동 설정 (참고용)

```bash
# namespace 생성
sudo ip netns add nm-client
sudo ip netns add nm-server

# client 측 veth
sudo ip link add veth-c0 type veth peer name veth-c1
sudo ip link set veth-c1 netns nm-client
sudo ip addr add 10.255.1.1/30 dev veth-c0
sudo ip netns exec nm-client ip addr add 10.255.1.2/30 dev veth-c1
sudo ip link set veth-c0 up
sudo ip netns exec nm-client ip link set veth-c1 up
sudo ip netns exec nm-client ip link set lo up
sudo ip netns exec nm-client ip route add default via 10.255.1.1

# server 측 veth
sudo ip link add veth-s0 type veth peer name veth-s1
sudo ip link set veth-s1 netns nm-server
sudo ip addr add 10.255.2.1/30 dev veth-s0
sudo ip netns exec nm-server ip addr add 10.255.2.2/30 dev veth-s1
sudo ip link set veth-s0 up
sudo ip netns exec nm-server ip link set veth-s1 up
sudo ip netns exec nm-server ip link set lo up
sudo ip netns exec nm-server ip route add default via 10.255.2.1

# IP 포워딩 활성화
sudo sysctl -w net.ipv4.ip_forward=1

# 정리
sudo ip netns del nm-client
sudo ip netns del nm-server
sudo ip link del veth-c0
sudo ip link del veth-s0
```

---

## 3. External Port 모드 (CAP_NET_ADMIN 필요)

물리 NIC 2개로 외부 DUT를 경유하는 실제 트래픽 시험.

```
[net-meter 단일 호스트]
┌──────────────────────────────────────────────────────────┐
│                                                          │
│  Generator                              Responder        │
│  (client IP bind)                       (server IP bind) │
│       │                                      │           │
│  upper_iface (client_iface)         lower_iface (server_iface)
│       │                                      │           │
└───────│──────────────────────────────────────│───────────┘
        │                                      │
        ▼                                      ▲
  ┌─────────────────────────────────────────────┐
  │              외부 DUT                        │
  │   (L2 스위치, 방화벽, 로드밸런서, 라우터 등) │
  └─────────────────────────────────────────────┘
```

### 정책 라우팅 (DUT short-circuit 방지)

net-meter 호스트에 upper/lower 인터페이스가 모두 있으면 커널이 DUT를 우회할 수 있다.
이를 방지하기 위해 정책 라우팅을 설정한다.

```
# table 191: Generator → DUT 방향 강제
ip rule add from <client_cidr> table 191
ip route add default dev <upper_iface> table 191

# table 192: Responder → DUT 방향 강제
ip rule add from <server_ip>/32 table 192
ip route add default dev <lower_iface> table 192
```

### 소켓 바인딩

- Generator: `bind(client_ip, 0)` — client IP를 소스로 고정
- Responder: `TcpListener::bind(server_ip:port)` — 특정 server IP에만 리슨

---

## 4. veth-dut 테스트베드 (단일 머신 External Port 검증)

단일 머신에서 External Port 모드를 검증하는 veth + bridge 토폴로지.
물리 DUT 없이도 정책 라우팅 동작을 검증할 수 있다.

```
[net-meter 단일 호스트]
┌────────────────────────────────────────────────────────┐
│                                                        │
│  Generator                         Responder           │
│  (upper=veth-c0)                   (lower=veth-s0)     │
│       │                                  │             │
│  veth-c0                            veth-s0            │
│       │                                  │             │
└───────│──────────────────────────────────│─────────────┘
        │                                  │
   veth-c1                             veth-s1
        │                                  │
        └──────────┐        ┌──────────────┘
                   │        │
              ┌────┴────────┴────┐
              │     br-dut       │
              │  (L2 bridge)     │
              │  DUT 시뮬레이션  │
              └──────────────────┘
```

### 특징

- Proxy ARP 불필요: ARP broadcast가 bridge를 통해 자연스럽게 전달됨
- 정책 라우팅으로 단락(short-circuit) 방지
- 단일 머신에서 External Port 모드 전체 스택 검증 가능

### 실행

```bash
# 토폴로지 생성 + 빌드 + net-meter 실행 (Ctrl+C 시 자동 정리)
sudo env PATH="$PATH" ./testbed/veth-dut/setup.sh

# 비상 정리 (setup.sh가 비정상 종료된 경우)
sudo ./testbed/veth-dut/teardown.sh
```

### 수동 설정 (참고용)

```bash
# veth-dut bridge 생성
sudo ip link add br-dut type bridge
sudo ip link set br-dut up

# upper side: veth-c0 (net-meter) ←→ veth-c1 (bridge)
sudo ip link add veth-c0 type veth peer name veth-c1
sudo ip link set veth-c1 master br-dut
sudo ip link set veth-c0 up
sudo ip link set veth-c1 up

# lower side: veth-s0 (net-meter) ←→ veth-s1 (bridge)
sudo ip link add veth-s0 type veth peer name veth-s1
sudo ip link set veth-s1 master br-dut
sudo ip link set veth-s0 up
sudo ip link set veth-s1 up

# 정리
sudo ip link del veth-c0
sudo ip link del veth-s0
sudo ip link del br-dut
```
