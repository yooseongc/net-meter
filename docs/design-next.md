# 다음 단계 설계 (Phase 10, 11)

## Phase 10: Association 기반 설정 전환 + VLAN 지원

### 배경

현재 `PairConfig` 구조는 클라이언트 한 가지 IP만 가지는 단순 쌍 모델이었다. Avalanche 스타일로
전환하여 "몇 개의 클라이언트가 어느 서버와 어떻게 통신하는가"를 명시적으로 표현한다.

---

### 10-1. config.rs 변경

#### TestConfig 레벨

```rust
pub struct TestConfig {
    pub id: String,
    pub name: String,
    pub test_type: TestType,
    pub duration_secs: u64,
    /// 전체 가상 클라이언트 수. associations 간 균등 분배 기본값.
    pub total_clients: u32,
    pub default_load: LoadConfig,
    /// 구 pairs → Association 목록
    pub associations: Vec<Association>,
    pub network: NetworkConfig,        // 구 NsConfig → 일반화 (Phase 11 대비)
    pub thresholds: Thresholds,
}
```

#### Association (구 PairConfig)

```rust
pub struct Association {
    pub id: String,
    pub name: String,                  // (신규) 사람이 읽기 좋은 이름
    /// 클라이언트 IP 대역
    pub client_net: ClientNet,
    /// 서버 엔드포인트
    pub server: ServerEndpoint,
    pub protocol: Protocol,
    pub payload: PayloadProfile,
    pub tls: bool,
    /// per-association 부하 오버라이드. None이면 TestConfig.default_load 사용.
    #[serde(default)]
    pub load: Option<LoadConfig>,
    /// VLAN 설정 (선택). None이면 태그 없음.
    #[serde(default)]
    pub vlan: Option<VlanConfig>,
}
```

#### ClientNet — 클라이언트 IP 대역

```rust
pub struct ClientNet {
    /// 대역 시작 IP (e.g. "10.10.1.1")
    /// NS 모드: veth-c1에 IP 앨리어스로 할당
    /// External Port 모드: client_iface에 직접 할당
    pub base_ip: String,
    /// 이 association에 할당할 클라이언트 IP 수.
    /// None이면 total_clients / associations.len() 자동 계산.
    #[serde(default)]
    pub count: Option<u32>,
    /// 서브넷 마스크 길이 (기본 24)
    #[serde(default = "default_prefix_len")]
    pub prefix_len: u8,
}
// base_ip="10.10.1.1", count=50 → 10.10.1.1 ~ 10.10.1.50 을 50개 클라이언트가 각각 사용
```

#### LoadConfig — per-client 기준으로 전환

```rust
pub struct LoadConfig {
    /// [CPS] 클라이언트 1개당 초당 연결 시도 수
    /// 시스템 전체 목표 CPS = total_clients_in_association × cps_per_client
    #[serde(default)]
    pub cps_per_client: Option<u64>,       // 구 target_cps (전체 기준) 대체

    /// [CC/BW] 클라이언트 1개당 유지할 동시 연결 수
    /// 시스템 전체 CC = total_clients_in_association × cc_per_client
    #[serde(default)]
    pub cc_per_client: Option<u64>,        // 구 target_cc (전체 기준) 대체

    /// [CPS] 최대 in-flight 연결 수 (per-client). None이면 cps_per_client × 2.
    #[serde(default)]
    pub max_inflight_per_client: Option<u64>,

    pub connect_timeout_ms: Option<u64>,
    pub response_timeout_ms: Option<u64>,
    pub ramp_up_secs: u64,
}
```

> **역방향 호환**: 기존 `target_cps`/`target_cc` 필드는 직렬화 시 `#[serde(alias)]`로 읽기만
> 허용하고, 저장 시는 새 이름으로 출력한다.

