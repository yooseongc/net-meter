# net-meter 시험 모드 정리

## 개요

시험은 두 축의 조합으로 정의된다.

| 축 | 선택지 |
|----|--------|
| **시험 유형** (TestType) | `cps` / `cc` / `bw` |
| **프로토콜** (Protocol) | `tcp` / `http1` / `http2` |

모든 시험은 동일한 네트워크 구조 위에서 실행된다.

```
[Generator (Client)] ──TCP──▶ [Responder (Server)]
        │                               │
        └─── Collector ◀── 계측 ──────-┘
                │
           Aggregator (1초 간격 rate 계산)
                │
          MetricsSnapshot ──▶ API / WebSocket
```

---

## 시험 유형 (TestType)

### CPS — Connections Per Second

**목표:** 초당 신규 연결 생성 능력 측정

- Generator가 tight loop으로 목표 연결 수를 제어한다.
- 각 루프에서 신규 TCP 연결을 생성하고 요청 1건을 전송한 뒤 연결을 닫는다.
- `num_connections`개의 병렬 루프(워커)가 동시 동작한다.

```
워커 × num_connections:
  loop:
    TCP connect
    요청 1건 전송 + 응답 수신
    연결 종료
    (즉시 다음 반복)
```

**핵심 지표:**
- `connections_established / sec` → 실측 CPS
- `connections_failed` / `connections_timed_out` → 실패 분류
- `latency_p50/p95/p99` → 연결~응답 완료 시간 분포

---

### CC — Concurrent Connections

**목표:** 일정 수의 동시 연결을 유지하는 능력 측정

- `num_connections`개의 워커 태스크를 생성한다.
- 각 워커는 "연결 → 요청 → 응답 → 짧은 idle → (연결 재사용 또는 재연결)"을 무한 반복한다.
- 연결 유지 중심: idle 구간을 두어 점유형 동시 연결 시뮬레이션.

```
워커 × num_connections:
  loop:
    TCP connect
    요청 전송 + 응답 수신  (keep-alive 활용)
    짧은 idle
    [연결 끊기면 재연결]
```

**핵심 지표:**
- `active_connections` → 실측 동시 연결 수
- `rps` (responses/sec) → 유지 상태에서의 처리 속도
- `latency` → 동시성 하에서의 응답 시간

---

### BW — Bandwidth

**목표:** 최대 처리 대역폭 측정

- CC 모드와 구조가 유사하나, idle 없이 최대 속도로 반복한다.
- `num_connections`을 동시 연결 수로 사용한다.
- HTTP/2에서는 연결당 `h2_max_concurrent_streams`까지 다중 스트림으로 추가 멀티플렉싱.

```
워커 × num_connections:
  loop:
    요청 전송 + 응답 수신  (no delay)
    [연결 끊기면 재연결]
```

**핵심 지표:**
- `bytes_tx_per_sec` / `bytes_rx_per_sec` → 처리 대역폭 (Bps)
- `rps` → 단위 시간당 응답 수
- `server_bytes_tx` → 서버 사이드 전송량 (응답 body)

---

## 프로토콜 (Protocol)

### TCP (`tcp`)

- Generator: CPS 모드에서 ping-pong (연결 → tx_bytes 전송 → rx_bytes 수신 → 종료).
- CC/BW 모드: 연결 유지 후 지속 스트리밍.
- Responder: `responder/src/tcp.rs` — accept loop + per-connection 핸들러.

```
[TCP CPS]
Client ──connect──▶ Server
       ──tx_bytes──▶
       ◀──rx_bytes──
       ──close──

[TCP CC/BW]
Client ──connect──▶ Server
       loop:
         ──tx_bytes──▶
         ◀──rx_bytes──
```

---

### HTTP/1.1 (`http1`)

- Generator: `tokio::net::TcpStream`에 HTTP/1.1 텍스트 프로토콜 직접 구현 (zero-dep).
- Responder: `hyper 1.x` `http1::Builder::new().keep_alive(true)`.
- CPS 모드: 매 요청마다 `Connection: close`로 새 연결 사용.
- CC/BW 모드: `Connection: keep-alive` 연결 재사용.

