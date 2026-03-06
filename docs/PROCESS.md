# 개발 진행 사항

## 전체 Phase 계획

| Phase | 내용 | 상태 |
|-------|------|------|
| 1 | 기초 스켈레톤: Rust 워크스페이스, 크레이트 구조, Control API 서버 | ✅ 완료 |
| 2 | Generator 고도화 + hdrhistogram Metrics | ✅ 완료 |
| 3 | Responder: hyper 1.0 직접 사용, 서버 사이드 메트릭 | ✅ 완료 |
| 4 | Namespace 관리: veth pair, setns, IP forwarding | ✅ 완료 |
| 5 | 부가 기능: SO_REUSEPORT, 정적 파일 서빙, TCP_QUICKACK 등 | ✅ 완료 |
| 6 | Frontend UI 고도화: 차트, 프로파일 편집기 | 미시작 |
| 7 | HTTP/2 지원: h2c / TLS h2 | 미시작 |
| 8 | TLS 지원: rustls + rcgen 자체 서명 인증서 | 미시작 |
| 9 | eBPF/XDP 옵션: aya 기반 패킷 계측 | 미시작 |

---

## Phase 1: 기초 스켈레톤 ✅

**목표:** 컴파일/실행 가능한 최소 구조 확립

**달성 사항:**
- [x] engine/ Rust 워크스페이스 구성 (6개 크레이트)
  - `core`: 공통 타입 (TestProfile, TestState, MetricsSnapshot, NetMeterError)
  - `metrics`: lock-free atomic 카운터 + 초당 집계 (Aggregator)
  - `ns`: 네트워크 네임스페이스 관리 스텁
  - `generator`: HTTP/1.1 트래픽 발생기 스텁
  - `responder`: 가상 HTTP 서버 스텁
  - `control`: REST API (axum 0.7) + 오케스트레이션 바이너리
- [x] frontend/ React + Vite + TypeScript 기본 구조
- [x] scripts/ 빌드/실행 스크립트 (setup.sh, build.sh)

**아키텍처 결정:**
- 비동기 런타임: `tokio full`
- Control API: `axum 0.7`
- Metrics: `std::sync::atomic` (lock-free, Relaxed ordering)
- 상태 공유: `Arc<RwLock<>>` (control path only)
- 로깅: `tracing` + `tracing-subscriber`
- 에러: `thiserror` (라이브러리 크레이트), `anyhow` (바이너리)

**Control API 엔드포인트:**
```
GET    /api/health           → {"status":"ok","version":"0.1.0"}
GET    /api/status           → TestStatus {state, profile, elapsed_secs}
POST   /api/test/start       → 시험 시작 (body: TestProfile)
POST   /api/test/stop        → 시험 중지
GET    /api/metrics          → MetricsSnapshot (최신)
GET    /api/metrics/ws       → WebSocket 실시간 스트림 (1초 간격)
GET    /api/profiles         → Vec<TestProfile>
POST   /api/profiles         → 프로파일 저장
DELETE /api/profiles/:id     → 프로파일 삭제
```

---

## Phase 2: Generator 고도화 + hdrhistogram Metrics ✅

**목표:** 정밀한 CPS 제어 및 실측 latency 계산

**달성 사항:**
- [x] `tokio::time::interval` + `MissedTickBehavior::Skip` 기반 정밀 rate 제어
  - CPS 100 목표 → 99.9998 달성 (오차 < 0.01%)
- [x] 세마포어 backpressure: `max_inflight = target_cps * 2` (기본)
- [x] connect timeout + response timeout (`tokio::time::timeout`)
- [x] TCP connect latency, TTFB, 전체 latency 독립 계측
- [x] `hdrhistogram`: p50/p95/p99 실제 계산 (`Mutex<Histogram<u64>>`)
- [x] CPS / CC / BW 시험 모드 구현
- [x] TestProfile 필드: `connect_timeout_ms`, `response_timeout_ms`, `max_inflight`
- [x] MetricsSnapshot 필드: `connect_mean/p99`, `ttfb_mean/p99`, `cps`, `rps`

**실측 결과 (CPS 100, 로컬호스트):**
- CPS: 99.9988 (목표 100)
- latency p50: 0.498ms, p95: 0.700ms, p99: 0.853ms
- connect p99: 0.277ms, TTFB p99: 0.474ms
- 실패율: 0%, 타임아웃: 0%

---

## Phase 3: Responder 고도화 ✅

**목표:** 오버헤드 최소화, 서버 사이드 계측

**달성 사항:**
- [x] `axum` → `hyper 1.0` 직접 사용 (http-body-util, bytes, hyper-util)
- [x] keep-alive 내장: `http1::Builder::new().keep_alive(true)`
- [x] 서버 사이드 계측: `server_requests`, `server_bytes_tx` atomic 카운터
- [x] `Responder::start()` (로컬 모드) / `start_in_ns()` (NS 모드) API 분리
- [x] configurable response body 크기 (`response_body_bytes`)

---

## Phase 4: Namespace 관리 ✅

**목표:** client/server 네임스페이스 자동 생성·정리, 격리 환경 시험