#### VlanConfig — VLAN 태그 설정

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlanConfig {
    /// Outer VLAN ID (1~4094)
    pub outer_vid: u16,
    /// Double tag 시 inner VLAN ID. None이면 single tag.
    #[serde(default)]
    pub inner_vid: Option<u16>,
    /// Outer EtherType. 기본: Dot1Q (0x8100).
    /// QinQ outer에는 Dot1AD (0x88a8) 사용.
    #[serde(default)]
    pub outer_proto: VlanProto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VlanProto {
    #[default]
    Dot1Q,    // 0x8100 — IEEE 802.1Q (표준)
    Dot1AD,   // 0x88a8 — IEEE 802.1ad (QinQ outer, carrier grade)
}

impl VlanProto {
    pub fn kernel_str(self) -> &'static str {
        match self {
            Self::Dot1Q  => "802.1Q",
            Self::Dot1AD => "802.1ad",
        }
    }
}
```

#### NetworkConfig — NsConfig 일반화 (Phase 11 대비 포함)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    /// Namespace 모드 전용 설정
    #[serde(default)]
    pub ns: NsOptions,
    /// External Port 모드 전용 설정
    #[serde(default)]
    pub ext: Option<ExternalPortOptions>,
    /// 공통 소켓 옵션
    #[serde(default)]
    pub tcp_quickack: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    /// 루프백 / 로컬 호스트 — namespace 없음, 외부 장비 없음 (개발/기능 검증용)
    #[default]
    Loopback,
    /// Linux network namespace + veth pair (현재 구현)
    Namespace,
    /// 물리 NIC 2개를 직접 사용, 외부 DUT 연동 (Phase 11)
    ExternalPort,
}
```

---

### 10-2. ns/src/veth.rs 확장

```
assign_client_ips(ns: &str, iface: &str, base_ip: &str, count: u32, prefix: u8)
  → `ip addr add {base_ip+i}/{prefix} dev {iface}` × count

add_vlan_subif(ns_or_host: &str, parent: &str, vid: u16, proto: VlanProto) -> String
  → `ip link add link {parent} name {parent}.{vid} type vlan id {vid} proto {proto}`
  → `ip link set {parent}.{vid} up`
  → 반환값: 서브인터페이스 이름 (e.g. "veth-c1.100")

add_qinq_subif(ns: &str, parent: &str, outer_vid: u16, inner_vid: u16, outer_proto: VlanProto) -> String
  → outer subif 생성 → inner subif 생성 → 양쪽 up
  → 반환값: "veth-c1.100.200"
```

커널 모듈 사전 체크: `modprobe 8021q` (없으면 오류 반환)

---

### 10-3. Orchestrator 변경

NS 모드 셋업 흐름 변경:
```
기존: assign_pair_addrs() → server IP 앨리어스만
변경:
  1. client NS에 ClientNet 기반 IP 대역 앨리어싱
  2. VlanConfig 있으면 해당 NS 내 VLAN subif 생성
  3. 각 Association별 (client IP, server IP, vlan_subif) 맵 구성 → Generator 전달
```

---

### 10-4. Generator 변경

per-client 워커:
```
association.client_net.base_ip + i 에 각 워커가 bind (SO_BINDTODEVICE 또는 bind())
각 워커는 LoadConfig.cps_per_client / cc_per_client 기준으로 동작
총 동시 연결 = Σ(association_i.client_count × cc_per_client_i)
```

---

### 10-5. Frontend 변경

- `Association` 편집 모달: ClientNet (base_ip, count, prefix_len) 입력 폼
- `total_clients` 입력 (최상위) + association별 client_count 자동 계산 / 수동 오버라이드
- VLAN 설정 섹션: outer_vid, inner_vid (optional), outer_proto 선택 (dot1q/dot1ad)
- `LoadConfig` 입력 레이블 변경: "CPS per client", "CC per client"
- Header 또는 Dashboard에 "총 클라이언트 수 × per-client 부하 = 예상 전체 부하" 계산값 표시

---

---

## Phase 11: External Port Mode (물리 포트 2개 연동)

### 개념

namespace를 쓰지 않고 물리 NIC 2개를 직접 사용하여 외부 장비(DUT)를 경유하는 실제 트래픽을 발생/수신한다.

