# 개발 진행 사항

## 전체 Phase 계획

| Phase | 내용 | 상태 |
|-------|------|------|
| 1 | 기초 스켈레톤: Rust 워크스페이스, 크레이트 구조, Control API 서버 | ✅ 완료 |
| 2 | Generator 고도화 + hdrhistogram Metrics | ✅ 완료 |
| 3 | Responder: hyper 1.0 직접 사용, 서버 사이드 메트릭 | ✅ 완료 |
| 4 | Namespace 관리: veth pair, setns, IP forwarding | ✅ 완료 |
| 5 | 부가 기능: SO_REUSEPORT, 정적 파일 서빙, TCP_QUICKACK 등 | ✅ 완료 |
| 6 | Frontend UI 고도화: 차트, 프로파일 편집기 | ✅ 완료 |
| 7 | HTTP/2 지원: h2c / TLS h2 | ✅ 완료 (h2c) |
| Booster | TCP 프로토콜, 다중 Pair 토폴로지, TestConfig 전면 전환 | ✅ 완료 |
| 8 | TLS 지원: rustls + rcgen 자체 서명 인증서 | ✅ 완료 |
| 9 | ~~eBPF/XDP 옵션~~ | 취소 |
| 10 | Association 기반 설정 전환 + VLAN 지원 | 설계 완료, 미구현 |
| 11 | External Port Mode: 물리 NIC 2개 연동, DUT 시험 | 설계 완료, 미구현 |

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

## Phase 6: Frontend UI 고도화 ✅

**목표:** 실제 운용 가능한 계측 콘솔 수준 UI (Avalanche 참고)

**달성 사항:**
- [x] **헤더 글로벌 컨트롤**: elapsed/remaining 시간 + progress bar + Stop 버튼 항상 노출
- [x] **4탭 내비게이션**: Dashboard / Topology / Profiles / Results
- [x] **TestControl Accordion 재편**: Basic / Load / HTTP / Timing / Network 섹션, 모든 TestProfile 필드 노출
- [x] **Profile import/export**: JSON 파일로 저장/불러오기
- [x] **대시보드 목표값 vs 실측값**: TargetCard (달성률 progress bar 포함)
- [x] **차트 개선**: 목표선(ReferenceLine), Active Connections(Area), BW(Stacked Area), Latency 시계열
- [x] **Latency Histogram 차트**: BarChart (구간별 카운트 + p50/p95/p99 표시)
- [x] **Error Breakdown 패널**: 연결 실패/타임아웃/4xx/5xx 상세 분류
- [x] **Topology 뷰**: Client NS ↔ Host ↔ Server NS 다이어그램 + 실시간 지표 오버레이
- [x] **Results 탭**: 시험 결과 목록, 상세 펼침, JSON/CSV 다운로드
- [x] **백엔드 API 확장**:
  - `MetricsSnapshot`에 `latency_histogram: Vec<HistogramBucket>` 추가 (누적 버킷, 11개)
  - `elapsed_secs` 실제 구현 (`test_start_time` 추적)
  - `GET /api/results`, `DELETE /api/results/:id` 신규
  - 시험 종료 시 `TestResult` 자동 저장 (최대 50개)

---

## Phase 7: HTTP/2 h2c 지원 ✅

**목표:** h2c (cleartext HTTP/2, Prior Knowledge) 지원

**달성 사항:**
- [x] `engine/Cargo.toml`: `h2 = "0.4"`, `http = "1"` 추가, `hyper` features에 `http2` 추가
- [x] `core::TestProfile`: `h2_max_concurrent_streams: Option<u32>` 필드 추가
- [x] **Generator `http2.rs`** (신규 크레이트 모듈):
  - h2c Prior Knowledge 연결: `h2::client::Builder::new().handshake()`
  - CPS 모드: 초당 신규 h2c 연결 + 1 스트림 + 세마포어 backpressure
  - CC 모드: `target_cc`개 동시 h2c 연결, 각 연결에서 순차 스트림 반복
  - BW 모드: `target_cc` 연결 × `h2_max_concurrent_streams` 동시 스트림 (multiplexing)
  - `SendRequest::clone()` + `ready().await` 패턴으로 안전한 스트림 다중화
  - TCP connect latency, TTFB, 전체 latency 독립 계측
  - h2 flow control: `release_capacity()` 자동 처리
