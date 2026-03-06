use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hdrhistogram::Histogram;
use net_meter_core::MetricsSnapshot;

/// 1µs ~ 60s 범위, 3자리 유효숫자
fn new_hist() -> Histogram<u64> {
    Histogram::new_with_bounds(1, 60_000_000, 3).expect("valid histogram bounds")
}

/// lock-free 원자 카운터 + hdrhistogram 기반 계측 수집기.
///
/// 원자 카운터: 연결/요청/응답/대역폭 → `Relaxed` ordering으로 hot path에서 최소 오버헤드.
/// hdrhistogram: latency percentile (p50/p95/p99) → `Mutex`로 보호.
///   - 5만 CPS 이하에서는 Mutex 경합이 무시할 수 있는 수준.
///   - 그 이상 필요 시 per-thread histogram + merge 방식으로 전환 예정.
pub struct Collector {
    // 연결
    pub connections_attempted: AtomicU64,
    pub connections_established: AtomicU64,
    pub connections_failed: AtomicU64,
    pub connections_timed_out: AtomicU64,
    pub active_connections: AtomicU64,

    // 요청/응답
    pub requests_total: AtomicU64,
    pub responses_total: AtomicU64,
    pub status_2xx: AtomicU64,
    pub status_4xx: AtomicU64,
    pub status_5xx: AtomicU64,
    pub status_other: AtomicU64,

    // 대역폭
    pub bytes_tx: AtomicU64,
    pub bytes_rx: AtomicU64,

    // 서버 사이드 (Responder)
    pub server_requests: AtomicU64,
    pub server_bytes_tx: AtomicU64,

    // Latency histograms (microseconds)
    /// 전체 요청 latency (connect + send + recv)
    pub latency_hist: Mutex<Histogram<u64>>,
    /// TCP connect latency
    pub connect_hist: Mutex<Histogram<u64>>,
    /// Time To First Byte (request sent → first response byte)
    pub ttfb_hist: Mutex<Histogram<u64>>,
}