```
[net-meter: client side]                         [net-meter: server side]
   Generator                                         Responder
   (각 Association의                                 (각 Association의
    client IP bind)                                   server IP bind)
       |                                                    |
  eth1 (client_iface)  ----->  [외부 DUT]  ----->  eth2 (server_iface)
       |                       (L2 스위치,                  |
       |                       방화벽, 로드밸런서,           |
       |                       기타 피시험 장비)             |
       +----------- net-meter 단일 호스트 ---------------------+
```

- net-meter가 트래픽 발생기(client)이자 트래픽 수신기(server) 역할을 동시에 수행
- 두 포트 사이에 DUT가 존재하며, 트래픽은 반드시 외부를 경유함
- namespace 생성/삭제 불필요, IP forwarding 불필요

---

### 11-1. ExternalPortOptions 구조

```rust
/// External Port 모드 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalPortOptions {
    /// 클라이언트 측 물리 NIC 이름 (e.g. "eth1", "ens3f0")
    pub client_iface: String,
    /// 서버 측 물리 NIC 이름 (e.g. "eth2", "ens3f1")
    pub server_iface: String,

    /// DUT 방향 게이트웨이 IP (client 측).
    /// 설정 시 static ARP entry 추가: DUT가 ARP에 응답하지 않아도 통신 가능.
    #[serde(default)]
    pub client_gateway: Option<String>,
    /// DUT 방향 게이트웨이 MAC (client_gateway 있을 때 static ARP에 사용)
    #[serde(default)]
    pub client_gateway_mac: Option<String>,

    /// DUT 방향 게이트웨이 IP (server 측)
    #[serde(default)]
    pub server_gateway: Option<String>,
    /// DUT 방향 게이트웨이 MAC (server 측)
    #[serde(default)]
    pub server_gateway_mac: Option<String>,

    /// 시험 시작 전 NIC에 기존 IP 주소를 모두 제거할지 여부
    #[serde(default)]
    pub flush_iface_addrs: bool,

    /// 시험 종료 후 할당한 IP를 제거할지 여부 (기본 true)
    #[serde(default = "default_true")]
    pub cleanup_addrs: bool,
}
```

---

### 11-2. ns/src/port.rs (신규)

물리 NIC 설정 유틸리티:

```
setup_external_port(opts: &ExternalPortOptions, associations: &[Association])
  1. NIC 존재 확인: `ip link show {iface}`
  2. flush_iface_addrs=true이면: `ip addr flush dev {iface}`
  3. 각 Association의 ClientNet 기반 IP 할당:
     - VlanConfig 없음: `ip addr add {ip}/{prefix} dev {client_iface}` × count
     - VlanConfig 있음:
         a. add_vlan_subif(client_iface, outer_vid, proto) → vlan_if
         b. inner_vid 있으면 추가 중첩
         c. `ip addr add {ip}/{prefix} dev {vlan_if}` × count
  4. server_iface에도 동일하게 server IP 설정
  5. NIC up 보장: `ip link set {iface} up`
  6. static ARP entry (gateway 설정 시):
     `ip neigh replace {gw_ip} lladdr {gw_mac} dev {iface} nud permanent`

teardown_external_port(opts, associations)
  - 할당한 IP 제거, VLAN subif 제거, static ARP entry 제거
```

---

### 11-3. 소켓 바인딩 전략

| 방법 | 설명 | 용도 |
|------|------|------|
| `bind(src_ip, 0)` | 소스 IP를 특정 IP로 고정 | 가장 단순, IP가 NIC에 할당되어 있으면 커널이 올바른 NIC 선택 |
| `SO_BINDTODEVICE` | 소켓을 특정 NIC에 강제 귀속 (CAP_NET_RAW 또는 root 필요) | 동일 IP가 두 NIC에 있는 경우 등 명시 필요 시 |
| VLAN subif에 bind | VLAN subif IP로 bind → 태그된 트래픽 자동 처리 | VLAN 모드 기본 방식 |

기본 전략: **`bind(src_ip, 0)`** 사용. 동일 서브넷이 두 인터페이스에 걸친 경우에만 `SO_BINDTODEVICE` 추가 적용.

