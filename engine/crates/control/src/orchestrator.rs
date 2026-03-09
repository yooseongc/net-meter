use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use net_meter_core::{NetworkMode, PayloadProfile, Protocol, TestConfig, TestState};
use net_meter_generator::Generator;
use net_meter_metrics::Collector;
use net_meter_responder::Responder;
use tracing::{error, info};

use crate::event::TestEvent;
use crate::result::TestResult;
use crate::state::AppState;

/// 시험 생명주기를 관리한다.
///
/// start() → Preparing → Running
/// stop()  → Stopping  → Completed
///
/// NS/ExtPort 인프라는 프로그램 시작/종료 시 main에서 관리하며,
/// Orchestrator는 Generator/Responder 생명주기만 담당한다.
pub struct Orchestrator {
    generator: Generator,
    responder: Responder,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            generator: Generator::new(),
            responder: Responder::new(),
        }
    }

    /// 시험을 시작한다.
    pub async fn start(&mut self, config: TestConfig, state: Arc<AppState>) {
        // 설정 검증 — 런타임 오류를 조기에 방지
        if let Err(e) = config.validate() {
            *state.test_state.write().await = TestState::Failed;
            let _ = state.event_tx.send(TestEvent::Error { message: format!("Config validation failed: {}", e) });
            return;
        }

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
            network_mode = ?state.server_net.mode,
            tls = has_tls,
            "Starting test"
        );
        let _ = state.event_tx.send(TestEvent::TestStarted {
            config_name: config.name.clone(),
            test_type: format!("{:?}", config.test_type).to_lowercase(),
            duration_secs: config.duration_secs,
        });

        match state.server_net.mode {
            NetworkMode::Namespace => {
                match self
                    .start_ns_mode(config, proto_collectors, Arc::clone(&state), tls_bundle)
                    .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        error!(error = %e, "Failed to start test in namespace mode");
                        // 부분적으로 시작된 Responder 핸들 정리
                        self.responder.stop_all().await;
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
                        // 부분적으로 시작된 Responder 핸들 정리
                        self.responder.stop_all().await;
                        *state.test_state.write().await = TestState::Failed;
                        let _ = state.event_tx.send(TestEvent::Error { message: e.to_string() });
                    }
                }
            }
            NetworkMode::ExternalPort => {
                match self
                    .start_external_port_mode(config, proto_collectors, Arc::clone(&state), tls_bundle)
                    .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        error!(error = %e, "Failed to start test in external port mode");
                        // 부분적으로 시작된 Responder 핸들 정리
                        self.responder.stop_all().await;
                        // 정책 라우팅이 이미 설정됐을 경우 정리
                        if let Some(ps) = state.ext_policy_routing.lock().await.take() {
                            net_meter_ns::teardown_policy_routing(&ps).await;
                        }
                        *state.test_state.write().await = TestState::Failed;
                        let _ = state.event_tx.send(TestEvent::Error { message: e.to_string() });
                    }
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
        let tcp_quickack = config.network.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);
        let pair_addrs = config.local_server_addrs();

        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_h1_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.client_h1_config));
        let client_h2_tls = tls_bundle.map(|b| Arc::clone(&b.client_h2_config));

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
            .start(&config, global, &proto_collectors, &pair_addrs, None, client_h1_tls, client_h2_tls, &empty_client_ips)
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
        // NS는 프로그램 시작 시 이미 생성되어 있어야 한다.
        let ns_guard = state.ns_manager.lock().await;
        let ns = ns_guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Namespace not initialized. Server may not have started in namespace mode.")
        })?;

        // default_load를 사용하는 association 수로 num_connections를 나눠
        // per-association client IP 수를 계산한다.
        let default_assoc_count = config.associations.iter()
            .filter(|a| a.load.is_none())
            .count()
            .max(1) as u32;
        let total_clients = config.default_load.effective_num_connections() as u32;
        let per_assoc_count = (total_clients / default_assoc_count).max(1);

        // count가 None인 ClientDef에 per_assoc_count를 적용해 IP 할당 수를 결정한다.
        let patched_clients: Vec<net_meter_core::ClientDef> = config.clients.iter().map(|c| {
            if c.count.is_none() {
                net_meter_core::ClientDef { count: Some(per_assoc_count), ..c.clone() }
            } else {
                c.clone()
            }
        }).collect();

        let (pair_addrs, server_binds, client_ip_lists) = ns
            .setup_network(&patched_clients, &config.servers, &config.associations)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let client_ips: HashMap<String, Vec<String>> = client_ip_lists;
        let client_ns_name = ns.client_ns.clone();
        let server_ns_name = ns.server_ns.clone();
        drop(ns_guard); // 락 해제 후 generator/responder 시작

        let tcp_quickack = config.network.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);

        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_h1_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.client_h1_config));
        let client_h2_tls = tls_bundle.map(|b| Arc::clone(&b.client_h2_config));

        let server_map = config.server_map();

        for (server_id, bind_addr_str) in &server_binds {
            let server = match server_map.get(server_id.as_str()) {
                Some(s) => *s,
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
            .start(&config, global, &proto_collectors, &pair_addrs, Some(client_ns_name), client_h1_tls, client_h2_tls, &client_ips)
            .await;

        self.transition_to_running(config, state).await;
        Ok(())
    }

    /// External Port 모드: upper/lower 인터페이스에 IP를 할당하고
    /// Generator는 client IP 바인딩, Responder는 server IP 바인딩으로 실행한다.
    async fn start_external_port_mode(
        &mut self,
        config: TestConfig,
        proto_collectors: HashMap<Protocol, Arc<Collector>>,
        state: Arc<AppState>,
        tls_bundle: Option<crate::tls::TlsBundle>,
    ) -> anyhow::Result<()> {
        let upper_iface = state.server_net.upper_iface.clone();
        let lower_iface = state.server_net.lower_iface.clone();

        // per-assoc client count 계산 (NS 모드와 동일 로직)
        let default_assoc_count = config.associations.iter()
            .filter(|a| a.load.is_none())
            .count()
            .max(1) as u32;
        let total_clients = config.default_load.effective_num_connections() as u32;
        let per_assoc_count = (total_clients / default_assoc_count).max(1);

        let patched_clients: Vec<net_meter_core::ClientDef> = config.clients.iter().map(|c| {
            if c.count.is_none() {
                net_meter_core::ClientDef { count: Some(per_assoc_count), ..c.clone() }
            } else {
                c.clone()
            }
        }).collect();

        // 인터페이스에 IP 할당 + 주소 맵 구성
        let (pair_addrs, server_binds, client_ip_lists) =
            net_meter_ns::assign_ext_port_network(
                &upper_iface,
                &lower_iface,
                &patched_clients,
                &config.servers,
                &config.associations,
                per_assoc_count,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // 정책 라우팅 설정 (DUT proxy ARP 방향 강제)
        let client_cidrs: Vec<String> = patched_clients.iter().map(|c| c.cidr.clone()).collect();
        let server_ips: Vec<String> = server_binds
            .values()
            .filter_map(|addr| addr.split(':').next().map(|s| s.to_string()))
            .collect();
        let policy_state = net_meter_ns::setup_policy_routing(
            &upper_iface,
            &lower_iface,
            &client_cidrs,
            &server_ips,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Policy routing setup failed: {}", e))?;
        *state.ext_policy_routing.lock().await = Some(policy_state);

        let tcp_quickack = config.network.tcp_quickack;
        let global = Arc::clone(&state.global_metrics);
        let server_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.server_config));
        let client_h1_tls = tls_bundle.as_ref().map(|b| Arc::clone(&b.client_h1_config));
        let client_h2_tls = tls_bundle.map(|b| Arc::clone(&b.client_h2_config));
        let server_map = config.server_map();

        // Responder: 각 server IP에 바인딩 (로컬 모드, NS 없음)
        for (server_id, bind_addr_str) in &server_binds {
            let server = match server_map.get(server_id.as_str()) {
                Some(s) => *s,
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
                .start_server(bind_addr, server.protocol, &payload,
                    Arc::clone(&global), proto_col, tcp_quickack, pair_server_tls)
                .await
                .map_err(|e| anyhow::anyhow!("Responder start failed for {}: {}", server_id, e))?;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Generator: NS 없음, client IP 바인딩으로 실행
        self.generator
            .start(&config, global, &proto_collectors, &pair_addrs, None, client_h1_tls, client_h2_tls, &client_ip_lists)
            .await;

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

    /// 실제 종료 처리: generator/responder 중지, 결과 저장.
    /// NS/ExtPort 인프라는 프로그램 종료 시 main에서 teardown한다.
    async fn do_stop(&mut self, state: Arc<AppState>) {
        *state.test_state.write().await = TestState::Stopping;
        info!("Stopping test");

        self.generator.stop().await;
        self.responder.stop_all().await;

        // External Port 모드: 정책 라우팅 정리
        if state.server_net.mode == NetworkMode::ExternalPort {
            if let Some(ps) = state.ext_policy_routing.lock().await.take() {
                net_meter_ns::teardown_policy_routing(&ps).await;
            }
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
