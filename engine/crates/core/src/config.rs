use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 기본 열거형
// ---------------------------------------------------------------------------

/// 시험 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestType {
    /// Connections Per Second: 초당 신규 연결 수 측정
    Cps,
    /// Bandwidth: 처리 대역폭 측정
    Bw,
    /// Concurrent Connections: 동시 연결 유지 측정
    Cc,
}

/// 트래픽 프로토콜
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// Raw TCP (HTTP 없이 바이트 교환)
    Tcp,
    /// HTTP/1.1
    Http1,
    /// HTTP/2 h2c (cleartext)
    Http2,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Http1 => "http1",
            Self::Http2 => "http2",
        }
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// HTTP 요청 메서드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

// ---------------------------------------------------------------------------
// 페이로드 프로파일
// ---------------------------------------------------------------------------

/// TCP 페이로드 프로파일
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpPayload {
    /// 클라이언트가 연결당 전송할 바이트 수
    #[serde(default)]
    pub tx_bytes: usize,
    /// 서버가 응답할 바이트 수
    #[serde(default)]
    pub rx_bytes: usize,
}

impl Default for TcpPayload {
    fn default() -> Self {
        Self { tx_bytes: 0, rx_bytes: 0 }
    }
}

/// HTTP 페이로드 프로파일
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpPayload {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default)]
    pub request_body_bytes: Option<usize>,
    #[serde(default)]
    pub response_body_bytes: Option<usize>,
    #[serde(default)]
    pub path_extra_bytes: Option<usize>,
    /// HTTP/2 BW 모드: 연결당 최대 동시 스트림 수 (기본 10)
    #[serde(default)]
    pub h2_max_concurrent_streams: Option<u32>,
}

impl Default for HttpPayload {
    fn default() -> Self {
        Self {
            method: HttpMethod::Get,
            path: "/".to_string(),
            request_body_bytes: None,
            response_body_bytes: None,
            path_extra_bytes: None,
            h2_max_concurrent_streams: None,
        }
    }
}

/// 프로토콜별 페이로드 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PayloadProfile {
    Tcp(TcpPayload),
    Http(HttpPayload),
}

impl PayloadProfile {
    pub fn default_for(protocol: Protocol) -> Self {
        match protocol {
            Protocol::Tcp => PayloadProfile::Tcp(TcpPayload::default()),
            Protocol::Http1 | Protocol::Http2 => PayloadProfile::Http(HttpPayload::default()),
        }
    }
}

// ---------------------------------------------------------------------------
// 부하 설정 (per-client 기준)
// ---------------------------------------------------------------------------

/// 클라이언트 부하 파라미터 (클라이언트 1개 기준).
///
/// 시스템 전체 부하 = client_count × per_client 값.
/// association이 None으로 두면 TestConfig.default_load를 사용.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadConfig {
    /// [CPS] 클라이언트 1개당 초당 연결 시도 수.
    /// 전체 CPS = effective_client_count × cps_per_client
    #[serde(alias = "target_cps", default)]
    pub cps_per_client: Option<u64>,

    /// [CC/BW] 클라이언트 1개당 유지할 동시 연결 수.
    /// 전체 CC = effective_client_count × cc_per_client
    #[serde(alias = "target_cc", default)]
    pub cc_per_client: Option<u64>,

    /// [CPS] 최대 in-flight 연결 수 (per-client). None이면 cps_per_client × 2.
    #[serde(alias = "max_inflight", default)]
    pub max_inflight_per_client: Option<u64>,

    /// TCP 연결 타임아웃 (ms). None이면 5000.
    #[serde(default)]
    pub connect_timeout_ms: Option<u64>,

    /// 응답 완료 타임아웃 (ms). None이면 30000.
    #[serde(default)]
    pub response_timeout_ms: Option<u64>,

    /// 목표 CPS/CC까지 점진적으로 증가하는 구간 (초). 0이면 즉시 전속력.
    #[serde(default)]
    pub ramp_up_secs: u64,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            cps_per_client: Some(100),
            cc_per_client: None,
            max_inflight_per_client: None,
            connect_timeout_ms: Some(5000),
            response_timeout_ms: Some(30000),
            ramp_up_secs: 0,
        }
    }
}

impl LoadConfig {
    pub fn effective_cps(&self) -> u64 {
        self.cps_per_client.unwrap_or(100).max(1)
    }
    pub fn effective_cc(&self) -> u64 {
        self.cc_per_client.unwrap_or(50)
    }
    pub fn effective_max_inflight(&self) -> u64 {
        let cps = self.effective_cps();
        self.max_inflight_per_client.unwrap_or(cps.saturating_mul(2).min(65535))
    }
    pub fn connect_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.connect_timeout_ms.unwrap_or(5000))
    }
    pub fn response_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.response_timeout_ms.unwrap_or(30000))
    }
}

// ---------------------------------------------------------------------------
// 임계값 / 알람 설정
// ---------------------------------------------------------------------------

