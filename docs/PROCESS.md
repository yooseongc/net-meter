# PROCESS

현재 작업 진행상황과 운영 기준 메모를 기록한다.
과거 설계/이력 문서는 `old_docs/` 아래에 보관한다.

## 현재 상태

- 전체 핵심 기능은 구현 완료 상태다.
- 현재 우선순위는 신규 기능 추가보다 구조 명확화, 문서 정합성, 운영 품질 유지다.
- 프로파일 저장소는 백엔드가 아니라 브라우저 `localStorage`다.
- 시험 설정 스키마의 기준은 백엔드 `engine/crates/core/src/config.rs`다.
- 네트워크 모드는 시험 프로파일이 아니라 서버 실행 시 `--mode`로 결정된다.

## 완료된 범위

| 항목 | 상태 |
|------|------|
| Rust 워크스페이스 및 Control API 서버 | ✅ |
| Metrics collector + histogram 집계 | ✅ |
| TCP / HTTP/1.1 / HTTP/2 generator | ✅ |
| TCP / HTTP/1.1 / HTTP/2 responder | ✅ |
| TLS (rustls + rcgen) | ✅ |
| Thresholds / auto-stop / SSE 이벤트 로그 | ✅ |
| Ramp-up / Ramp-down | ✅ |
| Namespace 모드 | ✅ |
| External Port 모드 | ✅ |
| VLAN / QinQ | ✅ |
| React UI 리팩터 및 대시보드 | ✅ |
| 결과 목록 / 비교 UI | ✅ |
| veth-dut 테스트베드 | ✅ |

## 최근 정리 사항

- 프론트 빌드는 단일 JS/CSS 번들 정책으로 고정했다.
- 시험 프로파일과 서버 런타임 설정을 분리했다.
- 시험 프로파일 내 소켓 관련 필드는 `tcp_options`로 정리했다.
- `/api/profiles`는 실제 미사용이어서 제거했다.
- 프론트 API 타입은 Rust 스키마에서 생성된 `frontend/src/api/generated.ts`를 사용한다.
- Control API를 직접 다루는 전용 CLI `net-meter-cli`를 추가했다.
- 과거 설계 문서는 `old_docs/`로 이동했다.

## 현재 설정 경계

### 1. 런타임 설정

서버 시작 시 고정된다.

- `--mode`
- `--upper-iface`
- `--lower-iface`
- `--mtu`
- `--ns-prefix`

이 값들은 `/api/status`의 `runtime`으로 노출된다.

### 2. 시험 프로파일

사용자가 UI에서 편집하고 `localStorage`에 저장한다.

- `test_type`
- `duration_secs`
- `default_load`
- `clients`
- `servers`
- `associations`
- `tcp_options`
- `thresholds`

## 운영 메모

- 결과 목록은 서버 메모리에만 유지되므로 프로세스 재시작 시 초기화된다.
- 프로파일은 브라우저별로 분리된다.
- 프로파일 import 및 localStorage 복원은 현재 `TestConfig` 포맷만 허용한다.
- 예전 `network` / `socket_options` 키 기반 저장 데이터는 더 이상 자동 변환하지 않는다.

## 스키마 동기화 규칙

- 시험 설정 및 API 응답 스키마의 기준은 백엔드 Rust 타입이다.
- 프론트에서 `frontend/src/api/generated.ts`를 직접 수정하지 않는다.
- 백엔드 타입을 바꾼 뒤에는 반드시 `./scripts/generate-schema.sh`를 실행한다.
- 생성 결과로 갱신되는 파일은 `frontend/src/api/generated.ts`와 `docs/schema/*.schema.json`이다.
- 스키마 변경이 있으면 [docs/SCHEMA.md](/home/yooseongc/net-meter/docs/SCHEMA.md)와 [README.md](/home/yooseongc/net-meter/README.md)의 설명도 함께 확인한다.

## 남아 있는 작업 성격

- 문서와 코드의 경계를 계속 일치시키기
- 타입 중복 최소화 방안 검토
- 결과 영속화가 필요한지 정책 결정
- 테스트 자동화 범위 확대 검토
