use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Latency 누적 히스토그램 버킷 (Prometheus le 스타일)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistogramBucket {
    /// 상한 (ms). +Inf 버킷은 f64::INFINITY
    pub le_ms: f64,
    /// le_ms 이하 누적 요청 수
    pub count: u64,
}

/// 프로토콜별 누적 카운터 (rate 제외, 합산 집계용)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerProtocolSnapshot {
    pub connections_attempted: u64,
    pub connections_established: u64,
    pub connections_failed: u64,
    pub connections_timed_out: u64,
    pub active_connections: u64,
    pub bytes_tx_total: u64,
    pub bytes_rx_total: u64,
    // HTTP 전용 (TCP에서는 0)
    pub requests_total: u64,
    pub responses_total: u64,
    pub status_2xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub latency_mean_ms: f64,
    pub latency_p99_ms: f64,
}

/// 특정 시점의 계측 지표 스냅샷 (직렬화 가능, 프론트엔드 전송용)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Unix timestamp (초)
    pub timestamp_secs: u64,

    // --- 연결 지표 (전체 합산) ---
    pub connections_attempted: u64,
    pub connections_established: u64,
    pub connections_failed: u64,
    pub connections_timed_out: u64,
    pub active_connections: u64,

    // --- 요청/응답 지표 (HTTP pair 합산) ---
    pub requests_total: u64,
    pub responses_total: u64,
    pub status_2xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub status_other: u64,

    // --- 대역폭 지표 ---
    pub bytes_tx_total: u64,
    pub bytes_rx_total: u64,

    // --- 초당 율 (aggregator가 계산) ---
    pub cps: f64,
    pub rps: f64,
    pub bytes_tx_per_sec: f64,
    pub bytes_rx_per_sec: f64,

    // --- 전체 요청 latency (ms): 연결 시작 ~ 응답 완료 ---
    pub latency_mean_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub latency_max_ms: f64,

    // --- 연결 수립 latency (ms): TCP connect 시간 ---
    pub connect_mean_ms: f64,
    pub connect_p99_ms: f64,

    // --- TTFB (ms): 요청 전송 후 첫 바이트 수신까지 (HTTP 전용) ---
    pub ttfb_mean_ms: f64,
    pub ttfb_p99_ms: f64,

    // --- 서버 사이드 계측 (Responder 집계) ---
    pub server_requests: u64,
    pub server_bytes_tx: u64,

    // --- Latency 히스토그램 버킷 (누적, Prometheus le 스타일) ---
    /// 버킷: [0.5ms, 1ms, 2ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, +Inf]
    pub latency_histogram: Vec<HistogramBucket>,

    // --- 프로토콜별 분리 집계 ---
    /// 키: "tcp", "http1", "http2"
    #[serde(default)]
    pub by_protocol: HashMap<String, PerProtocolSnapshot>,

    // --- HTTP 상태코드 분포 (HTTP pair만 해당) ---
    /// 상태코드 → 누적 응답 수 (예: {200: 5000, 404: 3})
    #[serde(default)]
    pub status_code_breakdown: HashMap<u16, u64>,

    // --- 임계값 위반 ---
    /// 현재 시점에 위반된 임계값 항목 목록 (빈 배열이면 정상)
    #[serde(default)]
    pub threshold_violations: Vec<String>,

    // --- Ramp-up 진행 여부 ---
    #[serde(default)]
    pub is_ramping_up: bool,
}