impl Collector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            connections_attempted: AtomicU64::new(0),
            connections_established: AtomicU64::new(0),
            connections_failed: AtomicU64::new(0),
            connections_timed_out: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            requests_total: AtomicU64::new(0),
            responses_total: AtomicU64::new(0),
            status_2xx: AtomicU64::new(0),
            status_4xx: AtomicU64::new(0),
            status_5xx: AtomicU64::new(0),
            status_other: AtomicU64::new(0),
            bytes_tx: AtomicU64::new(0),
            bytes_rx: AtomicU64::new(0),
            server_requests: AtomicU64::new(0),
            server_bytes_tx: AtomicU64::new(0),
            latency_hist: Mutex::new(new_hist()),
            connect_hist: Mutex::new(new_hist()),
            ttfb_hist: Mutex::new(new_hist()),
        })
    }

    #[inline]
    pub fn record_connection_attempt(&self) {
        self.connections_attempted.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_connection_established(&self) {
        self.connections_established.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_connection_failed(&self) {
        self.connections_failed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_timeout(&self) {
        self.connections_timed_out.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_connection_closed(&self) {
        self.active_connections
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            })
            .ok();
    }

    /// TCP connect latency 기록 (microseconds)
    #[inline]
    pub fn record_connect_latency(&self, us: u64) {
        if let Ok(mut h) = self.connect_hist.lock() {
            let _ = h.record(us.max(1));
        }
    }

    /// TTFB 기록 (microseconds): 요청 전송 완료 → 첫 응답 바이트
    #[inline]
    pub fn record_ttfb(&self, us: u64) {
        if let Ok(mut h) = self.ttfb_hist.lock() {
            let _ = h.record(us.max(1));
        }
    }

    #[inline]
    pub fn record_request(&self, bytes: u64) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.bytes_tx.fetch_add(bytes, Ordering::Relaxed);
    }

    /// 서버 수신 요청 기록 (Responder 호출)
    #[inline]
    pub fn record_server_request(&self, bytes_tx: u64) {
        self.server_requests.fetch_add(1, Ordering::Relaxed);
        self.server_bytes_tx.fetch_add(bytes_tx, Ordering::Relaxed);
    }

    /// 응답 완료 기록. latency_us는 연결 시작부터 응답 완료까지의 전체 시간.
    #[inline]
    pub fn record_response(&self, status: u16, bytes: u64, latency_us: u64) {
        self.responses_total.fetch_add(1, Ordering::Relaxed);
        self.bytes_rx.fetch_add(bytes, Ordering::Relaxed);

        match status {
            200..=299 => self.status_2xx.fetch_add(1, Ordering::Relaxed),
            400..=499 => self.status_4xx.fetch_add(1, Ordering::Relaxed),
            500..=599 => self.status_5xx.fetch_add(1, Ordering::Relaxed),
            _ => self.status_other.fetch_add(1, Ordering::Relaxed),
        };

        if let Ok(mut h) = self.latency_hist.lock() {
            let _ = h.record(latency_us.max(1));
        }
    }

    /// 현재 누적값으로 MetricsSnapshot 생성
    pub fn snapshot(&self, timestamp_secs: u64) -> MetricsSnapshot {
        // Histogram 읽기 (각 lock을 짧게 유지)
        let (lat_mean, lat_p50, lat_p95, lat_p99, lat_max) = read_hist(&self.latency_hist);
        let (conn_mean, _, _, conn_p99, _) = read_hist(&self.connect_hist);
        let (ttfb_mean, _, _, ttfb_p99, _) = read_hist(&self.ttfb_hist);

        MetricsSnapshot {
            timestamp_secs,
            connections_attempted: self.connections_attempted.load(Ordering::Relaxed),
            connections_established: self.connections_established.load(Ordering::Relaxed),
            connections_failed: self.connections_failed.load(Ordering::Relaxed),
            connections_timed_out: self.connections_timed_out.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            responses_total: self.responses_total.load(Ordering::Relaxed),
            status_2xx: self.status_2xx.load(Ordering::Relaxed),
            status_4xx: self.status_4xx.load(Ordering::Relaxed),
            status_5xx: self.status_5xx.load(Ordering::Relaxed),
            status_other: self.status_other.load(Ordering::Relaxed),
            bytes_tx_total: self.bytes_tx.load(Ordering::Relaxed),
            bytes_rx_total: self.bytes_rx.load(Ordering::Relaxed),
            server_requests: self.server_requests.load(Ordering::Relaxed),
            server_bytes_tx: self.server_bytes_tx.load(Ordering::Relaxed),
            latency_mean_ms: lat_mean,
            latency_p50_ms: lat_p50,
            latency_p95_ms: lat_p95,
            latency_p99_ms: lat_p99,
            latency_max_ms: lat_max,
            connect_mean_ms: conn_mean,
            connect_p99_ms: conn_p99,
            ttfb_mean_ms: ttfb_mean,
            ttfb_p99_ms: ttfb_p99,
            // 율(rate)은 Aggregator가 채움
            cps: 0.0,
            rps: 0.0,
            bytes_tx_per_sec: 0.0,
            bytes_rx_per_sec: 0.0,
        }
    }

    /// 카운터 전체 초기화 (시험 시작 시 호출)
    pub fn reset(&self) {
        for counter in [
            &self.connections_attempted,
            &self.connections_established,
            &self.connections_failed,
            &self.connections_timed_out,
            &self.active_connections,
            &self.requests_total,
            &self.responses_total,
            &self.status_2xx,
            &self.status_4xx,
            &self.status_5xx,
            &self.status_other,
            &self.bytes_tx,
            &self.bytes_rx,
            &self.server_requests,
            &self.server_bytes_tx,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
        if let Ok(mut h) = self.latency_hist.lock() {
            h.reset();
        }
        if let Ok(mut h) = self.connect_hist.lock() {
            h.reset();
        }
        if let Ok(mut h) = self.ttfb_hist.lock() {
            h.reset();
        }
    }
}

impl Default for Collector {
    fn default() -> Self {
        Arc::try_unwrap(Self::new()).unwrap_or_else(|_| panic!("arc unwrap"))
    }
}

/// Histogram에서 (mean, p50, p95, p99, max) ms 추출
fn read_hist(hist: &Mutex<Histogram<u64>>) -> (f64, f64, f64, f64, f64) {
    if let Ok(h) = hist.lock() {
        if h.len() == 0 {
            return (0.0, 0.0, 0.0, 0.0, 0.0);
        }
        (
            h.mean() / 1000.0,
            h.value_at_quantile(0.50) as f64 / 1000.0,
            h.value_at_quantile(0.95) as f64 / 1000.0,
            h.value_at_quantile(0.99) as f64 / 1000.0,
            h.max() as f64 / 1000.0,
        )
    } else {
        (0.0, 0.0, 0.0, 0.0, 0.0)
    }
}