---

### 11-4. Orchestrator 변경

```
NetworkMode::ExternalPort 분기 추가:
  - NsManager 생성/삭제 스킵
  - port::setup_external_port() 호출
  - association별 (client_ip_list, server_ip, vlan_if) 맵 구성
  - Generator: client_ip bind 워커 생성
  - Responder: server_ip + server_iface bind
  - 종료 시: port::teardown_external_port() 호출
```

권한 체크: `CAP_NET_ADMIN` 필요 (IP 할당, 링크 설정)

---

### 11-5. Generator/Responder 소켓 확장

**Generator (http1.rs, http2.rs, tcp.rs):**
```rust
// 현재: TcpSocket::new_v4()  →  connect(server_addr)
// 변경: TcpSocket::new_v4()
//       if let Some(src_ip) = client_ip {
//           socket.bind(SocketAddr::new(src_ip, 0))?;
//       }
//       connect(server_addr)
```

**Responder:**
```rust
// 현재: TcpListener::bind("0.0.0.0:port")
// 변경: TcpListener::bind(SocketAddr::new(server_ip, port))
//   → External Port 모드에서는 특정 server_ip에만 리슨
```

---

### 11-6. Topology 뷰 UI 변경

External Port 모드 다이어그램 신규 추가:

```
[Client 측]          [DUT]           [Server 측]
Association A        외부 장비        Association A
10.10.1.1~50 ──────────────────────── 10.20.1.1~50
  VLAN 100                              VLAN 100

eth1 (client_iface)               eth2 (server_iface)
```

- NIC 이름, IP 대역, VLAN ID 표시
- DUT 노드는 "외부 장비 (DUT)" 라벨 + 회색 박스

---

### 11-7. UI 설정 폼

```
NetworkConfig.mode 선택: [ Loopback ] [ Namespace ] [ External Port ]

---- External Port 모드 선택 시 표시 ----
Client Interface : [eth1          ]   Server Interface: [eth2          ]
Client Gateway   : [192.168.1.1   ]   Server Gateway  : [192.168.2.1   ]
Client GW MAC    : [aa:bb:cc:...  ]   Server GW MAC   : [aa:bb:cc:...  ]
[x] Flush existing addresses on start
[x] Cleanup assigned addresses on stop
```

---

## Phase 계획 요약

| Phase | 내용 | 상태 |
|-------|------|------|
| 10 | Association 기반 설정 전환 (ClientNet, total_clients, per-client 부하), VLAN 지원 | 계획 |
| 11 | External Port Mode (물리 NIC 2개, DUT 연동, SO_BINDTODEVICE, static ARP) | 계획 |

### Phase 10 세부 작업

- [ ] `core/src/config.rs`: Association, ClientNet, VlanConfig, NetworkConfig/NetworkMode 추가; LoadConfig per-client 전환; TestConfig.total_clients 추가
- [ ] `ns/src/veth.rs`: `assign_client_ips()`, `add_vlan_subif()`, `add_qinq_subif()` 추가
- [ ] `control/src/orchestrator.rs`: Association 기반 NS IP 셋업, VLAN subif 생성
- [ ] `generator/src/`: per-client IP bind 소켓, cps/cc_per_client 기준 워커
- [ ] `frontend/src/`: Association 편집 UI, ClientNet 입력, VLAN 설정 폼, LoadConfig 레이블 변경

### Phase 11 세부 작업

- [ ] `core/src/config.rs`: ExternalPortOptions, NetworkConfig.ext 필드 추가
- [ ] `engine/crates/ns/src/port.rs` (신규): `setup_external_port()`, `teardown_external_port()`
- [ ] `control/src/orchestrator.rs`: NetworkMode::ExternalPort 분기
- [ ] `generator/src/`: client IP bind (bind(src_ip, 0) 기본, SO_BINDTODEVICE 선택)
- [ ] `responder/src/`: server IP bind
- [ ] `frontend/src/`: External Port 설정 폼, Topology 뷰 External Port 다이어그램
