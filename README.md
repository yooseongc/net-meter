# net-meter

네트워크 성능 계측기 / 트래픽 시험기.
Avalanche와 유사한 구조로 CPS · BW · CC 시험을 수행하며, Linux 네트워크 네임스페이스로 격리된 가상 Client/Server 환경을 제공합니다.

## 주요 기능

- **CPS (Connections Per Second)** — 초당 신규 연결 수 측정, latency 분포(p50/p95/p99)
- **BW (Bandwidth)** — 대역폭 최대치 측정, goodput / retransmission 지표
- **CC (Concurrent Connections)** — 목표 동시 연결 수 유지, 메모리 footprint 관찰
- **HTTP/1.1** — keep-alive, GET/POST, request/response body 크기 설정, URL 길이 조정
- **네트워크 네임스페이스** — client NS / server NS 자동 생성·정리, veth pair + IP forwarding
- **실시간 대시보드** — React 기반 UI, WebSocket 스트림, 시계열 차트
- **TCP 소켓 옵션** — Delayed ACK 제어(TCP_QUICKACK), SO_REUSEPORT

## 아키텍처

```
┌─────────────────────────────────────────────┐
│                   HOST NS                    │
│                                             │
│  net-meter (Control + Generator + Responder) │
│  ├─ Control API   :9090  (REST + WebSocket)  │
│  ├─ React UI      :9090/                     │
│  ├─ veth-c0  10.10.0.1/30                   │
│  └─ veth-s0  10.20.0.1/30                   │
│       │                    │                 │
└───────│────────────────────│─────────────────┘
        │                    │
┌───────┘          ┌─────────┘
│  CLIENT NS       │  SERVER NS
│  veth-c1         │  veth-s1
│  10.10.0.2/30    │  10.20.0.2/30
│  [Generator]     │  [Responder]
└──────────────────┘
```

### 크레이트 구조

```
engine/crates/
  core/       공통 타입: TestProfile, MetricsSnapshot, NetMeterError
  metrics/    lock-free atomic 카운터 + hdrhistogram (p50/p95/p99)
  ns/         네임스페이스 관리: veth pair, setns, IP forwarding
  generator/  HTTP/1.1 트래픽 발생기 (CPS/CC/BW)
  responder/  hyper 1.0 기반 가상 HTTP 서버
  control/    REST API (axum) + 오케스트레이션 바이너리
```

## 요구사항

| 항목 | 버전 |
|------|------|
| Linux | 5.x 이상 (WSL2 포함) |
| Rust | stable (1.75+) |
| Node.js | 18+ |
| 권한 | namespace 모드는 root 또는 CAP_NET_ADMIN 필요 |

## 빠른 시작

```bash
# 1. 의존성 설치 (최초 1회)
./scripts/setup.sh

# 2. 빌드 + 실행 (프론트 빌드 → 엔진 빌드 → 서버 시작)
./scripts/run.sh

# 브라우저: http://localhost:9090
```

### 옵션

```bash
./scripts/run.sh --port 8080          # 포트 변경
./scripts/run.sh --skip-frontend      # 프론트 재빌드 생략
./scripts/run.sh --no-build           # 빌드 없이 바로 실행
./scripts/run.sh --release            # 최적화 빌드
sudo ./scripts/run.sh                 # namespace 모드 시험 (root 필요)
```

### 개발 모드 (hot reload)

```bash
# 터미널 1 - 백엔드
cd engine && cargo run --bin net-meter -- --port 9090

# 터미널 2 - 프론트엔드
cd frontend && npm run dev   # → http://localhost:3000
```

## API

| Method | Path | 설명 |
|--------|------|------|
| GET | `/api/health` | 헬스체크 |
| GET | `/api/status` | 현재 시험 상태 |
| POST | `/api/test/start` | 시험 시작 |
| POST | `/api/test/stop` | 시험 중지 |
| GET | `/api/metrics` | 최신 MetricsSnapshot |
| GET | `/api/metrics/ws` | WebSocket 실시간 스트림 |
| GET | `/api/profiles` | 저장된 프로파일 목록 |
| POST | `/api/profiles` | 프로파일 저장 |
| DELETE | `/api/profiles/:id` | 프로파일 삭제 |

### TestProfile 예시

```json
{
  "id": "uuid-v4",
  "name": "CPS-100 로컬",
  "test_type": "cps",
  "protocol": "http1",
  "target_host": "127.0.0.1",
  "target_port": 8080,
  "duration_secs": 30,
  "target_cps": 100,
  "method": "GET",
  "path": "/",
  "response_body_bytes": 1024,
  "request_body_bytes": 0,
  "path_extra_bytes": 0,
  "tcp_quickack": false,
  "use_namespace": false
}
```

### namespace 모드 예시

```json
{
  "test_type": "cps",
  "target_host": "10.20.0.2",
  "target_port": 8080,
  "use_namespace": true,
  "netns_prefix": "nm"
}
```

> `use_namespace: true`이면 `nm-client` / `nm-server` 네임스페이스를 자동 생성하고,
> 시험 종료 후 자동으로 정리합니다.

## 측정 지표

```
connections_attempted / established / failed / timed_out
active_connections
requests_total / responses_total
status_2xx / 4xx / 5xx
bytes_tx_total / bytes_rx_total
cps / rps / bytes_tx_per_sec / bytes_rx_per_sec
latency_mean/p50/p95/p99/max (ms)
connect_mean/p99 (ms)
ttfb_mean/p99 (ms)
server_requests / server_bytes_tx
```

## 개발 로드맵

| Phase | 내용 | 상태 |
|-------|------|------|
| 1 | 기초 스켈레톤: 워크스페이스, Control API | ✅ |
| 2 | Generator 고도화 + hdrhistogram | ✅ |
| 3 | Responder hyper 1.0 + 서버 사이드 메트릭 | ✅ |
| 4 | Namespace 관리: veth, setns, IP forwarding | ✅ |
| 5 | BW/CC 시험 고도화 | 예정 |
| 6 | Frontend 차트 개선, 프로파일 UI | 예정 |
| 7 | HTTP/2 지원 | 예정 |
| 8 | TLS (rustls + rcgen 자체 서명) | 예정 |
| 9 | eBPF/XDP 옵션 계측 | 예정 |

## 라이선스

MIT
