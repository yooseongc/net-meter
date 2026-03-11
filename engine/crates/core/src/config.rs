use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 기본 열거형
// ---------------------------------------------------------------------------

/// 시험 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestType {
    /// Connections Per Second: 각 워커가 connect→transact→close 루프를 최대 속도로 반복
    Cps,
    /// Bandwidth: CC와 동일 구조, 페이로드 크기로 대역폭 결정
    Bw,
    /// Concurrent Connections: num_connections 개의 연결을 동시에 유지
    Cc,
}

/// 트래픽 프로토콜
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
// 부하 설정
// ---------------------------------------------------------------------------

/// 부하 파라미터 (association당).
///
/// - CPS 모드: 각 워커가 connect→transact→close 루프를 최대 속도로 반복.
///   `num_connections`는 전체 병렬 연결 루프 수(총량, 워커 수로 자동 분배).
/// - CC/BW 모드: 전체 동시 연결 수 (`num_connections`)를 워커 수로 자동 분배.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadConfig {
    /// CPS: 전체 병렬 연결 루프 수 (워커 수로 자동 분배, 기본 1).
    /// CC/BW: 전체 유지할 동시 연결 수 (워커 수로 자동 분배).
    #[serde(default)]
    pub num_connections: Option<u64>,

    /// TCP 연결 타임아웃 (ms). None이면 5000.
    #[serde(default)]
    pub connect_timeout_ms: Option<u64>,

    /// 응답 완료 타임아웃 (ms). None이면 30000.
    #[serde(default)]
    pub response_timeout_ms: Option<u64>,

    /// 목표까지 점진적으로 증가하는 구간 (초). 0이면 즉시 전속력.
    #[serde(default)]
    pub ramp_up_secs: u64,

    /// 종료 전 부하를 점진적으로 감소하는 구간 (초). 0이면 즉시 중지.
    #[serde(default)]
    pub ramp_down_secs: u64,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            num_connections: Some(1),
            connect_timeout_ms: Some(5000),
            response_timeout_ms: Some(30000),
            ramp_up_secs: 0,
            ramp_down_secs: 0,
        }
    }
}

impl LoadConfig {
    /// 전체 연결 수 (워커 분배 전 총량).
    pub fn effective_num_connections(&self) -> u64 {
        self.num_connections.unwrap_or(1).max(1)
    }

    /// 워커당 연결 수. 총 연결 수를 `worker_count`로 나눈다 (최소 1).
    pub fn connections_per_worker(&self, worker_count: usize) -> u64 {
        let total = self.effective_num_connections() as usize;
        let count = (total + worker_count - 1) / worker_count; // ceiling division
        count.max(1) as u64
    }

    /// num_connections를 교체한 복사본 반환.
    pub fn with_num_connections(self, n: u64) -> Self {
        Self { num_connections: Some(n), ..self }
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
// ClientDef: 클라이언트 IP 대역 정의
// ---------------------------------------------------------------------------

/// 클라이언트 IP 대역 정의.
///
/// 각 ClientDef는 독립된 소스 IP 풀을 나타낸다.
/// NS 모드: veth-c1에 IP alias로 할당.
/// External Port 모드: client_iface에 직접 할당.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClientDef {
    /// 고유 ID
    pub id: String,
    /// 사람이 읽기 좋은 이름
    pub name: String,
    /// IP 대역 (CIDR 표기, e.g. "10.10.1.1/24").
    /// base IP = 시작 IP, prefix_len = 서브넷 마스크 길이.
    pub cidr: String,
    /// 이 대역에서 사용할 워커(IP) 수. None이면 1.
    #[serde(default)]
    pub count: Option<u32>,
}

impl ClientDef {
    /// cidr에서 (base_ip, prefix_len) 파싱.
    pub fn parse_cidr(&self) -> Result<(std::net::Ipv4Addr, u8), String> {
        let (ip_str, prefix_str) = self.cidr.split_once('/').ok_or_else(|| {
            format!("Invalid CIDR '{}': missing prefix length (e.g. '10.10.1.1/24')", self.cidr)
        })?;
        let prefix = prefix_str
            .parse::<u8>()
            .map_err(|e| format!("Invalid CIDR '{}': bad prefix length: {}", self.cidr, e))?;
        if prefix > 32 {
            return Err(format!("Invalid CIDR '{}': prefix length {} exceeds 32", self.cidr, prefix));
        }
        let ip = ip_str
            .parse::<std::net::Ipv4Addr>()
            .map_err(|e| format!("Invalid CIDR '{}': {}", self.cidr, e))?;
        Ok((ip, prefix))
    }

