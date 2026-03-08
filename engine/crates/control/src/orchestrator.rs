use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use net_meter_core::{NetworkMode, PayloadProfile, Protocol, TestConfig, TestState};
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

        // TLS server 존재 시 자체 서명 인증서 번들 생성
        let has_tls = config.servers.iter().any(|s| {
            s.tls && matches!(s.protocol, Protocol::Http1 | Protocol::Http2)
        });
        let tls_bundle = if has_tls {
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
            associations = config.associations.len(),
            servers = config.servers.len(),
            clients = config.clients.len(),
            network_mode = ?config.network.mode,
            tls = has_tls,
            "Starting test"
        );
        let _ = state.event_tx.send(TestEvent::TestStarted {
            config_name: config.name.clone(),
            test_type: format!("{:?}", config.test_type).to_lowercase(),
            duration_secs: config.duration_secs,
        });

        match config.network.mode {
            NetworkMode::Namespace => {
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
            }
            NetworkMode::Loopback => {
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
            NetworkMode::ExternalPort => {
                error!("ExternalPort mode is not yet implemented (Phase 11)");
                *state.test_state.write().await = TestState::Failed;
                let _ = state.event_tx.send(TestEvent::Error {
                    message: "ExternalPort mode not implemented (planned for Phase 11)".to_string(),
                });
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
        let tcp_quickack = config.network.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);
        let pair_addrs = config.local_server_addrs();

        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_tls = tls_bundle.map(|b| b.client_config);

        // 각 ServerDef마다 Responder 시작
        for server in &config.servers {
            let bind_ip = server.ip.as_deref().unwrap_or("0.0.0.0");
            let bind_addr: SocketAddr = format!("{}:{}", bind_ip, server.port)
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid server addr: {}", e))?;

            let proto_col = proto_collectors
                .get(&server.protocol)
                .cloned()
                .unwrap_or_else(Collector::new);

            let pair_server_tls = if server.tls { server_tls.clone() } else { None };

            // 이 서버를 참조하는 첫 번째 association에서 payload를 가져옴
            let payload = config.associations.iter()
                .find(|a| a.server_id == server.id)
                .map(|a| a.payload.clone())
                .unwrap_or_else(|| PayloadProfile::default_for(server.protocol));

            self.responder
                .start_server(
                    bind_addr,
                    server.protocol,
                    &payload,
                    Arc::clone(&global),
                    proto_col,
                    tcp_quickack,
                    pair_server_tls,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Responder start failed: {}", e))?;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let empty_client_ips = HashMap::new();
        self.generator
            .start(&config, global, &proto_collectors, &pair_addrs, None, client_tls, &empty_client_ips)
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

        let mut ns = NamespaceManager::new(&config.network.ns.netns_prefix);
        ns.setup().await.map_err(|e| anyhow::anyhow!("{}", e))?;
        let _ = state.event_tx.send(TestEvent::NsSetupComplete);

        let (pair_addrs, server_binds, client_ip_lists) = ns
            .setup_network(&config.clients, &config.servers, &config.associations)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let client_ips: HashMap<String, Vec<String>> = client_ip_lists;

        let client_ns_name = ns.client_ns.clone();
        let server_ns_name = ns.server_ns.clone();
        let tcp_quickack = config.network.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);

        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_tls = tls_bundle.map(|b| b.client_config);

        let server_map = config.server_map();

        for (server_id, bind_addr_str) in &server_binds {
            let server = match server_map.get(server_id) {
                Some(s) => s,
                None => continue,
            };

            let bind_addr: SocketAddr = bind_addr_str
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid bind addr {}: {}", bind_addr_str, e))?;

            let proto_col = proto_collectors
                .get(&server.protocol)
                .cloned()
                .unwrap_or_else(Collector::new);

            let pair_server_tls = if server.tls { server_tls.clone() } else { None };

            let payload = config.associations.iter()
                .find(|a| &a.server_id == server_id)
                .map(|a| a.payload.clone())
                .unwrap_or_else(|| PayloadProfile::default_for(server.protocol));

            self.responder
                .start_server_in_ns(
                    &server_ns_name,
                    bind_addr,
                    server.protocol,
                    &payload,
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
            .start(&config, global, &proto_collectors, &pair_addrs, Some(client_ns_name), client_tls, &client_ips)
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
                tokio::time::sleep(Duration::from_secs(ramp_up_secs)).await;
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
                tokio::time::sleep(Duration::from_secs(duration)).await;
                let current = *state_clone.test_state.read().await;
                if current == TestState::Running || current == TestState::RampingUp {
                    info!("Test duration elapsed, stopping automatically");
                    let mut orch = state_clone.orchestrator.lock().await;
                    orch.stop(Arc::clone(&state_clone)).await;
                }
            });
        }
    }

    /// 시험을 중지한다.
    ///
    /// `ramp_down_secs > 0`이면 RampingDown 상태를 거쳐 delay 후 실제 중지.
    /// 이미 RampingDown이면 즉시 do_stop() 호출.
    pub async fn stop(&mut self, state: Arc<AppState>) {
        let current = *state.test_state.read().await;
        if matches!(current, TestState::Idle | TestState::Completed | TestState::Stopping) {
            return;
        }

        // 이미 RampingDown 중이면 즉시 종료
        if current == TestState::RampingDown {
            self.do_stop(state).await;
            return;
        }

        let ramp_down_secs = state.active_config.read().await
            .as_ref()
            .map(|c| c.default_load.ramp_down_secs)
            .unwrap_or(0);

        if ramp_down_secs > 0 {
            *state.test_state.write().await = TestState::RampingDown;
            info!(ramp_down_secs, "Ramp-down phase starting");
            let _ = state.event_tx.send(TestEvent::RampDownStarted { ramp_down_secs });

            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(ramp_down_secs)).await;
                if *state_clone.test_state.read().await == TestState::RampingDown {
                    let _ = state_clone.event_tx.send(TestEvent::RampDownComplete);
                    let mut orch = state_clone.orchestrator.lock().await;
                    orch.do_stop(Arc::clone(&state_clone)).await;
                }
            });
        } else {
            self.do_stop(state).await;
        }
    }

    /// 실제 종료 처리: generator/responder 중지, NS 정리, 결과 저장.
    async fn do_stop(&mut self, state: Arc<AppState>) {
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
