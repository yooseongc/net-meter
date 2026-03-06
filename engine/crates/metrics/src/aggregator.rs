use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use net_meter_core::MetricsSnapshot;

use crate::Collector;

/// 초당 지표 집계기.
///
/// 이전 스냅샷과의 차이를 계산해 CPS/RPS/BW 등 율(rate) 값을 채운다.
/// Control 바이너리의 백그라운드 태스크에서 1초 간격으로 호출된다.
pub struct Aggregator {
    collector: Arc<Collector>,
    prev: Option<MetricsSnapshot>,
    prev_time: Instant,
}

impl Aggregator {
    pub fn new(collector: Arc<Collector>) -> Self {
        Self {
            collector,
            prev: None,
            prev_time: Instant::now(),
        }
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
