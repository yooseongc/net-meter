# 코드 개선 계획

코드 리뷰에서 발견된 개선점을 우선순위별로 정리한다.

---

## 높음 (버그 / 안정성 / 리소스)

### 1. Generator Helper 함수 3중 복제 ✅ 완료
**위치:** `generator/src/http1.rs`, `http2.rs`, `tcp.rs`

**문제:**
다음 6개 함수가 3개 파일에 **정확히 동일하게 복붙**되어 있다.
- `connect_tcp` — TCP 연결 수립 (src_ip 바인딩)
- `wait_deadline` — deadline까지 sleep
- `record_attempt` / `record_established` / `record_failed` / `record_timeout` / `record_response` — dual-collector 헬퍼

`resolve_tls_sni` (http1, http2)와 `build_path` (http1, http2)도 중복.

**해결:** `generator/src/common.rs` 신규 모듈로 추출.

---

### 2. 대용량 페이로드 버퍼 폭발 ✅ 완료
**위치:**
- `responder/lib.rs:234` — `Bytes::from(vec![b'x'; body_size])` 요청마다 반복 할당
- `generator/tcp.rs:248` — `vec![0u8; tx_bytes]` 송신 크기만큼 단일 할당
- `generator/tcp.rs:331` — `vec![0u8; rx_bytes.max(65536)]` 수신 크기만큼 할당
- `generator/http1.rs:343,611` — `vec![0u8; req_body_bytes]` 요청 body 매번 할당
- `generator/http2.rs:511` — `Bytes::from(vec![0u8; req_body_bytes])` 매번 할당

**문제:** 사용자가 `tx_bytes=1GB`, `response_body_bytes=1GB` 등을 설정하면 OOM 발생.
  요청마다 반복 할당되는 responder body는 GC 압력 증가.

**해결:**
- 송신: `write_zeroes()` — 정적 64KB 제로 버퍼를 청크 단위로 재사용
- 수신: 64KB 고정 버퍼로 루프 수신 (`rx_bytes.max(65536)` 제거)
- h2 송신: `Bytes::from_static(&ZERO_CHUNK[..n])` — 정적 버퍼 zero-copy 참조
- responder body: 서버 시작 시 `Arc<Bytes>` 1회 할당 → 모든 요청에서 clone (O(1))

---

### 3. Orchestrator 시작 실패 시 리소스 누수 ✅ 완료
**위치:** `control/src/orchestrator.rs`

**문제:** `start_local_mode` / `start_ns_mode` / `start_external_port_mode` 실패 시
`?`로 즉시 반환되어 이미 시작된 Responder 핸들이 정리되지 않고 남는다.
ExternalPort 모드에서는 정책 라우팅(policy routing)도 정리되지 않을 수 있다.

**해결:** 각 모드 실패 경로에서:
- `self.responder.stop_all()` 호출 (부분 시작된 리스너 정리)
- ExternalPort 모드: `teardown_policy_routing()` 추가 호출

---

### 10. NS 모드 setup 실패 시 롤백 없음 ✅ 완료
**위치:** `ns/src/manager.rs`

**문제:** `setup()` 도중 veth pair 생성 또는 NS 생성이 실패하면 `?`로 즉시 반환되어
이미 생성된 namespace / veth pair가 시스템에 남아 orphan 리소스가 된다.
다음 시험 시작 시 "already exists" 오류나 IP 충돌 가능성이 있다.

**해결:** `setup()` 실패 시 `cleanup_resources()` 자동 호출로 롤백.
- `setup_inner()` 내부 함수로 실제 setup 로직 분리
- `cleanup_resources()` — `ready` 상태 무관하게 link/ns 삭제 시도
- `teardown()` — `cleanup_resources()` 위임으로 코드 재사용

---

## 중간 (메트릭 정확성 / 안정성)

### 4. Histogram lock 실패 조용히 무시
**위치:** `metrics/src/collector.rs`

**문제:** `if let Ok(mut h) = self.latency_us.lock()` 패턴으로 lock poison 시
latency 메트릭이 기록되지 않고 조용히 손실됨.

**해결:** lock 실패 시 `warn!` 로그 추가. lock poison은 패닉이 발생한 경우이므로
최소한 경고 수준 로그가 있어야 디버깅 가능.

---

### 5. Shutdown 메커니즘 불일치
**위치:** `generator/src/http1.rs`, `http2.rs`, `tcp.rs`

**문제:**
- CPS 병렬 모드: `AtomicBool running` + `h.abort()`
- CC/BW 모드: `oneshot::Receiver shutdown` + `h.abort()`
- 두 메커니즘이 혼재하여 일관성 부족

**해결:** 통일된 shutdown 구조 검토.
현재 구조상 큰 문제는 없으나, 향후 graceful shutdown 구현 시 일관성 필요.

---

### 6. `server_map()` / `client_map()` 매 호출마다 clone
**위치:** `core/src/config.rs`

**문제:** 두 메서드가 호출될 때마다 전체 HashMap을 새로 생성.
Generator `start()` 와 Orchestrator에서 여러 번 호출될 경우 불필요한 복사.

**해결:** 참조 반환 또는 lazy 초기화 캐싱 고려.
현재 크기에서는 성능 영향 미미하나, associations/servers 수가 많아지면 문제.

---

## 낮음 (유지보수성 / 설정)

### 7. HTTP 헤더 파싱 수동 구현
**위치:** `generator/src/http1.rs`

**문제:** `do_keepalive_request()` 함수가 Content-Length, Connection 헤더를
라인 단위로 수동 파싱. 잘못된 응답 처리가 취약할 수 있음.

**개선 방향:** `http` 크레이트의 파서 활용 또는 현재 구조 유지 + fuzz 테스트.

---

### 8. 브로드캐스트 채널 크기 하드코딩
**위치:** `control/src/state.rs`

**문제:** `broadcast::channel(64)`, `broadcast::channel(256)` 고정.
수신자가 느리면 메시지 손실 (lagged 에러).

**해결:** 설정 가능하게 하거나, 채널 크기를 넉넉히 증가.
혹은 SSE 연결 수에 따라 동적 조정.

---

### 9. `parse_cidr()` 빈 문자열 폴백
**위치:** `core/src/config.rs`

**문제:** `split('/').next().unwrap_or("")`로 파싱 실패 시 빈 문자열 사용.
`"".parse::<Ipv4Addr>()` 실패 시 이후 로직에서 혼란 발생 가능.

**해결:** 명시적 `Err` 반환으로 조기 실패 처리.

---

## 구현 현황

| # | 이슈 | 상태 | 관련 파일 |
|---|------|------|-----------|
| 1 | Helper 함수 중복 제거 | ✅ 완료 | `generator/src/common.rs` (신규) |
| 2 | 대용량 버퍼 폭발 수정 | ✅ 완료 | `generator/src/{http1,http2,tcp}.rs`, `responder/src/lib.rs` |
| 3 | Orchestrator 실패 시 정리 | ✅ 완료 | `control/src/orchestrator.rs` |
| 4 | Histogram lock 무시 | 미구현 | `metrics/src/collector.rs` |
| 5 | Shutdown 메커니즘 통일 | 미구현 | `generator/src/` |
| 6 | server/client_map clone | 미구현 | `core/src/config.rs` |
| 7 | HTTP 헤더 파싱 개선 | 미구현 | `generator/src/http1.rs` |
| 8 | 채널 크기 하드코딩 | 미구현 | `control/src/state.rs` |
| 9 | parse_cidr 폴백 | 미구현 | `core/src/config.rs` |
| 10 | NS setup 롤백 | ✅ 완료 | `ns/src/manager.rs` |