```
[HTTP/1.1 CPS]
Client ──connect──▶ Server
       ──GET / HTTP/1.1\r\nConnection: close\r\n──▶
       ◀──HTTP/1.1 200 OK\r\n──
       ──close──

[HTTP/1.1 CC/BW]
Client ──connect──▶ Server
       loop:
         ──GET / HTTP/1.1\r\nConnection: keep-alive\r\n──▶
         ◀──HTTP/1.1 200 OK\r\n──
```

---

### HTTP/2 h2c (`http2`)

- h2c (cleartext HTTP/2): TLS 없이 HTTP/2 Prior Knowledge로 연결.
- Generator: `h2` 0.4 크레이트. `h2::client::Builder::new().handshake(stream)`으로 핸드셰이크.
- Responder: `hyper 1.x` `http2::Builder::new(TokioExecutor::new())`.
- 멀티플렉싱: `SendRequest::clone()`으로 같은 TCP 연결에서 여러 스트림 워커가 동시 요청.
- Flow control: 응답 body 수신 후 `recv_body.flow_control().release_capacity()` 자동 처리.

```
[HTTP/2 h2c 연결 시퀀스]
Client ──TCP connect──▶ Server
       ──PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n (preface)──▶
       ◀──SETTINGS frame──
       ──SETTINGS ACK──▶
       [연결 수립 완료]

[스트림 (요청/응답)]
       ──HEADERS frame (요청)──▶
       ◀──HEADERS frame (응답 status)──
       ◀──DATA frame(s) (응답 body)──
       ◀──END_STREAM──
```

---

## 프로토콜 × 시험 유형 조합

| | TCP | HTTP/1.1 | HTTP/2 |
|---|---|---|---|
| **CPS** | 연결→ping-pong→종료 반복 | 연결→GET close→종료 반복 | 연결→h2 handshake→1스트림→종료 반복 |
| **CC** | 연결 유지 + idle | 연결 유지 keep-alive + idle | h2 연결 유지, 순차 스트림 |
| **BW** | 연결 유지 + 최대 스트리밍 | 연결 유지 keep-alive + 최대속도 | h2 연결 × `h2_max_concurrent_streams` 스트림 |

---

## 계측 지표 (MetricsSnapshot)

### 연결 지표

| 필드 | 설명 | 갱신 시점 |
|------|------|-----------|
| `connections_attempted` | 연결 시도 누적 | TCP connect 직전 |
| `connections_established` | 연결 성공 누적 | TCP connect 성공 후 |
| `connections_failed` | 연결 실패 누적 | connect 오류 또는 h2 handshake 실패 |
| `connections_timed_out` | 타임아웃 누적 | connect/response 타임아웃 발생 |
| `active_connections` | 현재 활성 연결 수 | established +1 / closed -1 |

### 요청/응답 지표

| 필드 | 설명 |
|------|------|
| `requests_total` | 송신 요청 누적 (bytes_tx 포함) |
| `responses_total` | 수신 응답 누적 |
| `status_2xx/4xx/5xx/other` | HTTP 상태 코드별 응답 수 |
| `status_code_breakdown` | per-code 응답 수 HashMap |

### 대역폭 지표

| 필드 | 설명 |
|------|------|
| `bytes_tx_total` | 총 송신 바이트 (요청 헤더 + body) |
| `bytes_rx_total` | 총 수신 바이트 (응답 헤더 + body) |
| `bytes_tx_per_sec` | 초당 송신 (Aggregator 계산) |
| `bytes_rx_per_sec` | 초당 수신 (Aggregator 계산) |

### 율(Rate) 지표 — Aggregator가 1초 간격으로 계산

