# net-meter 시험 모드 정리

## 개요

시험은 두 축의 조합으로 정의된다.

| 축 | 선택지 |
|----|--------|
| **시험 유형** (TestType) | `cps` / `cc` / `bw` |
| **프로토콜** (Protocol) | `http1` / `http2` |

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

- Generator가 `tokio::time::interval` + `MissedTickBehavior::Skip`으로 목표 CPS를 제어한다.
- tick마다 신규 TCP 연결을 생성하고 요청 1건을 전송한 뒤 연결을 닫는다.
- `Semaphore(max_inflight)`로 동시 진행 중인 연결 수에 상한을 둔다 (backpressure).
  - 세마포어가 가득 찼으면 해당 tick을 스킵한다 (CPS 상한 유지, 대기 없음).

```
tick ─▶ acquire semaphore (non-blocking)
          ├─ full → skip this tick
          └─ ok  → spawn task
                    ├─ TCP connect
                    ├─ 요청 1건 전송 + 응답 수신
                    └─ 연결 종료 → semaphore 반환
```

**핵심 지표:**
- `connections_established / sec` → 실측 CPS
- `connections_failed` / `connections_timed_out` → 실패 분류
- `latency_p50/p95/p99` → 연결~응답 완료 시간 분포

**관련 파라미터:**
| 파라미터 | 기본값 | 설명 |
|----------|--------|------|
| `target_cps` | 100 | 초당 목표 연결 수 |
| `max_inflight` | `target_cps × 2` | 동시 진행 최대 연결 수 |
| `connect_timeout_ms` | 5000 | TCP 연결 타임아웃 |
| `response_timeout_ms` | 30000 | 응답 완료 타임아웃 |

---

### CC — Concurrent Connections

**목표:** 일정 수의 동시 연결을 유지하는 능력 측정

- `target_cc`개의 워커 태스크를 생성한다.
- 각 워커는 독립적으로 "연결 → 요청 → 응답 → (연결 재사용 또는 재연결)"을 무한 반복한다.
- 시험 종료 시 모든 워커를 abort한다.

```
워커 × target_cc
  └─ loop:
       ├─ TCP connect
       ├─ 요청 전송 + 응답 수신  (keep-alive 활용)
       └─ (연결 닫히면 재연결)
```

**핵심 지표:**
- `active_connections` → 실측 동시 연결 수
- `rps` (responses/sec) → 유지 상태에서의 처리 속도
- `latency` → 동시성 하에서의 응답 시간

**관련 파라미터:**
| 파라미터 | 기본값 | 설명 |
|----------|--------|------|
| `target_cc` | 100 | 목표 동시 연결 수 |
| `connect_timeout_ms` | 5000 | 재연결 타임아웃 |
| `response_timeout_ms` | 30000 | 개별 요청 타임아웃 |

---

### BW — Bandwidth

**목표:** 최대 처리 대역폭 측정

- CC 모드와 구조가 유사하나, 더 큰 body 크기와 많은 동시 연결로 대역폭 포화를 유도한다.
- `target_cc`를 동시 연결 수(또는 연결 수)로 사용한다.
- HTTP/2에서는 연결당 다중 스트림으로 추가 멀티플렉싱을 활용한다.

**핵심 지표:**
- `bytes_tx_per_sec` / `bytes_rx_per_sec` → 처리 대역폭 (Bps)
- `rps` → 단위 시간당 응답 수
- `server_bytes_tx` → 서버 사이드 전송량 (응답 body)

**관련 파라미터:**
| 파라미터 | 기본값 | 설명 |
|----------|--------|------|
| `target_cc` | 50 (HTTP/1.1) / 10 (HTTP/2) | 동시 연결 수 |
| `request_body_bytes` | none | 요청 body 크기 |
| `response_body_bytes` | none | 응답 body 크기 |
| `h2_max_concurrent_streams` | 10 | (HTTP/2 전용) 연결당 동시 스트림 수 |

---

## 프로토콜 (Protocol)

### HTTP/1.1 (`http1`)

- Generator: `tokio::net::TcpStream`에 HTTP/1.1 텍스트 프로토콜 직접 구현 (zero-dep).
- Responder: `hyper 1.x` `http1::Builder::new().keep_alive(true)`.
- Keep-alive: CC/BW 모드에서 기존 연결을 재사용 (Connection: keep-alive).
- CPS 모드에서는 매 요청마다 `Connection: close`로 새 연결 사용.

```
[HTTP/1.1 CPS]
Client ──connect──▶ Server
       ──GET / HTTP/1.1\r\n...Connection: close\r\n──▶
       ◀──HTTP/1.1 200 OK\r\n...──
       ──close──

[HTTP/1.1 CC/BW]
Client ──connect──▶ Server
       ──GET / HTTP/1.1\r\n...Connection: keep-alive\r\n──▶
       ◀──HTTP/1.1 200 OK\r\n...──
       ──GET / HTTP/1.1\r\n...──▶  (재사용)
       ◀──HTTP/1.1 200 OK\r\n...──
       ...
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

### HTTP/1.1 × CPS

```
매 tick:
  새 TCP 연결
  → GET /path HTTP/1.1 (Connection: close)
  → 응답 수신
  → TCP 종료
```
- 연결 = TCP 연결 = 계측 단위

### HTTP/1.1 × CC

```
워커 × target_cc:
  TCP 연결
  loop:
    GET /path HTTP/1.1 (keep-alive)
    응답 수신
    [연결 끊기면 재연결]
