use serde::{Deserialize, Serialize};

/// 시험 종류: CPS, BW, CC
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

/// HTTP 프로토콜 버전
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Http1,
    Http2,
}

/// HTTP 요청 메서드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// 시험 프로파일: 한 번의 시험에 필요한 모든 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestProfile {
    /// 고유 ID (UUID v4)
    pub id: String,

    /// 사람이 읽을 수 있는 이름
    pub name: String,

    /// 시험 종류
    pub test_type: TestType,

    /// HTTP 프로토콜 버전
    pub protocol: Protocol,

    /// 목표 서버 호스트
    pub target_host: String,

    /// 목표 서버 포트
    pub target_port: u16,

    /// 시험 지속 시간 (초), 0이면 수동 중지까지 계속
    pub duration_secs: u64,

    // --- CPS 전용 ---
    /// 초당 목표 연결 수 (CPS 시험)
    pub target_cps: Option<u64>,

    // --- CC 전용 ---
    /// 목표 동시 연결 수 (CC 시험)
    pub target_cc: Option<u64>,

    // --- BW 전용 ---
    /// 요청 body 크기 (bytes)
    pub request_body_bytes: Option<usize>,

    /// 응답 body 크기 (bytes)
    pub response_body_bytes: Option<usize>,

    // --- HTTP 공통 ---
    pub method: HttpMethod,
    pub path: String,

    // --- 타임아웃 ---
    /// TCP 연결 타임아웃 (ms), None이면 5000ms
    pub connect_timeout_ms: Option<u64>,

    /// 응답 완료 타임아웃 (ms), None이면 30000ms
    pub response_timeout_ms: Option<u64>,

    // --- 동시성 제어 ---
    /// 최대 in-flight 연결 수 (backpressure), None이면 target_cps * 2
    pub max_inflight: Option<u64>,

    // --- 네트워크 네임스페이스 ---
    /// true이면 client/server 네임스페이스를 생성하여 격리 환경에서 시험
    /// root 또는 CAP_NET_ADMIN 권한 필요
    #[serde(default)]
    pub use_namespace: bool,

    /// 네임스페이스 이름 prefix (예: "nm" → "nm-client", "nm-server")
    #[serde(default = "default_netns_prefix")]
    pub netns_prefix: String,

    // --- 서버 TCP 소켓 옵션 ---
    /// true이면 accept한 소켓에 TCP_QUICKACK 설정 → Delayed ACK 비활성화.
    /// false(기본)이면 Linux 기본 동작 (약 40ms Delayed ACK).
    #[serde(default)]
    pub tcp_quickack: bool,

    // --- HTTP 페이로드 크기 조정 ---
    /// 요청 URL path에 추가할 바이트 수 (쿼리 파라미터로 패딩).
    /// None이면 `path` 필드 그대로 사용.
    #[serde(default)]
    pub path_extra_bytes: Option<usize>,

    // --- 가상 Client / Server 수 (Phase 4+) ---
    /// 가상 클라이언트(트래픽 발생기) 수. 현재는 1만 지원, 향후 다중 NS 확장 예정.
    #[serde(default = "default_one")]
    pub num_clients: u32,

    /// 가상 서버(응답기) 수. 현재는 1만 지원, 향후 다중 NS 확장 예정.
    #[serde(default = "default_one")]
    pub num_servers: u32,

    // --- HTTP/2 전용 ---
    /// HTTP/2 연결당 최대 동시 스트림 수 (BW 모드에서 활용).
    /// None이면 10 사용. 서버 SETTINGS_MAX_CONCURRENT_STREAMS를 따름.
    #[serde(default)]
    pub h2_max_concurrent_streams: Option<u32>,
}

fn default_netns_prefix() -> String {
    "nm".to_string()
}

fn default_one() -> u32 {
    1
}

impl Default for TestProfile {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Default Profile".to_string(),
            test_type: TestType::Cps,
            protocol: Protocol::Http1,
            target_host: "127.0.0.1".to_string(),
            target_port: 8080,
            duration_secs: 60,
            target_cps: Some(100),
            target_cc: None,
            request_body_bytes: None,
            response_body_bytes: None,
            method: HttpMethod::Get,
            path: "/".to_string(),
            connect_timeout_ms: None,
            response_timeout_ms: None,
            max_inflight: None,
            use_namespace: false,
            netns_prefix: "nm".to_string(),
            tcp_quickack: false,
            path_extra_bytes: None,
            num_clients: 1,
            num_servers: 1,
            h2_max_concurrent_streams: None,
        }
    }
}