    pub fn effective_count(&self) -> u32 {
        self.count.unwrap_or(1).max(1)
    }
}

impl Default for ClientDef {
    fn default() -> Self {
        Self {
            id: "client-0".to_string(),
            name: "client-0".to_string(),
            cidr: "10.10.1.1/24".to_string(),
            count: Some(1),
        }
    }
}

// ---------------------------------------------------------------------------
// ServerDef: 서버 엔드포인트 정의
// ---------------------------------------------------------------------------

/// 서버 엔드포인트 정의.
///
/// 프로토콜과 TLS 설정을 포함한다.
/// 여러 Association이 동일한 ServerDef를 참조할 수 있다.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerDef {
    /// 고유 ID
    pub id: String,
    /// 사람이 읽기 좋은 이름
    pub name: String,
    /// 서버 IP. NS 모드: None이면 자동 할당 (10.20.1.{N}).
    /// 로컬 모드: None이면 127.0.0.1.
    #[serde(default)]
    pub ip: Option<String>,
    pub port: u16,
    /// 사용 프로토콜
    pub protocol: Protocol,
    /// TLS 활성화 (Http1 / Http2 프로토콜에만 적용, TCP는 무시)
    #[serde(default)]
    pub tls: bool,
    /// TLS SNI 서버 이름. IP 주소를 입력하면 "localhost"로 대체된다.
    /// 기본값: "test.net-meter.com"
    #[serde(default = "default_tls_server_name")]
    pub tls_server_name: String,
}

fn default_tls_server_name() -> String {
    "test.net-meter.com".to_string()
}

impl Default for ServerDef {
    fn default() -> Self {
        Self {
            id: "server-0".to_string(),
            name: "server-0".to_string(),
            ip: None,
            port: 8080,
            protocol: Protocol::Http1,
            tls: false,
            tls_server_name: default_tls_server_name(),
        }
    }
}

// ---------------------------------------------------------------------------
// VLAN 설정
// ---------------------------------------------------------------------------

/// VLAN 태그 설정
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VlanProto {
    /// 0x8100 — IEEE 802.1Q (표준)
    #[default]
    Dot1Q,
    /// 0x88a8 — IEEE 802.1ad (QinQ outer, carrier grade)
    #[serde(rename = "dot1_ad")]
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
// Association: Client ↔ Server 트래픽 매핑
// ---------------------------------------------------------------------------

/// ClientDef와 ServerDef 간의 트래픽 매핑.
///
/// Association 자체는 ID 참조만 담당하고, IP 대역/프로토콜 등의
/// 세부 설정은 각각 ClientDef/ServerDef에 위임한다.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Association {
    /// 식별자
    pub id: String,
    /// 사람이 읽기 좋은 이름
    #[serde(default)]
    pub name: String,
    /// 참조하는 ClientDef의 id
    pub client_id: String,
    /// 참조하는 ServerDef의 id
    pub server_id: String,
    /// 프로토콜별 페이로드 설정 (ServerDef.protocol과 일치해야 함)
    pub payload: PayloadProfile,
    /// VLAN 설정 (선택). None이면 태그 없음.
    #[serde(default)]
    pub vlan: Option<VlanConfig>,
    /// per-association 부하 설정 오버라이드. None이면 TestConfig.default_load 사용.
    #[serde(default)]
    pub load: Option<LoadConfig>,
}

impl Association {
    /// 유효 부하 설정 반환 (association 개별 설정 > 글로벌 기본값)
    pub fn effective_load<'a>(&'a self, default: &'a LoadConfig) -> &'a LoadConfig {
        self.load.as_ref().unwrap_or(default)
    }
}

