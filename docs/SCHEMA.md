# SCHEMA

현재 `net-meter`의 시험 설정 및 주요 API 응답 스키마를 정리한다.

## 기준 원본

- 백엔드 Rust 타입이 스키마의 기준이다.
- 핵심 정의 위치:
  - [config.rs](/home/yooseongc/net-meter/engine/crates/core/src/config.rs)
  - [snapshot.rs](/home/yooseongc/net-meter/engine/crates/core/src/snapshot.rs)
  - [state.rs](/home/yooseongc/net-meter/engine/crates/core/src/state.rs)
  - [schema.rs](/home/yooseongc/net-meter/engine/crates/control/src/schema.rs)
  - [result.rs](/home/yooseongc/net-meter/engine/crates/control/src/result.rs)
  - [event.rs](/home/yooseongc/net-meter/engine/crates/control/src/event.rs)

## 생성 산출물

- 프론트 타입: [generated.ts](/home/yooseongc/net-meter/frontend/src/api/generated.ts)
- JSON Schema: [docs/schema](/home/yooseongc/net-meter/docs/schema)

생성 명령:

```bash
./scripts/generate-schema.sh
```

## 주요 스키마 묶음

### 시험 설정

- [TestConfig.schema.json](/home/yooseongc/net-meter/docs/schema/TestConfig.schema.json)
- 포함 하위 타입:
  - `ClientDef`
  - `ServerDef`
  - `Association`
  - `PayloadProfile`
  - `LoadConfig`
  - `Thresholds`
  - `TcpOptions`

핵심 구분:

- `TestConfig`는 시험 프로파일이다.
- 네트워크 모드 자체는 `TestConfig`가 아니라 서버 런타임 설정이다.
- 시험 프로파일의 TCP 관련 옵션만 `tcp_options`에 포함된다.

### 런타임 상태

- [TestStatus.schema.json](/home/yooseongc/net-meter/docs/schema/TestStatus.schema.json)
- [RuntimeConfig.schema.json](/home/yooseongc/net-meter/docs/schema/RuntimeConfig.schema.json)
- [TestState.schema.json](/home/yooseongc/net-meter/docs/schema/TestState.schema.json)

`/api/status`는 다음을 함께 반환한다.

- `state`
- `config`
- `elapsed_secs`
- `runtime`

### 메트릭

- [MetricsSnapshot.schema.json](/home/yooseongc/net-meter/docs/schema/MetricsSnapshot.schema.json)
- [PerProtocolSnapshot.schema.json](/home/yooseongc/net-meter/docs/schema/PerProtocolSnapshot.schema.json)
- [HistogramBucket.schema.json](/home/yooseongc/net-meter/docs/schema/HistogramBucket.schema.json)

`HistogramBucket.le_ms`는 일반 버킷에서는 숫자(ms)이고, 마지막 `+Inf` 버킷에서는 `null`이다.

대표 필드:

- 연결 수치: `connections_*`, `active_connections`
- 처리량: `cps`, `rps`, `bytes_*`
- 지연: `latency_*`, `connect_*`, `ttfb_*`
- 분해 지표: `by_protocol`, `status_code_breakdown`

### 결과 및 이벤트

- [TestResult.schema.json](/home/yooseongc/net-meter/docs/schema/TestResult.schema.json)
- [TestEventType.schema.json](/home/yooseongc/net-meter/docs/schema/TestEventType.schema.json)

## 프론트 사용 원칙

- 프론트는 [generated.ts](/home/yooseongc/net-meter/frontend/src/api/generated.ts)에서 타입을 import/re-export 한다.
- 수기 타입 정의를 `client.ts`에 다시 만들지 않는다.
- import/export와 localStorage는 현재 스키마만 지원한다.