- [x] **Generator `lib.rs`**: `protocol == Http2` 시 `http2::run()` 라우팅 (local & NS 모드 모두)
- [x] **Responder `lib.rs`** (HTTP/2 서버):
  - `start()` / `start_in_ns()`에 `protocol: Protocol` 파라미터 추가
  - `hyper::server::conn::http2::Builder::new(TokioExecutor::new())` 기반 h2c 서버
  - 기존 HTTP/1.1 (`http1::Builder`) 및 HTTP/2 (`http2::Builder`) 프로토콜 분기
  - 동일한 `handle_request` 서비스 함수 재사용 (hyper body 타입 호환)
- [x] **Orchestrator**: `profile.protocol` → responder에 전달
- [x] **Frontend**:
  - `TestProfile` 타입에 `h2_max_concurrent_streams?: number` 추가
  - HTTP 섹션: `protocol === 'http2'` 시 "Max Concurrent Streams" 필드 표시

**아키텍처 결정:**
- h2c (cleartext HTTP/2 Prior Knowledge): TLS 없이 HTTP/2 직접 사용
- TLS h2는 Phase 8 (rustls + rcgen)에서 처리
- Generator: `h2` 0.4 crate (저수준 API, 세밀한 스트림 제어)
- Responder: `hyper` 1.x `http2::Builder` (서비스 함수 재사용, 심플)
- `SendRequest::clone()`으로 동일 TCP 연결에서 다중 스트림 워커 공유

---

## Booster Phase: TCP 프로토콜 + 다중 Pair 토폴로지 + TestConfig 전면 전환 ✅

**목표:** 단일 HTTP 대상 → 다중 Pair (TCP+HTTP 혼합) 지원, 프로토콜 독립적 계측

**달성 사항:**

### 1. TestProfile → TestConfig 전면 전환
- [x] **`TestConfig`** (구 TestProfile) 신규 구조 설계:
  - `pairs: Vec<PairConfig>` — 다중 클라이언트/서버 쌍
  - `default_load: LoadConfig` — pairs 공통 기본 부하 설정
  - `ns_config: NsConfig` — 네임스페이스 설정 분리
- [x] **`PairConfig`**: `id`, `protocol`, `client: ClientEndpoint`, `server: ServerEndpoint`, `payload: PayloadProfile`, `load: Option<LoadConfig>`
- [x] **`PayloadProfile`** enum (`#[serde(tag="type")]`):
  - `Tcp(TcpPayload)` — `client_tx_bytes`, `server_tx_bytes`
  - `Http(HttpPayload)` — `method`, `path`, `request_body_bytes`, `response_body_bytes`, `h2_max_concurrent_streams`
- [x] **`LoadConfig`**: `target_cps`, `target_cc`, `max_inflight`, `connect_timeout_ms`, `response_timeout_ms`
- [x] **`Protocol`**: `Http1` | `Http2` | `Tcp` 추가
- [x] `Display for Protocol` → `core/src/config.rs` (orphan rule 준수)

### 2. TCP 프로토콜 지원
- [x] **`responder/src/tcp.rs`** (신규): TCP accept loop + per-connection 핸들러
  - client_tx bytes 수신 → server_tx bytes 응답 (ping-pong) 반복
  - 서버 사이드 메트릭 기록 (global + proto dual collector)
- [x] **Generator**: TCP CPS (신규 연결 ping-pong), CC/BW (스트리밍) 구현
- [x] 멀티 Responder: `handles: Vec<JoinHandle<()>>`, `stop_all()` API