```
- `active_connections` ≈ `target_cc`

### HTTP/1.1 × BW

- CC와 동일한 구조, body 크기를 크게 설정해 대역폭 포화 유도.

---

### HTTP/2 × CPS

```
매 tick:
  새 TCP 연결
  → HTTP/2 handshake
  → HEADERS (요청) + END_STREAM
  → 응답 수신
  → TCP 종료
```
- 연결 = TCP+h2 연결 = 계측 단위
- h2 핸드셰이크 오버헤드가 있으므로 HTTP/1.1 CPS보다 단위 비용이 높다.

### HTTP/2 × CC

```
워커 × target_cc:
  TCP + h2 handshake
  loop:
    sr = send_req.clone().ready().await   ← 스트림 용량 대기
    HEADERS + (DATA) → 응답 수신
    [연결 오류 시 재연결]
```
- 연결당 1개 스트림을 순차 실행.
- `active_connections` ≈ `target_cc` (각 워커 = 1 h2 연결).

### HTTP/2 × BW

```
워커 × target_cc:
  TCP + h2 handshake

  스트림 워커 × h2_max_concurrent_streams:  ← SendRequest::clone() 공유
    loop:
      HEADERS → 응답 수신
      [연결 오류 → 외부에서 재연결]
```
- 총 동시 스트림 수 = `target_cc × h2_max_concurrent_streams`.
- 하나의 TCP 연결 위에서 여러 스트림이 동시에 진행 (멀티플렉싱).

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

### 대역폭 지표

| 필드 | 설명 |
|------|------|
| `bytes_tx_total` | 총 송신 바이트 (요청 헤더 + body) |
| `bytes_rx_total` | 총 수신 바이트 (응답 헤더 + body) |
| `bytes_tx_per_sec` | 초당 송신 (직전 스냅샷과의 차이, Aggregator 계산) |
| `bytes_rx_per_sec` | 초당 수신 (직전 스냅샷과의 차이, Aggregator 계산) |

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
| `connect_mean/p99_ms` | TCP connect (+ h2 handshake) latency |
| `ttfb_mean/p99_ms` | 요청 전송 후 → 첫 응답 바이트 수신까지 |
| `latency_histogram` | 누적 버킷 (0.5 / 1 / 2 / 5 / 10 / 25 / 50 / 100 / 250 / 500 ms / +Inf) |

### 서버 사이드 지표 — Responder가 기록

| 필드 | 설명 |
|------|------|
| `server_requests` | Responder가 처리한 요청 수 |
| `server_bytes_tx` | Responder가 전송한 응답 body 바이트 |

---

## 네트워크 모드

### 로컬 모드 (`use_namespace: false`, 기본)

```
Generator ──localhost──▶ Responder
(호스트 네임스페이스)      (호스트 네임스페이스)
```
- 별도 권한 불필요.
- `target_host: "127.0.0.1"`, `target_port`에 서버 자동 바인드.

### Namespace 모드 (`use_namespace: true`)

```
[client NS: 10.10.0.2/30]                    [server NS: 10.20.0.2/30]
  veth-c1                                       veth-s1
      |  (veth pair)                                 |  (veth pair)
  veth-c0 [host: 10.10.0.1/30] ── IP fwd ── veth-s0 [host: 10.20.0.1/30]
```
- `CAP_NET_ADMIN` (root) 필요.
- Generator는 `spawn_blocking` + `setns(2)` + `current_thread` 런타임으로 client NS에서 실행.
- Responder는 `spawn_blocking` + `setns(2)`로 server NS에서 소켓을 바인드 후 호스트 NS로 복구.
- 시험 종료 시 namespace/veth 자동 정리.

---

## TestProfile 파라미터 요약

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `test_type` | `cps`/`cc`/`bw` | — | 시험 유형 |
| `protocol` | `http1`/`http2` | `http1` | HTTP 프로토콜 버전 |
| `target_host` | String | `"127.0.0.1"` | 목표 서버 주소 |
| `target_port` | u16 | 8080 | 목표 서버 포트 |
| `duration_secs` | u64 | 60 | 시험 시간 (0=수동 중지) |
| `target_cps` | u64? | 100 | [CPS] 초당 목표 연결 수 |
| `target_cc` | u64? | — | [CC/BW] 목표 동시 연결 수 |
| `max_inflight` | u64? | cps×2 | [CPS] 최대 동시 in-flight 연결 |
| `request_body_bytes` | usize? | none | 요청 body 크기 |
| `response_body_bytes` | usize? | none | 응답 body 크기 |
| `method` | `GET`/`POST` | `GET` | HTTP 메서드 |
| `path` | String | `"/"` | 요청 경로 |
| `path_extra_bytes` | usize? | none | URL 쿼리 패딩 (`?x=aaa...`) |
| `connect_timeout_ms` | u64? | 5000 | TCP 연결 타임아웃 |
| `response_timeout_ms` | u64? | 30000 | 응답 타임아웃 |
| `h2_max_concurrent_streams` | u32? | 10 | [HTTP/2 BW] 연결당 동시 스트림 |
| `use_namespace` | bool | false | 네임스페이스 격리 모드 |
| `netns_prefix` | String | `"nm"` | NS 이름 prefix |
| `tcp_quickack` | bool | false | TCP_QUICKACK (Delayed ACK 비활성화) |
| `num_clients` | u32 | 1 | 가상 클라이언트 수 (현재 1 고정) |
| `num_servers` | u32 | 1 | 가상 서버 수 (현재 1 고정) |
