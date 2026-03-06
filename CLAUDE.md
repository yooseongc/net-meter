# CLAUDE.md

## 프로젝트 개요

이 프로젝트는 Avalanche와 유사한 **네트워크 성능 계측기 / 트래픽 시험기**를 만드는 것을 목표로 한다.

핵심 목표는 다음과 같다.

- 리눅스 환경에서 동작하는 고성능 네트워크 계측 엔진 개발
- HTTP/1.1 및 HTTP/2 트래픽 기반의 성능 시험 지원
- CPS(Connections Per Second), BW(Bandwidth), CC(Concurrent Connections) 관점의 시험 수행
- 네트워크 네임스페이스로 격리된 가상 Client / Server 환경 제공
- React 기반 UI로 시작/중지, 설정, 모니터링, 통계 확인 지원
- 필요 시 eBPF / XDP를 활용한 고성능 계측 또는 패킷 경로 관찰 지원
- Rust 기반 구현 및 musl 빌드 가능성 확보

---

## 개발 원칙

 - CLAUDE는 항상 이 문서를 읽고 개발을 시작한다. 만약 변경사항이 있거나 이 문서에 문제가 있다고 생각하면 개발자와 논의하여 방향을 수정하고 문서를 변경할 수 있다.
 - CLAUDE는 개발 진행사항을 docs/PROCESS.md 에 기록하고, 작업을 시작하기 전에 읽는다.
 - 개발에 참고할 문서나 중요한 사항이 있으면 언제든지 docs 디렉터라 하위에 문서를 만들고, 그것을 읽는다.
 - 코드 작성 시 최대한 작고 논리적으로 분할하며 가독성을 중시한다. 작은 파일을 여러 개 역할 별로 분할하여 만드는 방식을 선호한다.
 - 구현은 작지만 돌아가는 프로그램 틀을 만드는 것을 우선한다. 디테일한 것은 나중에 잡을 수 있다. 단, 개발 진척 사항이 docs/PROCESS.md에 꼼꼼히 기록되어야 할 것이다.

---

## 최우선 요구사항

### 1. 개발/구동 환경

- 대상 운영체제는 Linux
- 최소 커널 버전은 **5.x 이상**
- 주요 개발/실행 환경은 다음을 우선 고려한다.
  - Rocky Linux 8+
  - Ubuntu 22.04+
  - 기타 glibc / musl 환경에서 동작 가능한 범용 Linux
- 빌드는 기본적으로 x86_64 Linux를 우선 지원하며, 이후 aarch64 확장을 고려한다

### 2. 엔진

- 엔진은 **Rust**로 작성한다
- 정적 배포 편의를 위해 **musl 타깃 빌드 가능**해야 한다
- 런타임은 비동기 기반으로 설계하며, 기본 후보는 `tokio`
- 성능 민감 경로에서는 allocation 최소화, zero-copy 지향, backpressure 제어를 중시한다

### 3. eBPF / XDP

- 필요 시 eBPF / XDP를 사용할 수 있다
- Rust 생태계에서는 **aya** 패키지를 사용한다
- eBPF/XDP는 반드시 “옵션 기능”으로 설계한다
- eBPF/XDP 없이도 핵심 기능은 동작해야 한다
- 사용 목적 예시
  - 패킷 카운팅
  - 흐름 식별
  - 지연 없는 드롭/필터링 실험
  - 커널 관측 지표 수집
  - NIC ingress 구간의 초고속 메트릭 계측

### 4. 프론트엔드

- 프론트엔드는 **React.js** 사용
- 주요 기능
  - 시험 시작 / 중지
  - 시험 프로파일 설정
  - 실시간 상태 확인
  - 통계 및 시계열 차트 확인
  - 에러/병목/제한 사항 표시
- UI는 단순 데모가 아니라 실제 운용 가능한 계측 콘솔 수준을 목표로 한다

### 5. 트래픽 프로토콜 및 시험 방식

- 대상 트래픽은 최소 다음을 포함한다.
  - HTTP/1.1
  - HTTP/2
- 시험 항목
  - **CPS**: 초당 신규 연결 수
  - **BW**: 처리 대역폭
  - **CC**: 동시 연결 수
- 측정은 단순 송신량이 아니라 애플리케이션 레벨 성공/실패와 latency 분포를 포함해야 한다

### 6. 네트워크 구조

- 시스템은 **inbound 포트**와 **outbound 포트**를 가진다
- inbound 쪽에는 **가상 Client**
- outbound 쪽에는 **가상 Server**
- Client와 Server는 각각 별도의 **network namespace** 내에 존재한다
- 네임스페이스 간 연결 방식은 veth pair, bridge, tc, routing rule 등을 활용할 수 있다
- 계측 엔진은 호스트 네임스페이스 또는 별도 control namespace에서 전체 시스템을 제어할 수 있어야 한다