### 3. 다중 Pair 네트워크 토폴로지
- [x] **NS 토폴로지 개편 (/24 서브넷 + IP 앨리어싱)**:
  - client NS: `10.10.1.1/24` (veth-c1)
  - host: `veth-c0(10.10.1.254/24)`, `veth-s0(10.20.1.254/24)`
  - server NS: `10.20.1.1/24` + 추가 서버별 앨리어스 (`10.20.1.N/24`)
- [x] **`assign_pair_addrs()`**: pair 순서대로 서버 IP 할당, 추가 IP는 `ip addr add` 앨리어스
- [x] **로컬 모드**: server_id 중복 체크로 동일 포트 이중 바인드 방지
- [x] **Dual Collector 패턴**: 각 pair worker가 `global: Arc<Collector>` + `proto: Arc<Collector>` 양쪽에 기록
- [x] **`MultiAggregator`**: 전체 rate 집계 + `by_protocol: HashMap<String, PerProtocolSnapshot>`

### 4. Frontend 전면 개편
- [x] **`api/client.ts`**: `TestConfig`, `PairConfig`, `PayloadProfile`, `LoadConfig`, `PerProtocolSnapshot` 신규 타입
- [x] **`TestControl.tsx`** (완전 재작성): Pairs 테이블 + PairDialog 모달 편집기
  - Protocol 선택에 따라 TCP/HTTP 페이로드 폼 전환
  - 클라이언트/서버 엔드포인트, 선택적 per-pair 부하 오버라이드
- [x] **`ProfileManager.tsx`**: TestConfig 기반 저장 프로파일 관리
- [x] **`Results.tsx`**, **`TopologyView.tsx`**, **`MetricsPanel.tsx`**: `profile.*` → `config.*` 필드 경로 업데이트

### 트러블슈팅
- E0117 orphan rule: `Display for Protocol` → `core/src/config.rs`로 이동
- E0521 borrow escape: `let pair_id = pair.id.clone()` before `tokio::spawn`
- E0063 missing field: `MetricsSnapshot { by_protocol: HashMap::new(), ... }`
- TS6133 unused `setField`: `PairDialog`에서 제거

**검증 결과:**
```
cargo check → Finished `dev` profile in 0.08s
npm run build → ✓ built in 2.16s
```

---

## P1 개선: 임계값/알람 + Ramp-up + 이벤트 로그 + 상태코드 Breakdown ✅

**목표:** 운용 수준 모니터링 기능 완성

**달성 사항:**

### 1. 임계값 / 알람 설정
- [x] **`Thresholds`** 구조체 추가 (`core/src/config.rs`):
  - `min_cps`, `max_error_rate_pct`, `max_latency_p99_ms`, `auto_stop_on_fail`
- [x] **`TestConfig.thresholds`** 필드 추가 (`#[serde(default)]`)
- [x] 1초 집계 루프(`main.rs`)에서 임계값 체크 → `MetricsSnapshot.threshold_violations` 설정
- [x] `auto_stop_on_fail=true`이면 엔진 자동 중단
- [x] **프론트엔드**: TestControl에 Thresholds 섹션 추가 (min_cps, max_error_rate, max_latency_p99, auto_stop 체크박스)
- [x] MetricsPanel 상단에 위반 배너 (빨간 테두리, 위반 항목 목록)
- [x] App.tsx 헤더에 ⚠ 알람 배지 (위반 시 점등)

### 2. Ramp-up 제어
- [x] **`LoadConfig.ramp_up_secs: u64`** 추가 (기본값 0)
- [x] **`TestState::RampingUp`** 추가 (보라색 배지)
- [x] **Orchestrator** `transition_to_running()`:
  - `ramp_up_secs > 0`이면 `RampingUp` 상태로 시작, 이후 `Running`으로 전환
  - `duration_secs` 타이머는 `RampingUp`도 포함하여 카운트
