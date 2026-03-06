use serde::{Deserialize, Serialize};

/// 특정 시점의 계측 지표 스냅샷 (직렬화 가능, 프론트엔드 전송용)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Unix timestamp (초)
    pub timestamp_secs: u64,

    // --- 연결 지표 ---
    pub connections_attempted: u64,
    pub connections_established: u64,
    pub connections_failed: u64,
    pub connections_timed_out: u64,
    pub active_connections: u64,

    // --- 요청/응답 지표 ---
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

    // --- TTFB (ms): 요청 전송 후 첫 바이트 수신까지 ---
    pub ttfb_mean_ms: f64,
    pub ttfb_p99_ms: f64,

    // --- 서버 사이드 계측 (Responder 집계) ---
    pub server_requests: u64,
    pub server_bytes_tx: u64,
}