| 필드 | 계산식 |
|------|--------|
| `cps` | `Δconnections_established / Δt` |
| `rps` | `Δresponses_total / Δt` |
| `bytes_tx_per_sec` | `Δbytes_tx_total / Δt` |
| `bytes_rx_per_sec` | `Δbytes_rx_total / Δt` |

### Latency 지표 — hdrhistogram (1µs ~ 60s, 3 significant digits)

| 필드 | 설명 |
|------|------|
| `latency_mean/p50/p95/p99/max_ms` | 연결 시작 ~ 응답 완료 전체 latency |
| `connect_mean/p99_ms` | TCP connect (+ h2 handshake + TLS) latency |
| `ttfb_mean/p99_ms` | 요청 전송 후 → 첫 응답 바이트 수신까지 |
| `latency_histogram` | 누적 버킷 (0.5 / 1 / 2 / 5 / 10 / 25 / 50 / 100 / 250 / 500 ms / +Inf) |

### 서버 사이드 지표 — Responder가 기록

| 필드 | 설명 |
|------|------|
| `server_requests` | Responder가 처리한 요청 수 |
| `server_bytes_tx` | Responder가 전송한 응답 body 바이트 |
| `server_bytes_rx` | Responder가 수신한 요청 body 바이트 |

### 프로토콜별 분리 집계

| 필드 | 설명 |
|------|------|
| `by_protocol` | `HashMap<String, PerProtocolSnapshot>` — tcp/http1/http2 분리 |

---

## 네트워크 모드

### Loopback 모드 (`mode: "loopback"`, 기본, 권한 불필요)

```
Generator ──localhost──▶ Responder
(호스트 네임스페이스)      (호스트 네임스페이스)
```

- 별도 권한 불필요.
- clients[].cidr의 IP가 로컬에 바인딩 가능한 주소여야 함 (127.x.x.x 권장).

### Namespace 모드 (`mode: "namespace"`, CAP_NET_ADMIN 필요)

```
[client NS]                   [host: ip_forward=1]              [server NS]
  10.255.1.2/30 ←─link─→  veth-c0: 10.255.1.1/30            10.255.2.2/30
  client CIDRs              veth-s0: 10.255.2.1/30  ─link─→  server IPs (/32)
  default gw: 10.255.1.1                                        default gw: 10.255.2.1
```

- `CAP_NET_ADMIN` (root) 필요.
- Generator는 `spawn_blocking` + `setns(2)` + `current_thread` 런타임으로 client NS에서 실행.
- Responder는 `spawn_blocking` + `setns(2)`로 server NS에서 소켓을 바인드 후 호스트 NS로 복구.
- 시험 종료 시 namespace/veth 자동 정리.

### External Port 모드 (`mode: "external_port"`, CAP_NET_ADMIN 필요)

```
[net-meter 단일 호스트]
  Generator                             Responder
  (client IPs bind)                     (server IPs bind)
       |                                     |
  upper_iface ─→ [외부 DUT] ─→  lower_iface
```

- `CAP_NET_ADMIN` (root) 필요.
- namespace 생성/삭제 불필요, IP forwarding 불필요.
- NIC에 client/server IP 직접 할당 (`ip addr add`).
- 정책 라우팅으로 DUT short-circuit 방지:
  - table 191: client CIDR → upper_iface (Generator → DUT 방향 강제)
  - table 192: server IP/32 → lower_iface (Responder → DUT 방향 강제)
- `ExternalPortOptions`: `client_iface`, `server_iface`, gateway IP/MAC, flush/cleanup 옵션.
- VLAN: 물리 NIC에 VLAN subif 생성 후 바인딩.

---

## TestConfig 파라미터 요약

### 최상위 (TestConfig)

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `test_type` | `cps`/`cc`/`bw` | — | 시험 유형 |
| `duration_secs` | u64 | 60 | 시험 시간 (0=수동 중지) |