- [x] **Generator CPS 루프**: 토큰 버킷 방식으로 선형 증가
  - `token_acc += scale.min(1.0)` → 누적 ≥ 1이면 연결 허용
  - HTTP/1.1, HTTP/2, TCP 모두 적용
- [x] **프론트엔드**: Default Load 섹션에 "Ramp-up" 입력 추가
- [x] MetricsPanel 상단 Ramp-up 진행 배너 (보라색)
- [x] `StateBadge`에 `ramping_up` 색상 추가

### 3. 실시간 이벤트 로그 패널 (SSE)
- [x] **`control/src/event.rs`** (신규): `TestEvent` enum 정의
  - `TestStarted`, `TestStopped`, `RampUpStarted`, `RampUpComplete`, `NsSetupComplete`, `NsTeardownComplete`, `ThresholdViolation`, `Error`
- [x] **`AppState.event_tx: broadcast::Sender<TestEvent>`** 추가
- [x] **`GET /api/events/stream`** SSE 엔드포인트 (`api/events.rs`) — `async_stream` 사용
- [x] Orchestrator에서 이벤트 발행 (시험 시작/중지, NS 준비/정리, Ramp-up 단계)
- [x] **프론트엔드**: `EventLog.tsx` 신규 컴포넌트
  - 최근 100개 항목, 레벨별 색상 (info/warn/error)
  - Dashboard 하단에 배치 (이벤트가 있을 때만 표시)
  - "Clear" 버튼

### 4. 상태코드 상세 Breakdown
- [x] **`Collector.status_code_breakdown: Mutex<HashMap<u16, u64>>`** 추가
- [x] `record_response()`에서 per-code 집계 (`status > 0`인 경우만)
- [x] `MetricsSnapshot.status_code_breakdown: HashMap<u16, u64>` 추가
- [x] **프론트엔드**: `StatusCodeTable` 컴포넌트 — 코드별 응답 수 그리드 표시

**검증 결과:**
```
cargo check → Finished `dev` profile in 0.08s
npm run build → ✓ built in 2.11s
```

---

## Phase 8: TLS 지원 ✅

**목표:** 자체 서명 인증서로 TLS 1.2/1.3 시험

**달성 사항:**
- [x] `rustls 0.23` + `rcgen 0.13` + `tokio-rustls 0.26` 워크스페이스 의존성 추가
- [x] `PairConfig.tls: bool` 필드 추가 (`#[serde(default)]`)
- [x] `control/src/tls.rs` (신규): rcgen 자체 서명 인증서 생성 + `TlsBundle { server_config, client_config }`
  - `NoCertVerifier`: 클라이언트 인증서 검증 비활성화 (IP 주소 직접 연결 허용)
- [x] Responder: TLS accept 지원
  - `start_server`, `start_server_in_ns`에 `tls_config: Option<Arc<ServerConfig>>` 파라미터 추가
  - `TlsAcceptor::accept()` 후 `hyper` HTTP/1.1 / HTTP/2 서비스 연결
  - `serve_http<I>` 제너릭 함수로 평문/TLS 통합 처리
- [x] Generator HTTP/1.1 TLS 지원 (`http1.rs`):
  - `run()`, 내부 함수에 `tls: Option<Arc<ClientConfig>>` 파라미터 추가
  - CPS 모드: TCP connect → TLS handshake → 제너릭 `send_and_receive<S: AsyncReadExt + AsyncWriteExt + Unpin>`
  - CC/BW 모드: TCP connect → TLS handshake → `tokio::io::split` → `DynReader/DynWriter` (Box<dyn>) → `do_keepalive_request`
- [x] Generator HTTP/2 TLS 지원 (`http2.rs`):
  - `connect_h2()` 함수: TLS 유무에 따라 `h2::client::Builder::new().handshake(tls_stream or tcp)` 분기
  - `SendRequest<Bytes>` 타입 통합 (Connection은 task spawn)
