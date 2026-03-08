# TODO: Avalanche 수준 UI/기능 개선 계획

Spirent Avalanche 계측기를 참고하여 net-meter의 UI와 기능을 개선할 방안을 정리한다.

---

## 완료된 항목 (Phase 6 작업 중 처리)

| 항목 | 분류 | 비고 |
|------|------|------|
| 헤더 elapsed/remaining + Stop 버튼 상시 노출 | FE | ✅ |
| TestControl Accordion 섹션 + 모든 필드 노출 | FE | ✅ Basic/Load/HTTP/Timing/Network |
| Profile JSON import/export | FE | ✅ |
| 대시보드 목표값 vs 실측값 (TargetCard) | FE | ✅ 달성률 progress bar 포함 |
| 시계열 차트 목표선 (ReferenceLine) | FE | ✅ CPS, Active Conn |
| Active Connections Area 차트 | FE | ✅ |
| BW Stacked Area 차트 | FE | ✅ TX/RX 분리 |
| Latency 시계열 차트 (mean + p99) | FE | ✅ |
| Latency Histogram 차트 (BarChart) | FE+BE | ✅ 0.5ms~500ms+Inf 구간 |
| Error Breakdown 패널 | FE | ✅ 연결실패/타임아웃/4xx/5xx |
| Topology 뷰 (신규 탭) | FE | ✅ NS/로컬 다이어그램 + 실시간 지표 |
| Results 탭 (신규) | FE+BE | ✅ 목록/상세/JSON/CSV 다운로드 |
| MetricsSnapshot에 latency_histogram 추가 | BE | ✅ hdrhistogram 버킷 추출 |
| elapsed_secs 실제 구현 | BE | ✅ test_start_time 추적 |
| GET/DELETE /api/results | BE | ✅ 시험 완료 시 자동 저장 (최대 50개) |
| 4탭 내비게이션 (Dashboard/Topology/Profiles/Results) | FE | ✅ |

---

## P1 항목 (완료)

| 항목 | 상태 |
|------|------|
| 임계값/알람 설정 (`Thresholds`, auto_stop_on_fail) | ✅ |
| Ramp-up 제어 (`ramp_up_secs`, `RampingUp` 상태, 토큰 버킷) | ✅ |
| 실시간 이벤트 로그 패널 (SSE, `EventLog` 컴포넌트) | ✅ |
| 상태코드 상세 breakdown (`StatusCodeTable`) | ✅ |

---

## 잔여 항목 — P2 (완료)

### 5. 좌측 사이드바 내비게이션

- [x] 현재 상단 탭 → 좌측 수직 사이드바로 전환
- [x] 반응형: 1600px 이상에서 사이드바 + 메인 + 우측 EventLog 패널 3단 레이아웃

### 6. 시험 결과 비교 뷰

- [x] Results 탭에서 두 결과 선택 (체크박스, 최대 2개)
- [x] 나란히 비교 테이블 + 지표별 증감율 표시 (Δ, %)

### 7. 서버 사이드 지표 API 보강

- [x] Collector에 `server_bytes_rx` AtomicU64 추가
- [x] Responder TCP: 수신 bytes 루프마다 기록
- [x] Responder HTTP: Content-Length 기반 수신 bytes 기록
- [x] MetricsSnapshot에 `server_bytes_rx` 노출
- [x] Topology 뷰 Server 노드에 Srv RX 표시

---

## Booster Phase (완료)

| 항목 | 상태 |
|------|------|
| TCP 프로토콜 지원 (CPS ping-pong / CC+BW 스트리밍) | ✅ |
| TestProfile → TestConfig 전면 전환 (PairConfig, PayloadProfile enum) | ✅ |
| 다중 Pair 토폴로지 (multi-server IP aliasing /24 서브넷) | ✅ |
| Dual Collector 패턴 (global + per-protocol) | ✅ |
| MultiAggregator (by_protocol 분리 집계) | ✅ |
| Frontend: PairDialog 모달 편집기 + Pairs 테이블 | ✅ |

---

## 잔여 항목 — Phase 8+

| Phase | 항목 | 상태 |
|-------|------|------|
| 8 | TLS 지원: rustls + rcgen 자체 서명 인증서 | ✅ |
| 9 | ~~eBPF/XDP 옵션~~ | 취소 |
| 10 | Association 기반 설정 전환 + VLAN 지원 | ✅ |
| 11 | External Port Mode: 물리 NIC 2개 연동, DUT 시험 | ✅ |

---

## Phase 10: Association 기반 설정 + VLAN

### BE (core/ns/generator)
- [ ] `core/src/config.rs`: Association, ClientNet, VlanConfig, NetworkConfig/Mode 신규
- [ ] `core/src/config.rs`: LoadConfig per-client 전환 (cps_per_client, cc_per_client)
- [ ] `core/src/config.rs`: TestConfig.total_clients 추가, pairs → associations
- [ ] `ns/src/veth.rs`: `assign_client_ips()` (IP 대역 앨리어싱)
- [ ] `ns/src/veth.rs`: `add_vlan_subif()`, `add_qinq_subif()`
- [ ] `control/src/orchestrator.rs`: Association 기반 NS IP 셋업, VLAN subif 생성
- [ ] `generator/src/`: per-client IP bind 소켓 (`bind(src_ip, 0)`)
- [ ] `generator/src/`: cps_per_client / cc_per_client 기준 워커

### FE
- [ ] Association 편집 모달: ClientNet (base_ip, count, prefix_len)
- [ ] VLAN 설정 섹션: outer_vid, inner_vid, outer_proto
- [ ] LoadConfig 레이블 변경: "CPS per client", "CC per client"
- [ ] total_clients 입력 + association별 자동 분배 계산 표시
- [ ] Dashboard 예상 전체 부하 계산값 표시 (clients × per-client)

---

## Phase 11: External Port Mode

### BE
- [ ] `core/src/config.rs`: ExternalPortOptions, NetworkConfig.ext 필드
- [ ] `engine/crates/ns/src/port.rs` (신규): setup/teardown_external_port
  - NIC IP 할당 (flush 옵션)
  - VLAN subif 생성 (single/QinQ)
  - static ARP entry (ip neigh replace)
- [ ] `control/src/orchestrator.rs`: NetworkMode::ExternalPort 분기
- [ ] `generator/src/`: bind(src_ip, 0) + 필요 시 SO_BINDTODEVICE
- [ ] `responder/src/`: bind(server_ip, port) (특정 IP 리슨)

### FE
- [ ] NetworkConfig.mode 선택 UI (Loopback / Namespace / External Port)
- [ ] External Port 설정 폼 (client_iface, server_iface, gateway, MAC)
- [ ] Topology 뷰: External Port 다이어그램 (eth1 → DUT → eth2)

---

## Avalanche 대응표 (현재 상태)

| Avalanche UI | net-meter | 상태 |
|--------------|-----------|------|
| Port Group (Client / Server) | Topology 뷰 | ✅ |
| Test Scenario | TestProfile + test_type | ✅ |
| Real-time Counters | Dashboard StatCard (목표 vs 실측) | ✅ |
| Timeline Chart | CPS / Active Conn / BW / Latency | ✅ |
| Latency Distribution | Latency Histogram 차트 | ✅ |
| Load Ramp | Ramp-up 제어 | ✅ |
| Pass/Fail Verdict | 임계값 설정 + 자동중단 | ✅ |
| Event Log | 실시간 이벤트 로그 패널 | ✅ |
| Result Archive | Results 탭 | ✅ |
| Result Compare | 결과 비교 뷰 | ⬜ P2 |
| Protocol Config | HTTP 설정 섹션 | ✅ |