// ---------------------------------------------------------------------------
// 네트워크 설정
// ---------------------------------------------------------------------------

/// 네트워크 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
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

/// 시험 실행 시 적용되는 TCP 옵션.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TcpOptions {
    #[serde(default)]
    pub tcp_quickack: bool,
}

impl Default for TcpOptions {
    fn default() -> Self {
        Self { tcp_quickack: false }
    }
}

// ---------------------------------------------------------------------------
// TestConfig — 전체 시험 설정
// ---------------------------------------------------------------------------

/// 전체 시험 설정.
///
/// clients/servers 목록을 정의하고, associations로 연결을 매핑한다.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestConfig {
    /// 고유 ID (UUID v4)
    pub id: String,
    /// 사람이 읽을 수 있는 이름
    pub name: String,
    /// 시험 종류 (모든 association에 적용)
    pub test_type: TestType,
    /// 시험 지속 시간 (초). 0이면 수동 중지까지 계속.
    pub duration_secs: u64,
    /// 글로벌 기본 부하 설정 (association이 override 가능)
    pub default_load: LoadConfig,
    /// 클라이언트 IP 대역 정의 목록
    pub clients: Vec<ClientDef>,
    /// 서버 엔드포인트 정의 목록
    pub servers: Vec<ServerDef>,
    /// 클라이언트 ↔ 서버 트래픽 매핑 목록
    pub associations: Vec<Association>,
    /// 시험별 TCP 옵션.
    #[serde(default)]
    pub tcp_options: TcpOptions,
    /// 임계값 / 알람 설정
    #[serde(default)]
    pub thresholds: Thresholds,
}