- [x] Orchestrator: TLS pair 있으면 `tls::build()` 호출, server/client config 분리 전달
- [x] Frontend: PairDialog에 TLS 체크박스 추가 (HTTP 프로토콜에만 표시)
- [x] `api/client.ts`: `PairConfig.tls?: boolean` 추가

**아키텍처 결정:**
- 인증서: rcgen 자체 서명, SAN `localhost` (NoCertVerifier로 IP 연결도 허용)
- 클라이언트: `NoCertVerifier` — 인증서 검증 없음 (시험 도구 특성)
- ServerName: `"localhost"` 고정 (SNI 전용, 검증과 무관)
- TLS handshake latency: connect_latency에 포함 (TCP + TLS 합산)
- DynReader/DynWriter: `Box<dyn AsyncRead/AsyncWrite + Unpin + Send>` — keep-alive TLS 지원

**검증:**
```
cargo check → Finished `dev` profile in 0.12s
npm run build → ✓ built in 2.48s
```

---

## Phase 10: Association 기반 설정 전환 + VLAN 지원 (설계 완료)

**목표:** Avalanche 스타일 "클라이언트 수 기반 설정", IP 대역 지정, VLAN 단일/이중 태그 지원

상세 설계: `docs/design-next.md`

**핵심 변경:**
- `PairConfig` → `Association` (ClientNet IP 대역 + VlanConfig 추가)
- `TestConfig.total_clients: u32` 추가 (associations 간 균등 분배)
- `LoadConfig`: `target_cps/cc` (시스템 전체 기준) → `cps_per_client / cc_per_client` (per-client 기준)
- `NsConfig` → `NetworkConfig { mode: NetworkMode, ns: NsOptions, ext: Option<ExternalPortOptions> }`
- `VlanConfig { outer_vid, inner_vid, outer_proto: VlanProto }` — single/QinQ 모두 지원
- `ns/src/veth.rs`: `assign_client_ips()`, `add_vlan_subif()`, `add_qinq_subif()` 추가
- Generator: per-client IP bind 소켓 (`bind(src_ip, 0)`)

**VLAN 구현 방식 (Linux):**
```bash
# single tag
ip link add link veth-c1 name veth-c1.100 type vlan id 100 proto 802.1Q
# double tag (QinQ)
ip link add link veth-c1 name veth-c1.100 type vlan id 100 proto 802.1ad
ip link add link veth-c1.100 name veth-c1.100.200 type vlan id 200 proto 802.1Q
```
커널 모듈 `8021q` 필요.

---

## Phase 11: External Port Mode (설계 완료)

**목표:** 물리 NIC 2개(inbound/outbound 포트)를 사용, 외부 DUT를 경유하는 실제 시험

상세 설계: `docs/design-next.md`

**트래픽 흐름:**
```
[net-meter Generator] --(eth1: client_iface)--> [외부 DUT] --(eth2: server_iface)--> [net-meter Responder]
```

**핵심 변경:**
- `NetworkMode::ExternalPort` 추가
- `ExternalPortOptions { client_iface, server_iface, client_gateway, client_gateway_mac, ... }`
- `engine/crates/ns/src/port.rs` (신규): NIC IP 할당, VLAN subif, static ARP entry
- Orchestrator: ExternalPort 분기 (NS 생성/삭제 스킵, port.rs 셋업/정리)
- Generator: `bind(client_ip, 0)`, 필요 시 `SO_BINDTODEVICE`
- Responder: `bind(server_ip, port)` (0.0.0.0 대신 특정 IP)
- Frontend: External Port 설정 폼 + Topology 뷰 DUT 다이어그램

---

## 참고 문서

- `testbed/topology.md`: 네트워크 토폴로지 및 수동 설정 예시
- `CLAUDE.md`: 프로젝트 요구사항 전체
- `docs/MODE.md`: 시험 모드(CPS/CC/BW × HTTP/1.1/HTTP/2) 동작 및 계측 지표 정리
- `docs/TODO.md`: 잔여 작업 목록 (P1/P2/Phase7+)
