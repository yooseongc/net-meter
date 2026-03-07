use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use net_meter_core::{PayloadProfile, Protocol, TestConfig, TestState};
use net_meter_generator::Generator;
use net_meter_metrics::Collector;
use net_meter_ns::NamespaceManager;
use net_meter_responder::Responder;
use tracing::{error, info};

use crate::event::TestEvent;
use crate::result::TestResult;
use crate::state::AppState;

/// 시험 생명주기를 관리한다.
///
/// start() → Preparing → Running
/// stop()  → Stopping  → Completed
pub struct Orchestrator {
    generator: Generator,
    responder: Responder,
    ns_manager: Option<NamespaceManager>,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            generator: Generator::new(),
            responder: Responder::new(),
            ns_manager: None,
        }
    }

    /// 시험을 시작한다.
    ///
    /// 1. 프로토콜별 Collector 생성 및 MultiAggregator 등록
    /// 2. TLS pair 존재 시 자체 서명 인증서 생성
    /// 3. NS 모드 또는 로컬 모드로 분기
    pub async fn start(&mut self, config: TestConfig, state: Arc<AppState>) {
        *state.test_state.write().await = TestState::Preparing;
        *state.active_config.write().await = Some(config.clone());
        *state.test_start_time.write().await = Some(Instant::now());

        // 글로벌 + 프로토콜별 Collector 초기화
        state.global_metrics.reset();
        let mut proto_collectors: HashMap<Protocol, Arc<Collector>> = HashMap::new();
        for &proto in &config.active_protocols() {
            let c = Collector::new();
            c.reset();
            proto_collectors.insert(proto, c);
        }

        // MultiAggregator에 등록
        {
            let mut agg = state.aggregator.lock().await;
            agg.set_protocol_collectors(
                proto_collectors
                    .iter()
                    .map(|(p, c)| (p.as_str().to_string(), Arc::clone(c)))
                    .collect(),
            );
        }
        *state.protocol_metrics.write().await = proto_collectors.clone();

        // TLS pair 존재 시 자체 서명 인증서 번들 생성
        let has_tls_pair = config.pairs.iter().any(|p| {
            p.tls && matches!(p.protocol, Protocol::Http1 | Protocol::Http2)
        });
        let tls_bundle = if has_tls_pair {
            match crate::tls::build() {
                Ok(b) => {
                    info!("TLS bundle ready (self-signed cert)");
                    Some(b)
                }
                Err(e) => {
                    error!(error = %e, "Failed to build TLS bundle");
                    *state.test_state.write().await = TestState::Failed;
                    let _ = state.event_tx.send(TestEvent::Error { message: e.to_string() });
                    return;
                }
            }
        } else {
            None
        };

        info!(
            config_name = %config.name,
            pairs = config.pairs.len(),
            use_namespace = config.ns_config.use_namespace,
            tls = has_tls_pair,
            "Starting test"
        );
        let _ = state.event_tx.send(TestEvent::TestStarted {
            config_name: config.name.clone(),
            test_type: format!("{:?}", config.test_type).to_lowercase(),
            duration_secs: config.duration_secs,
        });

        if config.ns_config.use_namespace {
            match self
                .start_ns_mode(config, proto_collectors, Arc::clone(&state), tls_bundle)
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    error!(error = %e, "Failed to start test in namespace mode");
                    *state.test_state.write().await = TestState::Failed;
                    let _ = state.event_tx.send(TestEvent::Error { message: e.to_string() });
                }
            }
        } else {
            match self
                .start_local_mode(config, proto_collectors, Arc::clone(&state), tls_bundle)
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    error!(error = %e, "Failed to start test in local mode");
                    *state.test_state.write().await = TestState::Failed;
                    let _ = state.event_tx.send(TestEvent::Error { message: e.to_string() });
                }
            }
        }
    }

    /// 로컬 모드: namespace 없이 localhost(또는 server.ip)에서 실행.
    async fn start_local_mode(
        &mut self,
        config: TestConfig,
        proto_collectors: HashMap<Protocol, Arc<Collector>>,
        state: Arc<AppState>,
        tls_bundle: Option<crate::tls::TlsBundle>,
    ) -> anyhow::Result<()> {
        let tcp_quickack = config.ns_config.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);
        let pair_addrs = config.local_server_addrs();

        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_tls = tls_bundle.map(|b| b.client_config);

        // 고유 server_id별로 Responder 하나씩 시작
        let mut started: HashSet<String> = HashSet::new();
        for pair in &config.pairs {
            if !started.insert(pair.server.id.clone()) {
                continue;
            }
            let bind_ip = pair.server.ip.as_deref().unwrap_or("0.0.0.0");
            let bind_addr: SocketAddr = format!("{}:{}", bind_ip, pair.server.port)
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid server addr: {}", e))?;

            let proto_col = proto_collectors
                .get(&pair.protocol)
                .cloned()
                .unwrap_or_else(Collector::new);

            // pair.tls가 true일 때만 ServerConfig 전달
            let pair_server_tls = if pair.tls { server_tls.clone() } else { None };

            self.responder
                .start_server(
                    bind_addr,
                    pair.protocol,
                    &pair.payload,
                    Arc::clone(&global),
                    proto_col,
                    tcp_quickack,
                    pair_server_tls,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Responder start failed: {}", e))?;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        self.generator
            .start(&config, global, &proto_collectors, &pair_addrs, None, client_tls)
            .await;

        self.transition_to_running(config, state).await;
        Ok(())
    }

    /// NS 모드: client/server NS를 생성하고 격리된 환경에서 실행.
    async fn start_ns_mode(
        &mut self,
        config: TestConfig,
        proto_collectors: HashMap<Protocol, Arc<Collector>>,
        state: Arc<AppState>,
        tls_bundle: Option<crate::tls::TlsBundle>,
    ) -> anyhow::Result<()> {
        net_meter_ns::check_capability().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut ns = NamespaceManager::new(&config.ns_config.netns_prefix);
        ns.setup().await.map_err(|e| anyhow::anyhow!("{}", e))?;
        let _ = state.event_tx.send(TestEvent::NsSetupComplete);

        let (pair_addrs, server_binds) = ns
            .assign_pair_addrs(&config.pairs)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let client_ns_name = ns.client_ns.clone();
        let server_ns_name = ns.server_ns.clone();
        let tcp_quickack = config.ns_config.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);

        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_tls = tls_bundle.map(|b| b.client_config);

        let mut server_proto_map: HashMap<String, (Protocol, PayloadProfile, bool)> = HashMap::new();
        for pair in &config.pairs {
            server_proto_map
                .entry(pair.server.id.clone())
                .or_insert_with(|| (pair.protocol, pair.payload.clone(), pair.tls));
        }

        for (server_id, bind_addr_str) in &server_binds {
            let bind_addr: SocketAddr = bind_addr_str
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid bind addr {}: {}", bind_addr_str, e))?;

            let (protocol, ref payload, pair_tls) = match server_proto_map.get(server_id) {
                Some(v) => v.clone(),
                None => continue,
            };

            let proto_col = proto_collectors
                .get(&protocol)
                .cloned()
                .unwrap_or_else(Collector::new);

            let pair_server_tls = if pair_tls { server_tls.clone() } else { None };

            self.responder
                .start_server_in_ns(
                    &server_ns_name,
                    bind_addr,
                    protocol,
                    payload,
                    Arc::clone(&global),
                    proto_col,
                    tcp_quickack,
                    pair_server_tls,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Responder NS start failed for {}: {}", server_id, e))?;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        self.generator
            .start(&config, global, &proto_collectors, &pair_addrs, Some(client_ns_name), client_tls)
            .await;

        self.ns_manager = Some(ns);
        self.transition_to_running(config, state).await;
        Ok(())
    }

    /// Running 상태로 전환하고 duration 기반 자동 종료를 등록한다.
    async fn transition_to_running(&self, config: TestConfig, state: Arc<AppState>) {
        let ramp_up_secs = config.default_load.ramp_up_secs;

        if ramp_up_secs > 0 {
            *state.test_state.write().await = TestState::RampingUp;
            info!(ramp_up_secs, "Ramp-up phase starting");
            let _ = state.event_tx.send(TestEvent::RampUpStarted { ramp_up_secs });

            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(ramp_up_secs)).await;
                if *state_clone.test_state.read().await == TestState::RampingUp {
                    *state_clone.test_state.write().await = TestState::Running;
                    info!("Ramp-up complete, running at full speed");
                    let _ = state_clone.event_tx.send(TestEvent::RampUpComplete);
                }
            });
        } else {
            *state.test_state.write().await = TestState::Running;
        }

        info!("Test is running");

        if config.duration_secs > 0 {
            let state_clone = Arc::clone(&state);
            let duration = config.duration_secs;
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(duration)).await;
                let current = *state_clone.test_state.read().await;
                if current == TestState::Running || current == TestState::RampingUp {
                    info!("Test duration elapsed, stopping automatically");
                    let mut orch = state_clone.orchestrator.lock().await;
                    orch.stop(Arc::clone(&state_clone)).await;
                }
            });
        }
    }

    /// 시험을 중지하고 모든 리소스를 정리한다.
    pub async fn stop(&mut self, state: Arc<AppState>) {
        let current = *state.test_state.read().await;
        if current == TestState::Idle || current == TestState::Completed {
            return;
        }

        *state.test_state.write().await = TestState::Stopping;
        info!("Stopping test");

        self.generator.stop().await;
        self.responder.stop_all();

        if let Some(mut ns) = self.ns_manager.take() {
            ns.teardown().await;
            let _ = state.event_tx.send(TestEvent::NsTeardownComplete);
        }

        {
            let mut agg = state.aggregator.lock().await;
            agg.clear_protocol_collectors();
        }
        *state.protocol_metrics.write().await = HashMap::new();

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let start_instant = state.test_start_time.write().await.take();
        let elapsed_secs = start_instant.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let started_at_secs = now_secs.saturating_sub(elapsed_secs);

        let config = state.active_config.read().await.clone();
        let final_snapshot = state.latest_snapshot.read().await.clone();

        if let Some(config) = config {
            let result = TestResult {
                id: uuid::Uuid::new_v4().to_string(),
                config,
                started_at_secs,
                ended_at_secs: now_secs,
                elapsed_secs,
                final_snapshot,
            };
            let mut results = state.test_results.write().await;
            results.insert(0, result);
            if results.len() > 50 {
                results.truncate(50);
            }
        }

        *state.test_start_time.write().await = None;
        *state.test_state.write().await = TestState::Completed;
        info!("Test completed");
        let _ = state.event_tx.send(TestEvent::TestStopped { reason: "completed".to_string() });
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}
