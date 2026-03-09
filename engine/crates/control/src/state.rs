use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use net_meter_core::{MetricsSnapshot, NetworkMode, Protocol, TestConfig, TestState};
use net_meter_metrics::{Collector, MultiAggregator};
use net_meter_ns::{ExternalPortState, NamespaceManager, PolicyRoutingState};
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::event::TestEvent;
use crate::result::TestResult;

/// 서버 시작 시 CLI 옵션으로 결정되는 네트워크 설정 (불변).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ServerNetConfig {
    pub mode: NetworkMode,
    pub upper_iface: String,
    pub lower_iface: String,
    pub mtu: u16,
    pub ns_prefix: String,
}

impl Default for ServerNetConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Loopback,
            upper_iface: "veth-c0".to_string(),
            lower_iface: "veth-s0".to_string(),
            mtu: 1500,
            ns_prefix: "nm".to_string(),
        }
    }
}

/// 전역 애플리케이션 상태.
///
/// Arc로 모든 핸들러에 공유된다.
/// 각 필드는 독립적으로 잠금된다 (giant mutex 금지).
pub struct AppState {
    /// 서버 네트워크 모드 (CLI로 결정, 불변)
    pub server_net: ServerNetConfig,

    /// 현재 시험 상태
    pub test_state: RwLock<TestState>,

    /// 현재 실행 중인 시험 설정
    pub active_config: RwLock<Option<TestConfig>>,

    /// 저장된 시험 설정 목록 (id → config)
    pub saved_configs: RwLock<HashMap<String, TestConfig>>,

    /// 글로벌 lock-free 계측 수집기
    pub global_metrics: Arc<Collector>,

    /// 프로토콜별 계측 수집기 (시험 중에만 존재)
    pub protocol_metrics: RwLock<HashMap<Protocol, Arc<Collector>>>,

    /// 가장 최근 집계 스냅샷 (1초마다 갱신)
    pub latest_snapshot: RwLock<MetricsSnapshot>,

    /// 실시간 스냅샷 브로드캐스트 (WebSocket 클라이언트에게 전송)
    pub snapshot_tx: broadcast::Sender<MetricsSnapshot>,

    /// 실시간 이벤트 브로드캐스트 (SSE 클라이언트에게 전송)
    pub event_tx: broadcast::Sender<TestEvent>,

    /// MultiAggregator (Mutex: 순차 접근 보장)
    pub aggregator: Mutex<MultiAggregator>,

    /// 시험 시작/중지 오케스트레이터 핸들
    pub orchestrator: Mutex<crate::orchestrator::Orchestrator>,

    /// 시험 시작 시각 (elapsed 계산용)
    pub test_start_time: RwLock<Option<Instant>>,

    /// 완료된 시험 결과 목록 (최신 순)
    pub test_results: RwLock<Vec<TestResult>>,

    /// Namespace 모드: 프로그램 시작 시 생성, 종료 시 teardown
    pub ns_manager: Mutex<Option<NamespaceManager>>,

    /// External Port 모드: 프로그램 시작 시 설정, 종료 시 복원
    pub ext_port_state: Mutex<Option<ExternalPortState>>,

    /// External Port 모드 정책 라우팅 상태 (시험 시작 시 설정, 종료 시 정리)
    pub ext_policy_routing: Mutex<Option<PolicyRoutingState>>,
}

impl AppState {
    pub fn new(server_net: ServerNetConfig) -> Arc<Self> {
        let global_metrics = Collector::new();
        let aggregator = MultiAggregator::new(Arc::clone(&global_metrics));
        let (snapshot_tx, _) = broadcast::channel(256);
        let (event_tx, _) = broadcast::channel(512);

        Arc::new(Self {
            server_net,
            test_state: RwLock::new(TestState::Idle),
            active_config: RwLock::new(None),
            saved_configs: RwLock::new(HashMap::new()),
            global_metrics,
            protocol_metrics: RwLock::new(HashMap::new()),
            latest_snapshot: RwLock::new(MetricsSnapshot::default()),
            snapshot_tx,
            event_tx,
            aggregator: Mutex::new(aggregator),
            orchestrator: Mutex::new(crate::orchestrator::Orchestrator::new()),
            test_start_time: RwLock::new(None),
            test_results: RwLock::new(Vec::new()),
            ns_manager: Mutex::new(None),
            ext_port_state: Mutex::new(None),
            ext_policy_routing: Mutex::new(None),
        })
    }
}
