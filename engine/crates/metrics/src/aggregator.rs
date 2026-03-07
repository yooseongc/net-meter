use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use net_meter_core::{MetricsSnapshot, PerProtocolSnapshot};

use crate::Collector;

/// 초당 지표 집계기 (단일 Collector 전용).
///
/// 이전 스냅샷과의 차이를 계산해 CPS/RPS/BW 등 율(rate) 값을 채운다.
pub struct Aggregator {
    collector: Arc<Collector>,
    prev: Option<MetricsSnapshot>,
    prev_time: Instant,
}

impl Aggregator {
    pub fn new(collector: Arc<Collector>) -> Self {
        Self { collector, prev: None, prev_time: Instant::now() }
    }

    /// 현재 수집기 값으로 스냅샷을 생성하고 율을 계산한다.
    pub fn tick(&mut self) -> MetricsSnapshot {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let elapsed = self.prev_time.elapsed().as_secs_f64().max(0.001);
        self.prev_time = Instant::now();

        let mut snap = self.collector.snapshot(now_secs);

        if let Some(ref prev) = self.prev {
            let dt = elapsed;
            snap.cps = snap
                .connections_established
                .saturating_sub(prev.connections_established) as f64
                / dt;
            snap.rps = snap
                .responses_total
                .saturating_sub(prev.responses_total) as f64
                / dt;
            snap.bytes_tx_per_sec =
                snap.bytes_tx_total.saturating_sub(prev.bytes_tx_total) as f64 / dt;
            snap.bytes_rx_per_sec =
                snap.bytes_rx_total.saturating_sub(prev.bytes_rx_total) as f64 / dt;
        }

        self.prev = Some(snap.clone());
        snap
    }
}

/// 다중 프로토콜 집계기.
///
/// 글로벌 Aggregator + 프로토콜별 Collector를 관리한다.
/// `tick()`은 글로벌 rate를 계산하고 by_protocol 누적 스냅샷을 채운다.
pub struct MultiAggregator {
    global: Aggregator,
    protocol_collectors: HashMap<String, Arc<Collector>>,
}

impl MultiAggregator {
    pub fn new(global: Arc<Collector>) -> Self {
        Self {
            global: Aggregator::new(global),
            protocol_collectors: HashMap::new(),
        }
    }

    /// 프로토콜별 Collector 등록 (시험 시작 시 호출)
    pub fn set_protocol_collectors(&mut self, collectors: HashMap<String, Arc<Collector>>) {
        self.protocol_collectors = collectors;
    }

    /// 프로토콜 Collector 초기화
    pub fn clear_protocol_collectors(&mut self) {
        self.protocol_collectors.clear();
    }

    /// 글로벌 + 프로토콜별 스냅샷 생성
    pub fn tick(&mut self) -> MetricsSnapshot {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut snap = self.global.tick();

        // 프로토콜별 누적 스냅샷 (rate 미계산)
        snap.by_protocol = self.protocol_collectors.iter()
            .map(|(proto, collector)| {
                let s = collector.snapshot(now_secs);
                let ps = PerProtocolSnapshot {
                    connections_attempted: s.connections_attempted,
                    connections_established: s.connections_established,
                    connections_failed: s.connections_failed,
                    connections_timed_out: s.connections_timed_out,
                    active_connections: s.active_connections,
                    bytes_tx_total: s.bytes_tx_total,
                    bytes_rx_total: s.bytes_rx_total,
                    requests_total: s.requests_total,
                    responses_total: s.responses_total,
                    status_2xx: s.status_2xx,
                    status_4xx: s.status_4xx,
                    status_5xx: s.status_5xx,
                    latency_mean_ms: s.latency_mean_ms,
                    latency_p99_ms: s.latency_p99_ms,
                };
                (proto.clone(), ps)
            })
            .collect();

        snap
    }
}