impl TestConfig {
    /// 단일 HTTP/1.1 association의 기본 설정
    pub fn default_single_pair() -> Self {
        let client_id = uuid::Uuid::new_v4().to_string();
        let server_id = "server-0".to_string();
        let client = ClientDef {
            id: client_id.clone(),
            name: "client-0".to_string(),
            cidr: "10.10.1.1/24".to_string(),
            count: Some(1),
        };
        let server = ServerDef {
            id: server_id.clone(),
            name: "server-0".to_string(),
            ip: None,
            port: 8080,
            protocol: Protocol::Http1,
            tls: false,
            tls_server_name: default_tls_server_name(),
        };
        let assoc = Association {
            id: uuid::Uuid::new_v4().to_string(),
            name: "default".to_string(),
            client_id,
            server_id,
            payload: PayloadProfile::Http(HttpPayload::default()),
            vlan: None,
            load: None,
        };
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Default Config".to_string(),
            test_type: TestType::Cps,
            duration_secs: 60,
            default_load: LoadConfig::default(),
            clients: vec![client],
            servers: vec![server],
            associations: vec![assoc],
            tcp_options: TcpOptions::default(),
            thresholds: Thresholds::default(),
        }
    }

    /// 사용 중인 프로토콜 집합 반환 (servers 기준)
    pub fn active_protocols(&self) -> Vec<Protocol> {
        let mut seen = std::collections::HashSet::new();
        self.servers
            .iter()
            .filter(|s| seen.insert(s.protocol))
            .map(|s| s.protocol)
            .collect()
    }

    /// server_id → &ServerDef 맵 (불필요한 clone 없이 참조 반환)
    pub fn server_map(&self) -> HashMap<&str, &ServerDef> {
        self.servers.iter().map(|s| (s.id.as_str(), s)).collect()
    }

    /// client_id → &ClientDef 맵 (불필요한 clone 없이 참조 반환)
    pub fn client_map(&self) -> HashMap<&str, &ClientDef> {
        self.clients.iter().map(|c| (c.id.as_str(), c)).collect()
    }

    /// assoc_id → "host:port" 맵 (로컬 모드용)
    pub fn local_server_addrs(&self) -> HashMap<String, String> {
        let server_map = self.server_map();
        self.associations
            .iter()
            .filter_map(|a| {
                let server = server_map.get(a.server_id.as_str())?;
                let host = server.ip.as_deref().unwrap_or("127.0.0.1");
                Some((a.id.clone(), format!("{}:{}", host, server.port)))
            })
            .collect()
    }

    pub fn num_associations(&self) -> usize {
        self.associations.len()
    }

    /// 설정의 논리적 일관성을 검사한다.
    ///
    /// 시험 시작 전에 호출해 런타임 오류를 조기에 방지한다.
    pub fn validate(&self) -> Result<(), String> {
        if self.clients.is_empty() {
            return Err("clients 목록이 비어 있습니다.".to_string());
        }
        if self.servers.is_empty() {
            return Err("servers 목록이 비어 있습니다.".to_string());
        }
        if self.associations.is_empty() {
            return Err("associations 목록이 비어 있습니다.".to_string());
        }

        let client_ids: std::collections::HashSet<&str> =
            self.clients.iter().map(|c| c.id.as_str()).collect();
        let server_ids: std::collections::HashSet<&str> =
            self.servers.iter().map(|s| s.id.as_str()).collect();

        for assoc in &self.associations {
            if !client_ids.contains(assoc.client_id.as_str()) {
                return Err(format!(
                    "Association '{}': client_id '{}'가 clients 목록에 없습니다. 가능한 ID: [{}]",
                    assoc.id,
                    assoc.client_id,
                    client_ids.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            if !server_ids.contains(assoc.server_id.as_str()) {
                return Err(format!(
                    "Association '{}': server_id '{}'가 servers 목록에 없습니다. 가능한 ID: [{}]",
                    assoc.id,
                    assoc.server_id,
                    server_ids.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }

        for client in &self.clients {
            client.parse_cidr().map_err(|e| format!("Client '{}': {}", client.id, e))?;
        }

        for server in &self.servers {
            if server.port == 0 {
                return Err(format!(
                    "Server '{}': port 0은 유효하지 않습니다.",
                    server.id
                ));
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 테스트
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 기본 설정 빌더: 유효한 TestConfig를 반환한다.
    fn valid_config() -> TestConfig {
        TestConfig::default_single_pair()
    }

    // ── validate() 단위 테스트 ──────────────────────────────────────────────

    #[test]
    fn test_validate_ok() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn test_validate_empty_clients() {
        let mut c = valid_config();
        c.clients.clear();
        let err = c.validate().unwrap_err();
        assert!(err.contains("clients"), "expected 'clients' in: {}", err);
    }

    #[test]
    fn test_validate_empty_servers() {
        let mut c = valid_config();
        c.servers.clear();
        let err = c.validate().unwrap_err();
        assert!(err.contains("servers"), "expected 'servers' in: {}", err);
    }

    #[test]
    fn test_validate_empty_associations() {
        let mut c = valid_config();
        c.associations.clear();
        let err = c.validate().unwrap_err();
        assert!(err.contains("associations"), "expected 'associations' in: {}", err);
    }

    #[test]
    fn test_validate_missing_client_id() {
        let mut c = valid_config();
        c.associations[0].client_id = "nonexistent-client".to_string();
        let err = c.validate().unwrap_err();
        assert!(err.contains("client_id"), "expected 'client_id' in: {}", err);
        assert!(err.contains("nonexistent-client"), "expected bad id in: {}", err);
    }

    #[test]
    fn test_validate_missing_server_id() {
        let mut c = valid_config();
        c.associations[0].server_id = "nonexistent-server".to_string();
        let err = c.validate().unwrap_err();
        assert!(err.contains("server_id"), "expected 'server_id' in: {}", err);
        assert!(err.contains("nonexistent-server"), "expected bad id in: {}", err);
    }

    #[test]
    fn test_validate_bad_cidr() {
        let mut c = valid_config();
        c.clients[0].cidr = "not-a-cidr".to_string();
        let err = c.validate().unwrap_err();
        assert!(err.contains("CIDR") || err.contains("cidr") || err.contains("prefix"),
            "expected CIDR error in: {}", err);
    }

    #[test]
    fn test_validate_port_zero() {
        let mut c = valid_config();
        c.servers[0].port = 0;
        let err = c.validate().unwrap_err();
        assert!(err.contains("port"), "expected 'port' in: {}", err);
    }

    // ── 직렬화 / 역직렬화 왕복 테스트 ──────────────────────────────────────

    #[test]
    fn test_config_serde_roundtrip() {
        let original = valid_config();
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: TestConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(original.id, restored.id);
        assert_eq!(original.name, restored.name);
        assert_eq!(original.test_type, restored.test_type);
        assert_eq!(original.duration_secs, restored.duration_secs);
        assert_eq!(original.clients.len(), restored.clients.len());
        assert_eq!(original.servers.len(), restored.servers.len());
        assert_eq!(original.associations.len(), restored.associations.len());
    }

    #[test]
    fn test_payload_profile_serde_roundtrip() {
        // TCP 페이로드
        let tcp_payload = PayloadProfile::Tcp(TcpPayload { tx_bytes: 64, rx_bytes: 128 });
        let json = serde_json::to_string(&tcp_payload).expect("serialize tcp");
        let restored: PayloadProfile = serde_json::from_str(&json).expect("deserialize tcp");
        match restored {
            PayloadProfile::Tcp(p) => {
                assert_eq!(p.tx_bytes, 64);
                assert_eq!(p.rx_bytes, 128);
            }
            _ => panic!("expected Tcp payload"),
        }

        // HTTP 페이로드
        let http_payload = PayloadProfile::Http(HttpPayload {
            method: HttpMethod::Post,
            path: "/api/test".to_string(),
            request_body_bytes: Some(1024),
            response_body_bytes: Some(2048),
            path_extra_bytes: None,
            h2_max_concurrent_streams: Some(10),
        });
        let json = serde_json::to_string(&http_payload).expect("serialize http");
        let restored: PayloadProfile = serde_json::from_str(&json).expect("deserialize http");
        match restored {
            PayloadProfile::Http(p) => {
                assert_eq!(p.method, HttpMethod::Post);
                assert_eq!(p.path, "/api/test");
                assert_eq!(p.request_body_bytes, Some(1024));
                assert_eq!(p.h2_max_concurrent_streams, Some(10));
            }
            _ => panic!("expected Http payload"),
        }
    }

    #[test]
    fn test_load_config_serde_defaults() {
        // 필드가 없을 때 기본값이 올바르게 적용되는지 확인
        let json = r#"{"ramp_up_secs": 5}"#;
        let load: LoadConfig = serde_json::from_str(json).expect("deserialize partial load");
        assert_eq!(load.ramp_up_secs, 5);
        assert_eq!(load.ramp_down_secs, 0);
        assert!(load.num_connections.is_none());
        assert!(load.connect_timeout_ms.is_none());
    }

    #[test]
    fn test_client_def_parse_cidr() {
        let client = ClientDef {
            id: "c0".to_string(),
            name: "c0".to_string(),
            cidr: "10.10.1.1/24".to_string(),
            count: Some(5),
        };
        let (ip, prefix) = client.parse_cidr().expect("valid cidr");
        assert_eq!(ip.to_string(), "10.10.1.1");
        assert_eq!(prefix, 24);
        assert_eq!(client.effective_count(), 5);
    }

    #[test]
    fn test_client_def_parse_cidr_invalid() {
        let bad_cases = [
            "10.10.1.1",         // prefix 없음
            "10.10.1.999/24",    // 잘못된 IP
            "10.10.1.1/33",      // prefix 범위 초과 (parse::<u8>는 성공하지만 IP 할당 시 오류)
            "abc/24",            // 숫자가 아닌 IP
        ];
        for cidr in bad_cases {
            let client = ClientDef {
                id: "c0".to_string(),
                name: "c0".to_string(),
                cidr: cidr.to_string(),
                count: None,
            };
            assert!(client.parse_cidr().is_err(), "expected error for cidr: {}", cidr);
        }
    }

    #[test]
    fn test_load_config_connections_per_worker() {
        let load = LoadConfig {
            num_connections: Some(100),
            ..LoadConfig::default()
        };
        // 4 워커: ceil(100/4) = 25
        assert_eq!(load.connections_per_worker(4), 25);
        // 7 워커: ceil(100/7) = 15
        assert_eq!(load.connections_per_worker(7), 15);
        // 워커 수 > total: 최소 1
        assert_eq!(load.connections_per_worker(200), 1);
    }
}
