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
    /// 0이면 connect/disconnect만 수행 (순수 CPS 측정)
    #[serde(default)]
    pub tx_bytes: usize,
    /// 서버가 응답할 바이트 수 (0이면 응답 없음)
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
    /// 요청 body 크기 (bytes). None이면 body 없음.
    #[serde(default)]
    pub request_body_bytes: Option<usize>,
    /// 응답 body 크기 (bytes). None이면 응답 body 없음.
    #[serde(default)]
    pub response_body_bytes: Option<usize>,
    /// URL 쿼리 파라미터로 추가할 패딩 바이트 수
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
// 부하 설정
// ---------------------------------------------------------------------------

/// 클라이언트 부하 파라미터. pair가 None으로 두면 TestConfig.default_load를 사용.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadConfig {
    /// [CPS] 초당 목표 연결 수
    #[serde(default)]
    pub target_cps: Option<u64>,
    /// [CC/BW] 목표 동시 연결 수
    #[serde(default)]
    pub target_cc: Option<u64>,
    /// [CPS] 최대 in-flight 연결 수. None이면 target_cps × 2.
    #[serde(default)]
    pub max_inflight: Option<u64>,
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
            target_cps: Some(100),
            target_cc: None,
            max_inflight: None,
            connect_timeout_ms: Some(5000),
            response_timeout_ms: Some(30000),
            ramp_up_secs: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// 임계값 / 알람 설정
// ---------------------------------------------------------------------------

/// 시험 Pass/Fail 임계값. 위반 시 대시보드 경고 또는 자동 중단.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Thresholds {
    /// 최소 CPS 기준 (이 값 미만이면 위반)
    #[serde(default)]
    pub min_cps: Option<f64>,
    /// 최대 에러율 % 기준 (connections_failed / connections_attempted × 100)
    #[serde(default)]
    pub max_error_rate_pct: Option<f64>,
    /// 최대 latency p99 (ms) 기준
    #[serde(default)]
    pub max_latency_p99_ms: Option<f64>,
    /// 위반 감지 시 시험 자동 중단
    #[serde(default)]
    pub auto_stop_on_fail: bool,
}

impl LoadConfig {
    pub fn effective_cps(&self) -> u64 {
        self.target_cps.unwrap_or(100).max(1)
    }
    pub fn effective_cc(&self) -> u64 {
        self.target_cc.unwrap_or(50)
    }
    pub fn effective_max_inflight(&self) -> u64 {
        let cps = self.effective_cps();
        self.max_inflight.unwrap_or(cps.saturating_mul(2).min(65535))
    }
    pub fn connect_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.connect_timeout_ms.unwrap_or(5000))
    }
    pub fn response_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.response_timeout_ms.unwrap_or(30000))
    }
}

// ---------------------------------------------------------------------------
// 엔드포인트
// ---------------------------------------------------------------------------

/// 클라이언트 엔드포인트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEndpoint {
    pub id: String,
    /// NS 모드에서 사용할 IP. None이면 자동 할당 (10.10.1.{N}).
    /// 로컬 모드에서는 무시된다.
    #[serde(default)]
    pub ip: Option<String>,
}

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
// PairConfig — 클라이언트-서버 쌍
// ---------------------------------------------------------------------------

/// 하나의 클라이언트-서버 트래픽 쌍 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairConfig {
    /// 식별자
    pub id: String,
    /// 클라이언트 엔드포인트
    pub client: ClientEndpoint,
    /// 서버 엔드포인트
    pub server: ServerEndpoint,
    /// 사용할 프로토콜
    pub protocol: Protocol,
    /// 프로토콜별 페이로드 설정
    pub payload: PayloadProfile,
    /// pair별 부하 설정. None이면 TestConfig.default_load 사용.
    #[serde(default)]
    pub load: Option<LoadConfig>,
    /// 이 pair에 대해 병렬로 실행할 클라이언트 워커 수 (기본 1)
    #[serde(default = "default_one")]
    pub client_count: u32,
}

