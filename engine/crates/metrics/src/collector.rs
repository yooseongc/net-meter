use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hdrhistogram::Histogram;
use net_meter_core::{HistogramBucket, MetricsSnapshot};
use tracing::warn;

/// 히스토그램 버킷 상한 (µs)
const BOUNDS_US: &[u64] = &[500, 1_000, 2_000, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000, 500_000];
/// 위와 동일 (ms)
const BOUNDS_MS: &[f64] = &[0.5, 1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0];

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
    pub server_bytes_rx: AtomicU64,

    // Latency histograms (microseconds)
    /// 전체 요청 latency (connect + send + recv)
    pub latency_hist: Mutex<Histogram<u64>>,
    /// TCP connect latency
    pub connect_hist: Mutex<Histogram<u64>>,
    /// Time To First Byte (request sent → first response byte)
    pub ttfb_hist: Mutex<Histogram<u64>>,

    /// HTTP 상태코드별 응답 수 (per-code breakdown)
    pub status_code_breakdown: Mutex<HashMap<u16, u64>>,
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
            server_bytes_rx: AtomicU64::new(0),
            latency_hist: Mutex::new(new_hist()),
            connect_hist: Mutex::new(new_hist()),
            ttfb_hist: Mutex::new(new_hist()),
            status_code_breakdown: Mutex::new(HashMap::new()),
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
        let mut h = match self.connect_hist.lock() {
            Ok(h) => h,
            Err(e) => {
                warn!("connect_hist lock poisoned — recovering");
                e.into_inner()
            }
        };
        let _ = h.record(us.max(1));
    }

    /// TTFB 기록 (microseconds): 요청 전송 완료 → 첫 응답 바이트
    #[inline]
    pub fn record_ttfb(&self, us: u64) {
        let mut h = match self.ttfb_hist.lock() {
            Ok(h) => h,
            Err(e) => {
                warn!("ttfb_hist lock poisoned — recovering");
                e.into_inner()
            }
        };
        let _ = h.record(us.max(1));
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

    /// 서버가 클라이언트로부터 수신한 바이트 기록 (요청 본문 등)
    #[inline]
    pub fn record_server_rx(&self, bytes: u64) {
        self.server_bytes_rx.fetch_add(bytes, Ordering::Relaxed);
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

        if status > 0 {
            let mut map = match self.status_code_breakdown.lock() {
                Ok(m) => m,
                Err(e) => {
                    warn!("status_code_breakdown lock poisoned — recovering");
                    e.into_inner()
                }
            };
            *map.entry(status).or_insert(0) += 1;
        }

        let mut h = match self.latency_hist.lock() {
            Ok(h) => h,
            Err(e) => {
                warn!("latency_hist lock poisoned — recovering");
                e.into_inner()
            }
        };
        let _ = h.record(latency_us.max(1));
    }

    /// 현재 누적값으로 MetricsSnapshot 생성
    pub fn snapshot(&self, timestamp_secs: u64) -> MetricsSnapshot {
        // Histogram 읽기 (각 lock을 짧게 유지)
        let (lat_mean, lat_p50, lat_p95, lat_p99, lat_max) = read_hist(&self.latency_hist);
        let (conn_mean, _, _, conn_p99, _) = read_hist(&self.connect_hist);
        let (ttfb_mean, _, _, ttfb_p99, _) = read_hist(&self.ttfb_hist);
        let latency_histogram = extract_buckets(&self.latency_hist);

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
            server_bytes_rx: self.server_bytes_rx.load(Ordering::Relaxed),
            latency_mean_ms: lat_mean,
            latency_p50_ms: lat_p50,
            latency_p95_ms: lat_p95,
            latency_p99_ms: lat_p99,
            latency_max_ms: lat_max,
            connect_mean_ms: conn_mean,
            connect_p99_ms: conn_p99,
            ttfb_mean_ms: ttfb_mean,
            ttfb_p99_ms: ttfb_p99,
            latency_histogram,
            status_code_breakdown: self
                .status_code_breakdown
                .lock()
                .map(|m| m.clone())
                .unwrap_or_default(),
            // 율(rate)은 Aggregator가 채움
            cps: 0.0,
            rps: 0.0,
            bytes_tx_per_sec: 0.0,
            bytes_rx_per_sec: 0.0,
            by_protocol: std::collections::HashMap::new(),
            // 임계값/ramp-up은 main 루프에서 채움
            threshold_violations: Vec::new(),
            is_ramping_up: false,
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
            &self.server_bytes_rx,
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
        if let Ok(mut m) = self.status_code_breakdown.lock() {
            m.clear();
        }
    }
}

