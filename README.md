# net-meter

네트워크 성능 계측기 / 트래픽 시험기.
Avalanche와 유사한 구조로 CPS · BW · CC 시험을 수행하며, Linux 네트워크 네임스페이스 또는 물리 NIC 2개로 격리된 가상 Client/Server 환경을 제공합니다.

관련 문서:

- 진행상황 / 운영 메모: [docs/PROCESS.md](/home/yooseongc/net-meter/docs/PROCESS.md)
- 시험 모드 상세: [docs/MODE.md](/home/yooseongc/net-meter/docs/MODE.md)
- 스키마 기준 / 생성 산출물: [docs/SCHEMA.md](/home/yooseongc/net-meter/docs/SCHEMA.md)
- 과거 설계 / 이력: [old_docs](/home/yooseongc/net-meter/old_docs)

## 주요 기능

- **CPS (Connections Per Second)** — 초당 신규 연결 수 측정, latency 분포 (p50/p95/p99)
- **BW (Bandwidth)** — 대역폭 최대치 측정, goodput / retransmission 지표
- **CC (Concurrent Connections)** — 목표 동시 연결 수 유지, 메모리 footprint 관찰
- **TCP** — CPS: ping-pong, CC/BW: 스트리밍 모드
- **HTTP/1.1** — keep-alive, GET/POST, request/response body 크기 설정, URL 길이 조정
- **HTTP/2 (h2c)** — Prior Knowledge cleartext HTTP/2, 연결당 다중 스트림
- **TLS** — rustls 0.23 + rcgen 자체 서명 인증서 (HTTP/1.1, HTTP/2 공통)
- **VLAN** — 단일 태그 (802.1Q) / 이중 태그 QinQ (802.1ad) 지원
- **네트워크 네임스페이스** — client NS / server NS 자동 생성·정리, veth pair + IP forwarding
- **External Port 모드** — 물리 NIC 2개로 외부 DUT 연동, 정책 라우팅으로 short-circuit 방지
- **실시간 대시보드** — React 기반 UI, WebSocket 스트림, 시계열 차트
- **임계값 / 자동중단** — min_cps, max_error_rate, max_latency_p99, auto_stop_on_fail
- **Ramp-up / Ramp-down** — 선형 부하 증감, TestState::RampingUp/RampingDown

## 아키텍처

### 네트워크 모드 3종

**Loopback 모드 (기본, 권한 불필요)**
```
Generator ──localhost──▶ Responder
(호스트 네임스페이스)      (호스트 네임스페이스)
```

**Namespace 모드 (CAP_NET_ADMIN 필요)**
```
[client NS]                   [host: ip_forward=1]              [server NS]
  10.255.1.2/30 ←─link─→  veth-c0: 10.255.1.1/30            10.255.2.2/30
  client CIDRs              veth-s0: 10.255.2.1/30  ─link─→  server IPs (/32)
  default gw: 10.255.1.1                                        default gw: 10.255.2.1
```

**External Port 모드 (CAP_NET_ADMIN 필요)**
```
Generator (upper/client_iface)
  → [정책 라우팅: table 191]
  → upper_iface → [외부 DUT] → lower_iface
  → [정책 라우팅: table 192]
  → Responder (lower/server_iface)
```

### 크레이트 구조

```
engine/crates/
  core/       공통 타입: TestConfig, TestState, MetricsSnapshot, NetMeterError
  metrics/    lock-free atomic 카운터 + hdrhistogram (p50/p95/p99) + MultiAggregator
  ns/         네임스페이스 관리: veth pair, setns, IP forwarding, 정책 라우팅, External Port
  generator/  TCP/HTTP1/HTTP2 트래픽 발생기 (dual collector, per-client IP 바인딩, NS 모드)
  responder/  TCP/HTTP1/HTTP2 가상 서버 (hyper 1.0, TLS, NS 모드)
  control/    REST API (axum 0.7) + 오케스트레이션 바이너리
```

## 요구사항

| 항목 | 버전 |
|------|------|
| Linux | 5.x 이상 (WSL2 포함) |
| Rust | stable (1.75+) |
| Node.js | 18+ |
| 권한 | Namespace/External Port 모드는 root 또는 CAP_NET_ADMIN 필요 |

## 빠른 시작

```bash
# 1. 의존성 설치 (최초 1회)
./scripts/setup.sh

# 2. 빌드 + 실행 (프론트 빌드 → 엔진 빌드 → 서버 시작)
./scripts/run-dev.sh

# 브라우저: http://localhost:9090
```

