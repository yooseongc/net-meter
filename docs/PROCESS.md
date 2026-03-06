# 개발 진행 사항

## 전체 Phase 계획

| Phase | 내용 | 상태 |
|-------|------|------|
| 1 | 기초 스켈레톤: Rust 워크스페이스, 크레이트 구조, Control API 서버 | 완료 |
| 2+5 | Generator 고도화 + hdrhistogram Metrics | 완료 |
| 3 | Responder: 가상 HTTP/1.1 서버 고도화 | 완료 |
| 4 | Namespace 관리: veth pair, client/server NS 생성/정리 | 완료 |
| 6 | BW/CC 시험 검증 및 고도화 | 미시작 |
| 6 | BW 시험: 대역폭 최대치 측정 | 미시작 |
| 7 | CC 시험: 동시 연결 유지 측정 | 미시작 |
| 8 | Frontend: React 대시보드, 실시간 차트 | 미시작 |
| 9 | HTTP/2 지원: h2c / TLS h2 | 미시작 |
| 10 | TLS 지원: rustls + rcgen 자체 서명 인증서 | 미시작 |
| 11 | eBPF/XDP (옵션): aya 기반 패킷 계측 | 미시작 |

---

## Phase 1: 기초 스켈레톤

**목표:** 컴파일/실행 가능한 최소 구조 확립

**작업 내역:**
- [x] docs/PROCESS.md 작성
- [x] engine/ Rust 워크스페이스 구성
  - [x] `core` 크레이트: 공통 타입, 에러, 설정
  - [x] `metrics` 크레이트: 원자 카운터 기반 수집기
  - [x] `ns` 크레이트: 네트워크 네임스페이스 관리
  - [x] `generator` 크레이트: 클라이언트 트래픽 발생기 (Phase 1: 스텁)
  - [x] `responder` 크레이트: 가상 서버 (Phase 1: 기본 HTTP)
  - [x] `control` 크레이트: REST API 서버 (axum), 테스트 오케스트레이션
- [x] frontend/ React + Vite + TypeScript 기본 구조
- [x] scripts/ 빌드/실행 스크립트

**아키텍처 결정 사항:**
- 비동기 런타임: `tokio` (full features)
- Control API 서버: `axum 0.7`
- HTTP/1.1 Generator: `tokio::net::TcpStream` 직접 사용 (zero-copy, low-alloc)
- HTTP Responder: `axum` (Phase 1 단순화), 이후 `hyper` 직접 사용 고려
- Metrics: `std::sync::atomic` (lock-free), per-second aggregation
- Namespace: `tokio::process::Command`으로 `ip netns` 명령 실행
- 상태 공유: `Arc<RwLock<>>` (control path only, hot path는 atomic)
- 로깅: `tracing` + `tracing-subscriber`
- 에러: `thiserror` (라이브러리), `anyhow` (바이너리)

**Control API 엔드포인트 (Phase 1):**
```
GET  /api/health              -> {"status":"ok"}
GET  /api/status              -> TestStatus (state, profile, elapsed)
POST /api/test/start          -> start test (body: TestProfile)
POST /api/test/stop           -> stop test
GET  /api/metrics             -> MetricsSnapshot (latest)
GET  /api/metrics/ws          -> WebSocket 실시간 스트림 (1초 간격)
GET  /api/profiles            -> Vec<TestProfile>
POST /api/profiles            -> save profile
DELETE /api/profiles/:id      -> delete profile
```

**기본 네트워크 구성 (Phase 1: 로컬호스트 모드):**
```
Generator -> localhost:8080 -> Responder
(NS 없이 로컬호스트로 동작, NS는 Phase 4에서 활성화)
```

**Phase 4 이후 NS 구성:**
```
[client NS: 10.10.0.2/30]
  veth-c1
      |
  veth-c0 [host: 10.10.0.1/30]  <-- control binary
  veth-s0 [host: 10.20.0.1/30]
      |
  veth-s1
[server NS: 10.20.0.2/30]
```

---

## Phase 2+5: Generator 고도화 + hdrhistogram Metrics (완료)

**달성 사항:**
- [x] `tokio::time::interval` + `MissedTickBehavior::Skip` 기반 정밀 rate 제어
  - CPS 100 목표 → 99.9998 달성 (오차 < 0.01%)