/// 시험 Pass/Fail 임계값. 위반 시 대시보드 경고 또는 자동 중단.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Thresholds {
    #[serde(default)]
    pub min_cps: Option<f64>,
    #[serde(default)]
    pub max_error_rate_pct: Option<f64>,
    #[serde(default)]
    pub max_latency_p99_ms: Option<f64>,
    #[serde(default)]
    pub auto_stop_on_fail: bool,
}

// ---------------------------------------------------------------------------
// 클라이언트 IP 대역
// ---------------------------------------------------------------------------

/// 클라이언트 IP 대역 설정.
///
/// NS 모드: veth-c1에 IP alias로 할당.
/// External Port 모드: client_iface에 직접 할당.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientNet {
    /// 대역 시작 IP (e.g. "10.10.1.1")
    pub base_ip: String,
    /// 이 association에 할당할 클라이언트 IP 수 (= 워커 수).
    /// None이면 total_clients / associations.len() 자동 계산.
    #[serde(default)]
    pub count: Option<u32>,
    /// 서브넷 마스크 길이 (기본 24)
    #[serde(default = "default_prefix_len")]
    pub prefix_len: u8,
}

fn default_prefix_len() -> u8 {
    24
}

impl Default for ClientNet {
    fn default() -> Self {
        Self {
            base_ip: "10.10.1.1".to_string(),
            count: None,
            prefix_len: 24,
        }
    }
}

// ---------------------------------------------------------------------------
// 서버 엔드포인트
// ---------------------------------------------------------------------------

/// 서버 엔드포인트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEndpoint {
    pub id: String,
    /// 서버 IP. NS 모드: None이면 자동 할당 (10.20.1.{N}).
    /// 로컬 모드: None이면 127.0.0.1.
    #[serde(default)]
    pub ip: Option<String>,
    pub port: u16,
}

// ---------------------------------------------------------------------------
// VLAN 설정
// ---------------------------------------------------------------------------

/// VLAN 태그 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlanConfig {
    /// Outer VLAN ID (1~4094)
    pub outer_vid: u16,
    /// Double tag 시 inner VLAN ID. None이면 single tag.
    #[serde(default)]
    pub inner_vid: Option<u16>,
    /// Outer EtherType. 기본: Dot1Q (0x8100).
    #[serde(default)]
    pub outer_proto: VlanProto,
}

/// VLAN 외부 EtherType 설정
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VlanProto {
    /// 0x8100 — IEEE 802.1Q (표준)
    #[default]
    Dot1Q,
    /// 0x88a8 — IEEE 802.1ad (QinQ outer, carrier grade)
    Dot1AD,
}

impl VlanProto {
    pub fn kernel_str(self) -> &'static str {
        match self {
            Self::Dot1Q => "802.1Q",
            Self::Dot1AD => "802.1ad",
        }
    }
}

// ---------------------------------------------------------------------------
// Association (구 PairConfig)
// ---------------------------------------------------------------------------

/// 하나의 클라이언트 IP 대역 ↔ 서버 엔드포인트 트래픽 설정.
///
/// 이전 버전의 PairConfig를 대체한다.
/// 각 Association은 독립적인 클라이언트 IP 대역에서 하나의 서버를 대상으로
/// 트래픽을 발생시킨다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Association {
    /// 식별자
    pub id: String,
    /// 사람이 읽기 좋은 이름
    #[serde(default)]
    pub name: String,
    /// 클라이언트 IP 대역
    #[serde(default)]
    pub client_net: ClientNet,
    /// 서버 엔드포인트
    pub server: ServerEndpoint,
    /// 사용할 프로토콜
    pub protocol: Protocol,
    /// 프로토콜별 페이로드 설정
    pub payload: PayloadProfile,
    /// TLS 활성화 (Http1 / Http2 프로토콜에만 적용, TCP는 무시)
    #[serde(default)]
    pub tls: bool,
    /// per-association 부하 설정. None이면 TestConfig.default_load 사용.
    #[serde(default)]
    pub load: Option<LoadConfig>,
    /// VLAN 설정 (선택). None이면 태그 없음.
    #[serde(default)]
    pub vlan: Option<VlanConfig>,
}

impl Association {
    /// 유효 부하 설정 반환 (association 개별 설정 > 글로벌 기본값)
    pub fn effective_load<'a>(&'a self, default: &'a LoadConfig) -> &'a LoadConfig {
        self.load.as_ref().unwrap_or(default)
    }

    /// 이 association에서 사용할 클라이언트 워커 수 계산.
    ///
    /// 우선순위: client_net.count > total_clients 균등 분배 > 1
    pub fn effective_client_count(&self, total_clients: u32, num_associations: usize) -> u32 {
        if let Some(count) = self.client_net.count {
            count.max(1)
        } else if total_clients > 0 && num_associations > 0 {
            (total_clients / num_associations as u32).max(1)
        } else {
            1
        }
    }
}