**달성 사항:**
- [x] `ip netns add/del` 명령 래퍼 (`manager.rs`)
- [x] veth pair 생성/설정 (`veth.rs`): `create_pair`, `move_to_ns`, `set_ip`, `bring_up` 등
- [x] NS 내 라우팅: `add_route_in_ns` (client NS → server NS, host 경유)
- [x] IP 포워딩 활성화: `sysctl -w net.ipv4.ip_forward=1`
- [x] teardown: `teardown()` 메서드 (시험 stop 시 NS·veth 자동 삭제)
- [x] 권한 체크: `check_capability()` (root 또는 CAP_NET_ADMIN)
- [x] `setns.rs`: `spawn_blocking` 내 `setns(2)` 기반 NS 귀속 소켓 생성
  - `bind_listener_in_ns()`: server NS 내에서 `std::net::TcpListener` 바인드
  - `create_socket_in_ns()`: client NS 내에서 `tokio::net::TcpSocket` 생성
  - 완료 후 반드시 호스트 NS 복구 (tokio 스레드 풀 오염 방지)
- [x] Generator NS 모드: `run_in_ns()` — client NS 진입 후 `current_thread` 런타임 구동
- [x] Orchestrator 통합: `use_namespace=true` 시 NS 생성 → 시험 → 자동 정리
- [x] TestProfile 필드: `use_namespace: bool`, `netns_prefix: String`

**네트워크 토폴로지 (NS 모드):**
```
[client NS: 10.10.0.2/30]                 [server NS: 10.20.0.2/30]
  veth-c1                                    veth-s1
      |  (veth pair)                              |  (veth pair)
  veth-c0 [host: 10.10.0.1/30] ── IP fwd ── veth-s0 [host: 10.20.0.1/30]
```

---

## Phase 5: 부가 기능 ✅

**목표:** 운용 편의성, 소켓 옵션, 페이로드 제어

**달성 사항:**
- [x] **SO_REUSEADDR + SO_REUSEPORT**: `socket2`로 구현 → 재시작 시 포트 충돌 없음
- [x] **정적 파일 서빙**: `tower-http ServeDir` — 프론트엔드 빌드 결과물을 `:9090/`에서 서빙
  - Vite `build.outDir`: `engine/crates/control/static/`
  - SPA fallback: 알 수 없는 경로 → `index.html`
  - `--web-dir` CLI 옵션, 생략 시 바이너리 옆 `static/` 자동 탐색
- [x] **TCP_QUICKACK**: Delayed ACK 비활성화 — accept 직후 소켓에 적용 (Linux only, `libc::setsockopt`)
- [x] **request body 전송**: `request_body_bytes` → `Content-Length` 헤더 + body 실제 전송
- [x] **URL 길이 조정**: `path_extra_bytes` → 쿼리 파라미터로 패딩 (`?x=aaa...`)
- [x] **serde default**: `use_namespace`, `netns_prefix`, `tcp_quickack`, `path_extra_bytes` 등 — JSON에 필드 누락 시 기본값 사용 (422 방지)
- [x] **TestProfile 필드 추가**: `num_clients`, `num_servers` (현재 1 고정, 다중 NS 확장 예정)
- [x] **`scripts/run.sh`**: 빌드 + 실행 통합 스크립트 (`--no-build`, `--skip-frontend`, `--release`, `--port`)

---

## Phase 6: Frontend UI 고도화 (예정)

**목표:** 실제 운용 가능한 계측 콘솔 수준 UI

**작업 계획:**
- [ ] 실시간 시계열 차트 (CPS, BW, latency p99)
- [ ] 프로파일 편집기 (모든 TestProfile 필드 설정 가능)
- [ ] namespace 모드 토글 UI
- [ ] 시험 결과 리포트 다운로드 (JSON / CSV)

---

## Phase 7: HTTP/2 지원 (예정)

**목표:** h2c (cleartext) 및 TLS h2 지원

**작업 계획:**
- [ ] Generator: `h2` 크레이트 기반 HTTP/2 클라이언트
- [ ] Responder: `h2` 서버 (multiplexing, stream 수 설정)
- [ ] TestProfile: `h2_max_concurrent_streams` 옵션
- [ ] h2c / TLS h2 경로 아키텍처 수준에서 분리

---

## Phase 8: TLS 지원 (예정)

**목표:** 자체 서명 인증서로 TLS 1.2/1.3 시험

**작업 계획:**
- [ ] `rustls` + `rcgen` 자체 서명 인증서 자동 생성
- [ ] Generator: TLS handshake latency 계측
- [ ] Responder: TLS accept
- [ ] TestProfile: `tls: bool`, `tls_version` 옵션

---

## Phase 9: eBPF/XDP 옵션 (예정)

**목표:** 커널 레벨 고성능 계측 (선택적 기능)

**작업 계획:**
- [ ] `aya` 크레이트 기반 eBPF 프로그램
- [ ] NIC ingress 패킷 카운팅
- [ ] TCP 흐름 식별, 지연 없는 드롭/필터링 실험
- [ ] eBPF 없이도 핵심 기능 완전 동작 보장

---

## 참고 문서

- `testbed/topology.md`: 네트워크 토폴로지 및 수동 설정 예시
- `CLAUDE.md`: 프로젝트 요구사항 전체
