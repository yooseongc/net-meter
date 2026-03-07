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

## 잔여 항목 — P2

### 5. 좌측 사이드바 내비게이션

- [ ] 현재 상단 탭 → 좌측 수직 사이드바로 전환
- [ ] 반응형: 1600px 이상에서 사이드바 + 메인 + 우측 요약 패널 3단 레이아웃

### 6. 시험 결과 비교 뷰

- [ ] Results 탭에서 두 결과 선택 → 나란히 비교
- [ ] 지표별 증감율 표시 (Δ CPS, Δ p99 latency 등)

### 7. 서버 사이드 지표 API 보강

- [ ] Collector에 `server_bytes_rx` AtomicU64 추가 (현재 미집계)
- [ ] Responder에서 수신 bytes 기록
- [ ] MetricsSnapshot에 노출
- [ ] Topology 뷰 Server 노드에 RX 표시

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

| Phase | 항목 |
|-------|------|
| 8 | TLS 지원: rustls + rcgen 자체 서명 인증서 |
| 9 | eBPF/XDP 옵션: aya 기반 패킷 계측 |

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