### LoadConfig (default_load / per-association load 오버라이드)

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `num_connections` | u64? | 100 | 총 클라이언트/연결 수. Generator에서 워커 자동 배분 |
| `connect_timeout_ms` | u64? | 5000 | TCP 연결 타임아웃 |
| `response_timeout_ms` | u64? | 30000 | 응답 완료 타임아웃 |
| `ramp_up_secs` | u64 | 0 | 0=off, >0 → 선형 증가 (토큰 버킷) |
| `ramp_down_secs` | u64 | 0 | 0=off, >0 → 선형 감소 (종료 전) |

> **num_connections 의미:**
> - CPS: 총 병렬 루프 수 (동시 connect→transact→close 루프)
> - CC: 총 동시 연결 수 (전체 persistent connection 수)
> - BW: 총 동시 연결 수 (각 연결에서 최대 처리량 추구)

### ClientDef

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `id` | String | — | 식별자 |
| `cidr` | String | — | IP 대역 (e.g. "10.10.1.1/24") |
| `count` | u32? | 1 | 이 CIDR에서 사용할 IP 수 |

### ServerDef

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `id` | String | — | 식별자 |
| `ip` | String? | 자동할당 | 서버 IP |
| `port` | u16 | — | 리슨 포트 |
| `protocol` | `tcp`/`http1`/`http2` | `http1` | 프로토콜 |
| `tls` | bool | false | TLS 활성화 |

### Association

| 파라미터 | 타입 | 설명 |
|----------|------|------|
| `id` | String | 식별자 |
| `client_id` | String | ClientDef 참조 |
| `server_id` | String | ServerDef 참조 |
| `payload` | PayloadProfile | `Tcp(tx_bytes, rx_bytes)` 또는 `Http(method, path, ...)` |
| `load` | LoadConfig? | per-association 부하 오버라이드 |
| `vlan` | VlanConfig? | VLAN 단일/이중 태그 |

### PayloadProfile

```
# TCP
{ "type": "Tcp", "tx_bytes": 64, "rx_bytes": 64 }

# HTTP
{ "type": "Http", "method": "GET", "path": "/",
  "request_body_bytes": 0, "response_body_bytes": 1024,
  "path_extra_bytes": 0, "h2_max_concurrent_streams": 10 }
```

### NetworkConfig

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `mode` | `loopback`/`namespace`/`external_port` | `loopback` | 네트워크 모드 |
| `tcp_quickack` | bool | false | TCP_QUICKACK (Delayed ACK 비활성화) |
| `ns` | NsOptions | — | Namespace 모드 전용 (prefix 등) |
| `ext` | ExternalPortOptions? | None | External Port 모드 전용 |

### ExternalPortOptions

| 파라미터 | 타입 | 설명 |
|----------|------|------|
| `client_iface` | String | 클라이언트 측 NIC (e.g. "eth1") |
| `server_iface` | String | 서버 측 NIC (e.g. "eth2") |
| `client_gateway` | String? | DUT client 측 gateway IP (static ARP용) |
| `client_gateway_mac` | String? | DUT client 측 gateway MAC |
| `server_gateway` | String? | DUT server 측 gateway IP |
| `server_gateway_mac` | String? | DUT server 측 gateway MAC |
| `flush_iface_addrs` | bool | 시작 시 NIC 기존 IP 제거 여부 |
| `cleanup_addrs` | bool | 종료 시 할당 IP 제거 여부 (기본 true) |

### VlanConfig

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `outer_vid` | u16 | — | Outer VLAN ID (1~4094) |
| `inner_vid` | u16? | None | Inner VLAN ID (QinQ) |
| `outer_proto` | `dot1q`/`dot1ad` | `dot1q` | Outer EtherType |

### Thresholds (선택)

| 파라미터 | 타입 | 설명 |
|----------|------|------|
| `min_cps` | f64? | 최소 CPS 임계값 |
| `max_error_rate_pct` | f64? | 최대 오류율 (%) |
| `max_latency_p99_ms` | f64? | 최대 p99 latency (ms) |
| `auto_stop_on_fail` | bool | 임계값 위반 시 자동 중단 |