### 옵션

```bash
./scripts/run-dev.sh --config config/loopback.runtime.yaml
./scripts/run-dev.sh --port 8080          # 포트 변경
./scripts/run-dev.sh --no-fe-build        # 프론트 재빌드 생략
./scripts/run-dev.sh --no-build           # 빌드 없이 바로 실행
sudo env PATH="$PATH" ./scripts/run-dev.sh --mode namespace  # Namespace 모드
```

`run-dev.sh`는 현재 debug 빌드 전용입니다. 릴리스 빌드는 `cargo build --release`와 `frontend npm run build`를 별도로 수행해야 합니다.
런타임 설정은 YAML 파일로 둘 수 있고, 같은 항목을 CLI 인자로 넘기면 YAML 값을 override 합니다. 예시는 [config](/home/yooseongc/net-meter/config) 디렉터리를 참조하세요.

### External Port 모드 (veth-dut 테스트베드)

단일 머신에서 External Port 모드를 검증하는 veth + bridge 토폴로지:

```
veth-c0 (upper) ←── veth-c1 ──┐
                                br-dut  (L2 bridge, DUT 시뮬레이션)
veth-s0 (lower) ←── veth-s1 ──┘
```

```bash
sudo env PATH="$PATH" ./testbed/veth-dut/setup.sh
# Ctrl+C 시 자동 정리
```

### 개발 모드 (hot reload)

```bash
# 터미널 1 - 백엔드
cd engine && cargo run --bin net-meter -- --port 9090

# 터미널 2 - 프론트엔드
cd frontend && npm run dev   # → http://localhost:3000
```

## CLI

웹 UI 없이 Control API를 조작하는 전용 CLI `net-meter-cli`를 제공합니다.

```bash
cd engine
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 health
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 status
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 start --file /path/to/test.json
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 monitor --watch-until-done
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 events
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 stop
```

`run` 서브커맨드는 시험 시작 후 모니터링까지 연속으로 수행합니다.

```bash
cd engine
cargo run --bin net-meter-cli -- --url http://127.0.0.1:9090 run --file /path/to/test.json --interval 1
```

`--json` 플래그를 주면 `health`, `status`, `metrics`, `results`, `monitor`, `events` 출력에 JSON 형식을 사용할 수 있습니다.

## API

| Method | Path | 설명 |
|--------|------|------|
| GET | `/api/health` | 헬스체크 |
| GET | `/api/status` | 현재 시험 상태 (state, config, elapsed_secs, runtime) |
| POST | `/api/test/start` | 시험 시작 (body: TestConfig JSON) |
| POST | `/api/test/stop` | 시험 중지 |
| GET | `/api/metrics` | 최신 MetricsSnapshot |
| GET | `/api/metrics/ws` | WebSocket 실시간 스트림 (1초 간격) |
| GET | `/api/results` | 완료된 시험 결과 목록 (최대 50개, 최신순) |
| DELETE | `/api/results/:id` | 결과 삭제 |
| GET | `/api/events/stream` | SSE 실시간 이벤트 로그 |

프로파일 관리는 브라우저 `localStorage`를 사용합니다. 서버 API로 저장되지 않으므로 브라우저를 바꾸면 프로파일이 공유되지 않습니다.
시험 설정 스키마의 기준은 백엔드 [config.rs](/home/yooseongc/net-meter/engine/crates/core/src/config.rs)이며, 프론트 import/localStorage도 현재 `TestConfig` 포맷만 지원합니다.

### TestConfig 예시

```json
{
  "id": "cfg-1",
  "name": "CPS 시험",
  "test_type": "cps",
  "duration_secs": 30,
  "tcp_options": { "tcp_quickack": false },
  "clients": [{ "id": "c1", "name": "client-1", "cidr": "127.0.0.1/8", "count": 10 }],
  "servers": [{ "id": "s1", "name": "server-1", "port": 8080, "protocol": "http1", "tls": false }],
  "associations": [{
    "id": "a1", "name": "assoc-1", "client_id": "c1", "server_id": "s1",
    "payload": { "type": "http", "method": "GET", "path": "/" }
  }],
  "default_load": { "num_connections": 100 },
  "thresholds": {}
}
```

### TestConfig 구조

