use std::collections::HashMap;
use std::sync::Arc;

use net_meter_core::{MetricsSnapshot, TestProfile, TestState};
use net_meter_metrics::{Aggregator, Collector};
use tokio::sync::{broadcast, Mutex, RwLock};

/// 전역 애플리케이션 상태.
///
/// Arc로 모든 핸들러에 공유된다.
/// 각 필드는 독립적으로 잠금된다 (giant mutex 금지).
pub struct AppState {
    /// 현재 시험 상태
    pub test_state: RwLock<TestState>,

    /// 현재 실행 중인 시험 프로파일
    pub active_profile: RwLock<Option<TestProfile>>,

    /// 저장된 프로파일 목록 (id -> profile)
    pub saved_profiles: RwLock<HashMap<String, TestProfile>>,

    /// lock-free 계측 수집기 (generator/responder가 직접 업데이트)
    pub metrics: Arc<Collector>,

    /// 가장 최근 집계 스냅샷 (1초마다 갱신)
    pub latest_snapshot: RwLock<MetricsSnapshot>,

    /// 실시간 스냅샷 브로드캐스트 (WebSocket 클라이언트에게 전송)
    pub snapshot_tx: broadcast::Sender<MetricsSnapshot>,

    /// Aggregator (Mutex: 순차 접근 보장)
    pub aggregator: Mutex<Aggregator>,

    /// 시험 시작/중지 오케스트레이터 핸들
    pub orchestrator: Mutex<crate::orchestrator::Orchestrator>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let metrics = Collector::new();
        let aggregator = Aggregator::new(Arc::clone(&metrics));
        let (snapshot_tx, _) = broadcast::channel(64);

        Arc::new(Self {
            test_state: RwLock::new(TestState::Idle),
            active_profile: RwLock::new(None),
            saved_profiles: RwLock::new(HashMap::new()),
            metrics,
            latest_snapshot: RwLock::new(MetricsSnapshot::default()),
            snapshot_tx,
            aggregator: Mutex::new(aggregator),
            orchestrator: Mutex::new(crate::orchestrator::Orchestrator::new()),
        })
    }
}