- [x] 세마포어 backpressure: `max_inflight = target_cps * 2` (기본)
- [x] connect timeout + response timeout (tokio::time::timeout)
- [x] TCP connect latency, TTFB, 전체 latency 독립 계측
- [x] hdrhistogram: p50/p95/p99 실제 계산 (Mutex<Histogram<u64>>)
- [x] 새 프로파일 필드: connect_timeout_ms, response_timeout_ms, max_inflight
- [x] 새 스냅샷 필드: connections_timed_out, connect_mean/p99, ttfb_mean/p99

**실측 결과 (CPS 100, 로컬호스트):**
- CPS: 99.9988 (목표 100)
- latency p50: 0.498ms, p95: 0.700ms, p99: 0.853ms
- connect p99: 0.277ms, TTFB p99: 0.474ms
- 실패율: 0%, 타임아웃: 0%

---

## Phase 3+4: Responder 고도화 + Namespace 통합 (완료)

**Phase 3 달성 사항:**
- [x] hyper 1.0 직접 사용 (axum → hyper, http-body-util, bytes, hyper-util)
- [x] keep-alive 내장 (`http1::Builder::new().keep_alive(true)`)
- [x] 서버 사이드 계측: `server_requests`, `server_bytes_tx` 카운터
- [x] `Responder::start()` (로컬 모드) / `start_in_ns()` (NS 모드) 분리
- [x] 요청당 configurable body 크기 (`response_body_bytes`)

**Phase 4 달성 사항:**
- [x] `ip netns add/del` 명령 래퍼 (manager.rs)
- [x] veth pair 생성/설정 (veth.rs): create_pair, move_to_ns, set_ip, bring_up 등
- [x] NS 내 IP/route/lo 설정: veth.rs의 `add_route_in_ns`
- [x] IP 포워딩 활성화: `sysctl -w net.ipv4.ip_forward=1`
- [x] cleanup on drop: `teardown()` 메서드 (stop 시 NS 삭제)
- [x] 권한 체크: `check_capability()` (root 또는 CAP_NET_ADMIN)
- [x] `setns.rs`: spawn_blocking 내 `setns(2)` 기반 NS 귀속 소켓 생성
  - `bind_listener_in_ns()`: server NS에서 TcpListener 바인드
  - `create_socket_in_ns()`: client NS에서 TcpSocket 생성
- [x] Generator NS 모드: `run_in_ns()`로 client NS에 진입 후 current_thread 런타임 구동
  - 완료 시 반드시 호스트 NS 복구 (스레드 풀 오염 방지)
- [x] Orchestrator 통합: `use_namespace=true` 시 NS 생성 → 시험 → 자동 정리
- [x] `TestProfile`에 `use_namespace: bool`, `netns_prefix: String` 추가

**네트워크 라우팅 (NS 모드):**
```
client NS (10.10.0.2): route 10.20.0.0/30 via 10.10.0.1
host: IP forward enabled, 10.10.0.0/30 via veth-c0, 10.20.0.0/30 via veth-s0
server NS (10.20.0.2): route 10.10.0.0/30 via 10.20.0.1
```

**NS 모드 실행 예시:**
```bash
# root 권한 필요
sudo ./target/debug/net-meter --port 9090

# 시험 시작 (NS 모드)
curl -X POST http://localhost:9090/api/test/start \
  -H 'Content-Type: application/json' \
  -d '{"id":"...","name":"ns-test","test_type":"cps","protocol":"http1",
       "target_host":"10.20.0.2","target_port":8080,"duration_secs":10,
       "target_cps":100,"method":"GET","path":"/",
       "use_namespace":true,"netns_prefix":"nm"}'
```

---

## Phase 5: Metrics 고도화

**작업 계획:**
- [ ] `hdrhistogram`으로 latency percentile (p50/p95/p99) 구현
- [ ] per-second 시계열 저장 (RingBuffer)
- [ ] 시험 결과 전체 리포트 생성

---

## 참고 문서

- `docs/architecture.md`: 상세 아키텍처 다이어그램
- `testbed/topology.md`: 네트워크 토폴로지 예시
- `CLAUDE.md`: 프로젝트 요구사항 전체