```
TestConfig {
  id, name
  test_type: cps / cc / bw
  duration_secs: u64
  default_load: LoadConfig {
    num_connections       -- 총 클라이언트/연결 수 (워커 자동 배분)
    connect_timeout_ms
    response_timeout_ms
    ramp_up_secs          -- 0=off, >0 → 선형 증가
    ramp_down_secs        -- 0=off, >0 → 선형 감소
  }
  clients: [{ id, cidr, count? }]
  servers: [{ id, ip?, port, protocol, tls }]
  associations: [{
    id, name, client_id, server_id
    payload: Tcp(tx_bytes, rx_bytes) | Http(method, path, ...)
    load?: LoadConfig      -- per-association 오버라이드
    vlan?: VlanConfig      -- 단일/이중 태그
  }]
  tcp_options: {
    tcp_quickack: bool
  }
  thresholds?: {
    min_cps, max_error_rate_pct, max_latency_p99_ms, auto_stop_on_fail
  }
}
```

주의:

- 실제 네트워크 모드(`loopback` / `namespace` / `external_port`)는 `TestConfig`가 아니라 서버 기동 CLI 옵션 `--mode`로 결정됩니다.
- `/api/status` 응답에는 시험 프로파일(`config`)과 별도로 런타임 환경(`runtime.mode`, `runtime.upper_iface`, `runtime.lower_iface`)이 포함됩니다.

## 측정 지표

```
connections_attempted / established / failed / timed_out
active_connections
requests_total / responses_total
status_2xx / 4xx / 5xx / other
status_code_breakdown: {코드: 횟수}
bytes_tx_total / bytes_rx_total
cps / rps / bytes_tx_per_sec / bytes_rx_per_sec
latency_mean/p50/p95/p99/max (ms)
connect_mean/p99 (ms)
ttfb_mean/p99 (ms)
latency_histogram: 11개 버킷 (0.5~500ms + Inf)
server_requests / server_bytes_tx / server_bytes_rx
by_protocol: { "http1": {...}, "http2": {...}, "tcp": {...} }
threshold_violations: [위반 항목]
```

## 개발 로드맵

| Phase | 내용 | 상태 |
|-------|------|------|
| 1 | 기초 스켈레톤: Rust 워크스페이스, Control API 서버 | ✅ |
| 2 | Generator 고도화 + hdrhistogram | ✅ |
| 3 | Responder hyper 1.0 + 서버 사이드 메트릭 | ✅ |
| 4 | Namespace 관리: veth, setns, IP forwarding | ✅ |
| 5 | 부가 기능: SO_REUSEPORT, 정적 서빙, TCP_QUICKACK | ✅ |
| 6 | Frontend UI 고도화: 차트, 프로파일 편집기 | ✅ |
| 7 | HTTP/2 h2c 지원 | ✅ |
| Booster | TCP 프로토콜 + 다중 Pair + TestConfig 전환 | ✅ |
| 8 | TLS: rustls + rcgen 자체 서명 인증서 | ✅ |
| P1 | Thresholds + Ramp-up + SSE 이벤트 로그 + 상태코드 breakdown | ✅ |
| P2 | UI 개선: 사이드바, 결과 비교, 서버 RX 지표 | ✅ |
| P3~P5 | 총 클라이언트 수 기반 배분, Ramp-down, CC/BW 분리 | ✅ |
| 10 | Association 기반 설정 전환 + VLAN 지원 | ✅ |
| 11 | External Port Mode: 물리 NIC 2개 + DUT 연동 + 정책 라우팅 | ✅ |
| UI-R | Frontend UI 전면 리팩터: Tailwind CSS v4 + shadcn/ui + Dark/Light 모드 | ✅ |
| UI-1~4 | UI 개선 (레이아웃, 데이터, 버그, UX) | ✅ |
| veth-dut | 단일 머신 External Port 검증 테스트베드 | ✅ |
| TLS-ALPN | ALPN 기반 TLS h2 + 사용자 정의 SNI 서버 이름 | ✅ |

## 현재 구현 메모

- 프론트 정적 산출물은 [frontend/vite.config.ts](/home/yooseongc/net-meter/frontend/vite.config.ts) 기준으로 단일 JS/CSS 번들 정책을 사용합니다.
- 프로파일은 브라우저 `localStorage`에 저장됩니다.
- 결과 목록은 서버 메모리에만 유지되므로 프로세스 재시작 시 초기화됩니다.

## 라이선스

MIT
