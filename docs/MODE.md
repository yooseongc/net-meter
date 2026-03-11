# MODE

`net-meter`의 3가지 시험 모드와 각 모드에서의 동작 방식을 정리한다.

## 공통 전제

- 시험 종류는 `cps`, `cc`, `bw` 세 가지다.
- 프로토콜은 `tcp`, `http1`, `http2`를 지원한다.
- 실제 네트워크 환경은 서버 런타임 설정(`--mode`)이 결정한다.
- 각 시험은 `default_load` 또는 association별 `load`를 사용한다.

## CPS

Connections Per Second.

### 목적

- 초당 신규 연결 처리량 측정
- connect / transact / close 경로의 지연 분포 측정
- 에러율과 timeout 비율 확인

### 동작 방식

- 각 워커는 연결을 만들고 요청/응답 또는 바이트 교환을 수행한 뒤 연결을 닫는다.
- 가능한 빠르게 이 루프를 반복한다.
- `ramp_up_secs`가 있으면 목표 부하까지 선형 증가한다.
- `ramp_down_secs`가 있으면 종료 전에 선형 감소한다.

### 주로 보는 지표

- `cps`
- `connections_attempted`
- `connections_established`
- `connections_failed`
- `connections_timed_out`
- `latency_p50_ms`
- `latency_p95_ms`
- `latency_p99_ms`
- `connect_p99_ms`
- `ttfb_p99_ms`

### 해석 포인트

- CPS가 높아도 `connections_failed`나 `timeout`이 늘면 유효 처리량이 아니다.
- `connect_p99_ms`가 튀면 네트워크 또는 listener backlog 문제가 의심된다.
- HTTP 계열에서는 `ttfb_*`가 애플리케이션 응답 경향을 더 잘 보여준다.

## CC

Concurrent Connections.

### 목적

- 목표 동시 연결 수를 안정적으로 유지할 수 있는지 측정
- 장시간 연결 유지 시 메모리/상태 테이블 부담 확인
- keep-alive 또는 session 유지 특성 확인

### 동작 방식

- 전체 `num_connections`를 워커 기준으로 분배한다.
- 연결이 끊기면 다시 채워 넣어 목표 동시 연결 수를 유지한다.
- CPS처럼 빠르게 열고 닫는 것이 아니라, 동시 연결 상태를 유지하는 쪽이 핵심이다.

### 주로 보는 지표

- `active_connections`
- `connections_established`
- `connections_failed`
- `bytes_tx_per_sec`
- `bytes_rx_per_sec`
- `latency_*`
- 서버 측 `server_requests`

### 해석 포인트

- `active_connections`가 목표치에 못 미치면 연결 유지 안정성이 떨어지는 것이다.
- 연결 수는 유지되지만 처리량이 낮으면 애플리케이션 또는 flow control 병목일 수 있다.

## BW

Bandwidth.

### 목적

- 최대 처리 가능한 전송량 측정
- 지속적인 송수신에서 goodput 경향 확인
- 프로토콜별 스트리밍 한계 확인

### 동작 방식

- CC와 유사하게 연결을 유지하되, payload 크기와 스트리밍 동작이 처리량 중심으로 작동한다.
- TCP는 바이트 스트림 송수신에 가깝고, HTTP/2는 다중 스트림 영향도 함께 반영된다.

### 주로 보는 지표

- `bytes_tx_per_sec`
- `bytes_rx_per_sec`
- `bytes_tx_total`
- `bytes_rx_total`
- `rps`
- `active_connections`
- `latency_p99_ms`

### 해석 포인트

- BW 모드에서는 `cps`보다 초당 바이트량이 더 중요하다.
- HTTP/2는 `h2_max_concurrent_streams` 설정에 따라 같은 연결 수에서도 결과가 크게 달라질 수 있다.
- TX와 RX의 불균형은 요청/응답 payload 비대칭 또는 서버 응답 제한을 의미할 수 있다.

## 프로토콜별 차이

### TCP

- 가장 낮은 레벨의 바이트 교환
- HTTP 파싱/헤더 비용이 없다
- 순수 연결/전송 경향을 보기 좋다

### HTTP/1.1

- keep-alive, GET/POST, body 크기 조절 가능
- 요청/응답 처리 오버헤드가 반영된다
- TTFB 해석이 유의미하다

### HTTP/2

- cleartext h2c와 TLS h2를 지원한다
- 연결당 다중 스트림 사용 가능
- 단일 연결 수 대비 더 높은 처리량이 나올 수 있다

## 네트워크 모드와 시험 해석

### Loopback

- 가장 단순한 기능 검증용
- DUT 없이 generator와 responder가 한 호스트에서 통신한다
- 절대 성능 수치보다 기능/회귀 확인에 적합하다

### Namespace

- client / server를 Linux netns로 분리한다
- DUT 없이도 IP, 라우팅, VLAN 시나리오를 더 현실적으로 검증할 수 있다

### External Port

- 물리 NIC 2개를 통해 외부 DUT를 경유한다
- 정책 라우팅으로 short-circuit를 방지한다
- 실제 장비 검증에 가장 가깝다

## 추천 사용 순서

1. Loopback에서 기능과 프로파일을 먼저 검증
2. Namespace에서 IP/VLAN/격리 구성을 점검
3. External Port에서 DUT 포함 실장비 시험 수행