/// 활성 연결 수를 추적하는 RAII 가드.
///
/// `record_connection_established()` 직후 생성해 스코프에 묶어둔다.
/// 정상 종료든 태스크 abort(future drop)든 관계없이 `record_connection_closed()`를
/// 반드시 호출해 active_connections 카운터를 정확히 유지한다.
pub struct ActiveConnectionGuard {
    global: Arc<Collector>,
    proto: Arc<Collector>,
}

impl ActiveConnectionGuard {
    pub fn new(global: Arc<Collector>, proto: Arc<Collector>) -> Self {
        Self { global, proto }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.global.record_connection_closed();
        self.proto.record_connection_closed();
    }
}

impl Default for Collector {
    fn default() -> Self {
        Arc::try_unwrap(Self::new()).unwrap_or_else(|_| panic!("arc unwrap"))
    }
}

/// Histogram에서 (mean, p50, p95, p99, max) ms 추출
fn read_hist(hist: &Mutex<Histogram<u64>>) -> (f64, f64, f64, f64, f64) {
    let h = match hist.lock() {
        Ok(h) => h,
        Err(e) => {
            warn!("histogram lock poisoned during snapshot — recovering");
            e.into_inner()
        }
    };
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
}

/// Histogram에서 누적 버킷 벡터 추출 (Prometheus le 스타일)
///
/// 각 버킷은 해당 le_ms 이하의 누적 카운트를 담는다.
/// BOUNDS_US 기준으로 버킷을 분류한 뒤 prefix-sum으로 누적값을 계산한다.
fn extract_buckets(hist: &Mutex<Histogram<u64>>) -> Vec<HistogramBucket> {
    let n = BOUNDS_US.len();
    let mut counts = vec![0u64; n + 1]; // [각 bound 버킷] + [+Inf 버킷]

    let h = match hist.lock() {
        Ok(h) => h,
        Err(e) => {
            warn!("histogram lock poisoned during bucket extraction — recovering");
            e.into_inner()
        }
    };
    if h.len() > 0 {
        for v in h.iter_recorded() {
            let val = v.value_iterated_to();
            let cnt = v.count_at_value();
            // val이 속하는 버킷 인덱스: 처음으로 bound >= val 인 위치
            let idx = BOUNDS_US.partition_point(|&b| b < val);
            counts[idx.min(n)] += cnt;
        }
        // prefix-sum → 누적 카운트
        for i in 1..=n {
            counts[i] += counts[i - 1];
        }
    }

    let mut result: Vec<HistogramBucket> = BOUNDS_MS
        .iter()
        .zip(counts.iter())
        .map(|(&le_ms, &count)| HistogramBucket { le_ms, count })
        .collect();
    result.push(HistogramBucket { le_ms: f64::INFINITY, count: counts[n] });
    result
}