fn default_one() -> u32 { 1 }

impl PairConfig {
    /// 유효 부하 설정 반환 (pair 개별 설정 > 글로벌 기본값)
    pub fn effective_load<'a>(&'a self, default: &'a LoadConfig) -> &'a LoadConfig {
        self.load.as_ref().unwrap_or(default)
    }
}

// ---------------------------------------------------------------------------
// NsConfig — 네트워크 네임스페이스 설정
// ---------------------------------------------------------------------------

/// 네트워크 네임스페이스 및 소켓 옵션 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NsConfig {
    /// true이면 client/server 네임스페이스에서 시험 (CAP_NET_ADMIN 필요)
    #[serde(default)]
    pub use_namespace: bool,
    /// 네임스페이스 이름 prefix (예: "nm" → "nm-client", "nm-server")
    #[serde(default = "default_ns_prefix")]
    pub netns_prefix: String,
    /// accept된 소켓에 TCP_QUICKACK 설정 (Delayed ACK 비활성화)
    #[serde(default)]
    pub tcp_quickack: bool,
}

fn default_ns_prefix() -> String {
    "nm".to_string()
}

impl Default for NsConfig {
    fn default() -> Self {
        Self {
            use_namespace: false,
            netns_prefix: "nm".to_string(),
            tcp_quickack: false,
        }
    }
}

// ---------------------------------------------------------------------------
// TestConfig — 전체 시험 설정
// ---------------------------------------------------------------------------

/// 전체 시험 설정. 하나 이상의 클라이언트-서버 pair를 정의한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// 고유 ID (UUID v4)
    pub id: String,
    /// 사람이 읽을 수 있는 이름
    pub name: String,
    /// 시험 종류 (모든 pair에 적용)
    pub test_type: TestType,
    /// 시험 지속 시간 (초). 0이면 수동 중지까지 계속.
    pub duration_secs: u64,
    /// 글로벌 기본 부하 설정 (pair가 override 가능)
    pub default_load: LoadConfig,
    /// 클라이언트-서버 pair 목록 (1개 이상 필수)
    pub pairs: Vec<PairConfig>,
    /// 네트워크/NS 설정
    pub ns_config: NsConfig,
    /// 임계값 / 알람 설정 (선택)
    #[serde(default)]
    pub thresholds: Thresholds,
}

impl TestConfig {
    /// 단일 HTTP/1.1 pair의 기본 설정
    pub fn default_single_pair() -> Self {
        let pair = PairConfig {
            id: uuid::Uuid::new_v4().to_string(),
            client: ClientEndpoint { id: "client-0".to_string(), ip: None },
            server: ServerEndpoint { id: "server-0".to_string(), ip: None, port: 8080 },
            protocol: Protocol::Http1,
            payload: PayloadProfile::Http(HttpPayload::default()),
            load: None,
            client_count: 1,
        };
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Default Config".to_string(),
            test_type: TestType::Cps,
            duration_secs: 60,
            default_load: LoadConfig::default(),
            pairs: vec![pair],
            ns_config: NsConfig::default(),
            thresholds: Thresholds::default(),
        }
    }

    /// NS 모드에서 사용할 프로토콜 집합 (메트릭 컬렉터 생성에 활용)
    pub fn active_protocols(&self) -> Vec<Protocol> {
        let mut seen = std::collections::HashSet::new();
        self.pairs.iter()
            .filter(|p| seen.insert(p.protocol))
            .map(|p| p.protocol)
            .collect()
    }

    /// pair별 서버 주소 맵 반환 (pair_id → "host:port")
    /// 로컬 모드용: NS 모드에서는 오케스트레이터가 별도 계산
    pub fn local_server_addrs(&self) -> HashMap<String, String> {
        self.pairs.iter().map(|p| {
            let host = p.server.ip.as_deref().unwrap_or("127.0.0.1");
            (p.id.clone(), format!("{}:{}", host, p.server.port))
        }).collect()
    }
}
