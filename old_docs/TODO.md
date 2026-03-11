# TODO

## 현재 상태

모든 계획된 Phase가 완료되었습니다.
현재는 신규 기능 추가보다 운영 품질과 문서 정합성 유지가 우선 과제입니다.

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
| VLAN External Port 모드 검증 | 기능 구현 완료, 실장비 검증 완료 ✅ |
| musl 빌드 검증 | 실제 musl 타깃 빌드 검증 완료 ✅ |
| aarch64 빌드 지원 | x86_64 우선 지원. aarch64 확장 미지원 |
| 성능 벤치마크 자동화 | 반복 시험 후 결과 비교 자동화 스크립트 |

---

## 현재 알려진 구조적 메모

- 프론트 프로파일 저장소는 브라우저 `localStorage`입니다.
- 서버 `/api/results`는 메모리 저장소이므로 프로세스 재시작 시 초기화됩니다.
- 네트워크 모드 선택은 `TestConfig`가 아니라 서버 시작 옵션(`--mode`)에 의해 결정되며, 시험 프로파일에는 `tcp_options`만 저장됩니다.
- 프론트 빌드는 단일 JS/CSS 번들 정책을 사용하며, 초기 로드 비용보다 배포 단순성을 우선합니다.
