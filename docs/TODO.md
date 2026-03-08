# TODO

## 현재 상태

모든 계획된 Phase가 완료되었습니다.

| Phase | 내용 | 상태 |
|-------|------|------|
| 1 | 기초 스켈레톤 | ✅ |
| 2 | Generator 고도화 + hdrhistogram | ✅ |
| 3 | Responder hyper 1.0 | ✅ |
| 4 | Namespace 관리 | ✅ |
| 5 | 부가 기능 (SO_REUSEPORT, 정적 서빙, TCP_QUICKACK) | ✅ |
| 6 | Frontend UI 고도화 | ✅ |
| 7 | HTTP/2 h2c 지원 | ✅ |
| Booster | TCP + 다중 Pair + TestConfig 전환 | ✅ |
| 8 | TLS: rustls + rcgen | ✅ |
| P1 | Thresholds + Ramp-up + SSE 이벤트 로그 + 상태코드 breakdown | ✅ |
| P2 | UI 개선: 사이드바, 결과 비교, 서버 RX 지표 | ✅ |
| P3 | 총 클라이언트 수 기반 워커 자동 배분 | ✅ |
| P4 | Ramp-down 지원 | ✅ |
| P5 | CC / BW 시험 동작 분리 | ✅ |
| 10 | Association 기반 설정 전환 + VLAN 지원 | ✅ |
| 11 | External Port Mode: 물리 NIC 2개 + DUT 연동 + 정책 라우팅 | ✅ |
| UI-R | Frontend UI 전면 리팩터: Tailwind CSS v4 + shadcn/ui + Dark/Light 모드 | ✅ |
| UI-1~4 | UI 개선 (레이아웃, 데이터, 버그, UX) | ✅ |
| veth-dut | 단일 머신 External Port 검증 테스트베드 | ✅ |
| TLS-ALPN | ALPN 기반 TLS h2 + 사용자 정의 SNI 서버 이름 | ✅ |

---

## 향후 고려 가능한 개선사항

아래 항목은 계획된 범위를 벗어난 선택적 개선 항목입니다.
필요 시 별도 Phase로 재검토합니다.

| 항목 | 비고 |
|------|------|
| eBPF/XDP 옵션 계측 | 취소됨. 필요 시 aya 크레이트로 재검토 |
| HTTP/2 TLS (h2) | ALPN 기반 TLS h2 구현 완료 |
| VLAN External Port 모드 검증 | 기능 구현 완료, 실장비 검증 필요 |
| musl 빌드 검증 | 아키텍처 차원 지원 예정, 실제 musl 타깃 빌드 미검증 |
| aarch64 빌드 지원 | x86_64 우선 지원. aarch64 확장 미검증 |
| 성능 벤치마크 자동화 | 반복 시험 후 결과 비교 자동화 스크립트 |