---

## 아키텍처 원칙

### 전체 구성

권장 구성은 아래와 같다.

1. **Control Plane**
   - 설정 저장
   - 시험 시작/중지 orchestration
   - namespace 생성/삭제
   - 리소스 제한 관리
   - Frontend API 제공

2. **Data Plane**
   - 가상 Client 트래픽 발생기
   - 가상 Server 응답기
   - HTTP/1.1, HTTP/2 세션 처리
   - 자체 인증서를 사용한 TLS 1.2/1.3 지원이 되어야 함
   - 연결/대역폭/동시성 제어
   - 지표 수집

3. **Metrics Plane**
   - per-second 통계 집계
   - latency histogram
   - success/failure counters
   - socket, TCP, HTTP, kernel-level metrics
   - optional eBPF/XDP telemetry

4. **Frontend**
   - React SPA
   - 시험 프로파일 관리
   - 실시간 대시보드
   - 결과 리포트 조회

---

## 권장 디렉터리 구조

```text
repo/
├─ engine/                 # Rust core engine
│  ├─ crates/
│  │  ├─ core/             # 공통 타입, 설정, 에러
│  │  ├─ control/          # control plane
│  │  ├─ generator/        # client traffic generator
│  │  ├─ responder/        # virtual server
│  │  ├─ metrics/          # metrics collection/aggregation
│  │  ├─ ns/               # namespace orchestration
│  │  ├─ http1/            # HTTP/1.1 handlers
│  │  ├─ http2/            # HTTP/2 handlers
│  │  └─ ebpf/             # aya integration (optional)
│  ├─ Cargo.toml
│  └─ rust-toolchain.toml
├─ frontend/               # React.js app
│  ├─ src/
│  └─ package.json
├─ ebpf/                   # aya-bpf programs (optional)
├─ scripts/                # setup/build/run scripts
├─ docs/
├─ testbed/                # namespace/veth topology examples
└─ CLAUDE.md
```

--- 

## 주의 사항

* 단일 giant mutex 구조 금지
* 통계 집계와 트래픽 처리의 강결합 금지
* 모든 세션에 대해 과도한 per-request allocation 금지
* UI 요구사항 때문에 엔진 API를 불필요하게 오염시키지 말 것
* eBPF/XDP 의존 설계를 기본값으로 두지 말 것

---

## 예시 구조

```
[ client ns ]
  veth-client
      |
      |  (veth pair)
      |
[ host ] -- metrics/control
      |
      |  (veth pair)
      |
  veth-server
[ server ns ]
```

---

## 요구사항

* client ns, server ns를 자동 생성/정리
* 각 namespace에 독립 IP/route/lo 설정
* inbound / outbound 포트 개념은 사용자에게 논리적으로 노출
* 내부적으로는 veth/bridge/tc/routing을 사용 가능
* namespace 준비 실패 시 전체 시험 시작을 막고 명확한 오류를 제공
* 테스트 종료 후 orphan namespace/interface가 남지 않도록 cleanup 철저히 수행

---

## 지원할 시험 유형

1. CPS 시험

* 목표:
 * 초당 신규 연결 생성 능력 측정
 * 연결 성공률, 실패율, 타임아웃률 측정
* 측정 지표:
 * attempted connections/sec
 * established connections/sec
 * HTTP handshake success rate
 * error code distribution
 * connection latency histogram

2. BW 시험

* 목표:
 * 대역폭 최대치 및 안정 구간 측정
* 측정 지표:
 * tx/rx bytes per sec
 * application goodput
 * response throughput
 * retransmission indicators
 * CPU / memory usage correlation

3. CC 시험

* 목표:
 * 일정 수의 동시 연결 유지 능력 측정
* 측정 지표:
 * active connections
 * session survival duration
 * error/timeout/reset rate
 * latency under concurrency
 * memory footprint per connection

---

## HTTP 기능 요구사항

* HTTP/1.1
 * keep-alive 지원
 * GET/POST 기본 지원
 * request body / response body 크기 설정 가능
 * 헤더 수/크기 프로파일 설정 가능
* HTTP/2
 * multiplexing 지원
 * 동시 stream 수 설정 가능
 * TLS는 초기에는 선택사항이나, cleartext h2c와 TLS h2 여부를 아키텍처 차원에서 분리해서 고려
 * flow control, max concurrent streams, header table 영향 확인 가능하도록 확장 여지 확보
* 공통 측정 항목
 * request count
 * response count
 * success/failure
 * status code distribution
 * request latency
 * time to first byte
 * full response completion latency
 * bytes in/out
 * socket-level errors
 * TLS 사용 시 handshake 지표