// ---------------------------------------------------------------------------
// 테스트
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_basic_counters() {
        let c = Collector::new();
        c.record_connection_attempt();
        c.record_connection_attempt();
        c.record_connection_established();
        assert_eq!(c.connections_attempted.load(Ordering::Relaxed), 2);
        assert_eq!(c.connections_established.load(Ordering::Relaxed), 1);
        assert_eq!(c.active_connections.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_active_connection_guard_decrements_on_drop() {
        let c = Collector::new();
        c.record_connection_established();
        c.record_connection_established();
        assert_eq!(c.active_connections.load(Ordering::Relaxed), 2);
        {
            let _g1 = ActiveConnectionGuard::new(Arc::clone(&c), Arc::clone(&c));
            let _g2 = ActiveConnectionGuard::new(Arc::clone(&c), Arc::clone(&c));
            // _g1, _g2 각각 record_connection_established()를 호출하지 않으므로
            // active_connections는 여기서도 2이다.
            // Guard는 drop 시에만 감소시킨다.
        }
        // 두 가드가 drop되어 2회 감소 → 0
        assert_eq!(c.active_connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_response_status_categorization() {
        let c = Collector::new();
        c.record_response(200, 1024, 500);
        c.record_response(201, 512, 300);
        c.record_response(404, 0, 100);
        c.record_response(500, 0, 50);
        c.record_response(0, 64, 200);   // TCP (status=0)
        assert_eq!(c.status_2xx.load(Ordering::Relaxed), 2);
        assert_eq!(c.status_4xx.load(Ordering::Relaxed), 1);
        assert_eq!(c.status_5xx.load(Ordering::Relaxed), 1);
        assert_eq!(c.status_other.load(Ordering::Relaxed), 1); // status=0
        assert_eq!(c.responses_total.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_status_code_breakdown() {
        let c = Collector::new();
        c.record_response(200, 0, 0);
        c.record_response(200, 0, 0);
        c.record_response(404, 0, 0);
        let map = c.status_code_breakdown.lock().unwrap();
        assert_eq!(*map.get(&200).unwrap_or(&0), 2);
        assert_eq!(*map.get(&404).unwrap_or(&0), 1);
        assert!(!map.contains_key(&0)); // TCP(status=0)은 breakdown에 기록 안 됨
    }

    #[test]
    fn test_histogram_latency_recording() {
        let c = Collector::new();
        // 1ms = 1000us
        c.record_connect_latency(1_000);
        c.record_ttfb(2_000);
        c.record_response(200, 0, 5_000);

        let snap = c.snapshot(0);
        // 1회씩 기록했으므로 mean ≈ 기록값
        assert!(snap.connect_mean_ms > 0.0);
        assert!(snap.ttfb_mean_ms > 0.0);
        assert!(snap.latency_mean_ms > 0.0);
    }

    #[test]
    fn test_histogram_buckets_cumulative() {
        let c = Collector::new();
        // 0.5ms (500us) 경계 이하 기록
        c.record_response(200, 0, 400); // 0.4ms — ≤0.5ms 버킷에 들어감
        c.record_response(200, 0, 1_500); // 1.5ms — ≤2ms 버킷에 들어감

        let snap = c.snapshot(0);
        // 버킷은 누적이므로 ≤1ms 버킷 count >= ≤0.5ms 버킷 count
        let buckets = &snap.latency_histogram;
        assert!(!buckets.is_empty());
        // 마지막 +Inf 버킷 = 총 기록 수
        let inf_bucket = buckets.last().unwrap();
        assert_eq!(inf_bucket.count, 2);
        // 누적 단조 증가 확인
        for w in buckets.windows(2) {
            assert!(w[1].count >= w[0].count, "buckets must be cumulative");
        }
    }

    #[test]
    fn test_reset_clears_all() {
        let c = Collector::new();
        c.record_connection_attempt();
        c.record_response(200, 1024, 500);
        c.reset();
        assert_eq!(c.connections_attempted.load(Ordering::Relaxed), 0);
        assert_eq!(c.responses_total.load(Ordering::Relaxed), 0);
        assert_eq!(c.bytes_rx.load(Ordering::Relaxed), 0);
        let snap = c.snapshot(0);
        assert_eq!(snap.latency_mean_ms, 0.0);
    }

    #[test]
    fn test_bytes_accounting() {
        let c = Collector::new();
        c.record_request(512);
        c.record_request(512);
        c.record_response(200, 1024, 100);
        assert_eq!(c.bytes_tx.load(Ordering::Relaxed), 1024);
        assert_eq!(c.bytes_rx.load(Ordering::Relaxed), 1024);
        assert_eq!(c.requests_total.load(Ordering::Relaxed), 2);
    }
}