// ---------------------------------------------------------------------------
// 네트워크 설정 (구 NsConfig 일반화)
// ---------------------------------------------------------------------------

/// 네트워크 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    /// 루프백 / 로컬 호스트 (개발/기능 검증용)
    #[default]
    Loopback,
    /// Linux network namespace + veth pair (CAP_NET_ADMIN 필요)
    Namespace,
    /// 물리 NIC 2개를 직접 사용, 외부 DUT 연동 (Phase 11)
    ExternalPort,
}

/// Namespace 모드 전용 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NsOptions {
    /// 네임스페이스 이름 prefix (예: "nm" → "nm-client", "nm-server")
    #[serde(default = "default_ns_prefix")]
    pub netns_prefix: String,
}

fn default_ns_prefix() -> String {
    "nm".to_string()
}

impl Default for NsOptions {
    fn default() -> Self {
        Self { netns_prefix: "nm".to_string() }
    }
}

/// External Port 모드 전용 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalPortOptions {
    /// 클라이언트 측 물리 NIC 이름 (e.g. "eth1", "ens3f0")
    pub client_iface: String,
    /// 서버 측 물리 NIC 이름 (e.g. "eth2", "ens3f1")
    pub server_iface: String,
    /// DUT 방향 게이트웨이 IP (client 측)
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

fn default_true() -> bool {
    true
}

/// 전체 네트워크/소켓 설정 (구 NsConfig 일반화)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// 네트워크 모드
    #[serde(default)]
    pub mode: NetworkMode,
    /// Namespace 모드 전용 설정
    #[serde(default)]
    pub ns: NsOptions,
    /// External Port 모드 전용 설정
    #[serde(default)]
    pub ext: Option<ExternalPortOptions>,
    /// accept된 소켓에 TCP_QUICKACK 설정 (Delayed ACK 비활성화)
    #[serde(default)]
    pub tcp_quickack: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Loopback,
            ns: NsOptions::default(),
            ext: None,
            tcp_quickack: false,
        }
    }
}

// ---------------------------------------------------------------------------
// TestConfig — 전체 시험 설정
// ---------------------------------------------------------------------------

/// 전체 시험 설정. 하나 이상의 Association을 정의한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// 고유 ID (UUID v4)
    pub id: String,
    /// 사람이 읽을 수 있는 이름
    pub name: String,
    /// 시험 종류 (모든 association에 적용)
    pub test_type: TestType,
    /// 시험 지속 시간 (초). 0이면 수동 중지까지 계속.
    pub duration_secs: u64,
    /// 전체 가상 클라이언트 수. associations 간 균등 분배 기본값.
    /// 0이면 각 association의 client_net.count 사용 (없으면 1).
    #[serde(default)]
    pub total_clients: u32,
    /// 글로벌 기본 부하 설정 (association이 override 가능)
    pub default_load: LoadConfig,
    /// Association 목록 (구 pairs)
    #[serde(alias = "pairs")]
    pub associations: Vec<Association>,
    /// 네트워크 설정 (구 ns_config)
    #[serde(alias = "ns_config", default)]
    pub network: NetworkConfig,
    /// 임계값 / 알람 설정
    #[serde(default)]
    pub thresholds: Thresholds,
}

impl TestConfig {
    /// 단일 HTTP/1.1 association의 기본 설정
    pub fn default_single_pair() -> Self {
        let assoc = Association {
            id: uuid::Uuid::new_v4().to_string(),
            name: "default".to_string(),
            client_net: ClientNet::default(),
            server: ServerEndpoint { id: "server-0".to_string(), ip: None, port: 8080 },
            protocol: Protocol::Http1,
            payload: PayloadProfile::Http(HttpPayload::default()),
            tls: false,
            load: None,
            vlan: None,
        };
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Default Config".to_string(),
            test_type: TestType::Cps,
            duration_secs: 60,
            total_clients: 0,
            default_load: LoadConfig::default(),
            associations: vec![assoc],
            network: NetworkConfig::default(),
            thresholds: Thresholds::default(),
        }
    }

    /// 사용 중인 프로토콜 집합 반환 (메트릭 컬렉터 생성에 활용)
    pub fn active_protocols(&self) -> Vec<Protocol> {
        let mut seen = std::collections::HashSet::new();
        self.associations
            .iter()
            .filter(|a| seen.insert(a.protocol))
            .map(|a| a.protocol)
            .collect()
    }

    /// association별 서버 주소 맵 반환 (assoc_id → "host:port")
    /// 로컬 모드용: NS 모드에서는 오케스트레이터가 별도 계산
    pub fn local_server_addrs(&self) -> HashMap<String, String> {
        self.associations
            .iter()
            .map(|a| {
                let host = a.server.ip.as_deref().unwrap_or("127.0.0.1");
                (a.id.clone(), format!("{}:{}", host, a.server.port))
            })
            .collect()
    }

    /// association 수 반환
    pub fn num_associations(&self) -> usize {
        self.associations.len()
    }
}
