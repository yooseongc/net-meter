use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use net_meter_core::{TestProfile, TestState};
use net_meter_generator::Generator;
use net_meter_metrics::Collector;
use net_meter_ns::NamespaceManager;
use net_meter_responder::Responder;
use tracing::{error, info};

use crate::result::TestResult;

use crate::state::AppState;

/// 시험 생명주기를 관리한다.
///
/// start() -> Preparing -> Running
/// stop()  -> Stopping  -> Completed
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
    pub async fn start(
        &mut self,
        profile: TestProfile,
        metrics: Arc<Collector>,
        state: Arc<AppState>,
    ) {
        *state.test_state.write().await = TestState::Preparing;
        *state.active_profile.write().await = Some(profile.clone());
        *state.test_start_time.write().await = Some(Instant::now());
        metrics.reset();

        info!(profile_name = %profile.name, use_namespace = profile.use_namespace, "Starting test");

        if profile.use_namespace {
            match self.start_ns_mode(profile, metrics, Arc::clone(&state)).await {
                Ok(()) => {}
                Err(e) => {
                    error!(error = %e, "Failed to start test in namespace mode");
                    *state.test_state.write().await = TestState::Failed;
                }
            }
        } else {
            self.start_local_mode(profile, metrics, state).await;
        }
    }

    /// 로컬 모드: namespace 없이 localhost에서 실행.
    async fn start_local_mode(
        &mut self,
        profile: TestProfile,
        metrics: Arc<Collector>,
        state: Arc<AppState>,
    ) {
        let responder_addr: SocketAddr = format!("0.0.0.0:{}", profile.target_port)
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:8080".parse().unwrap());

        if let Err(e) = self
            .responder
            .start(responder_addr, Arc::clone(&metrics), profile.response_body_bytes, profile.tcp_quickack)
            .await
        {
            error!(error = %e, "Failed to start responder");
            *state.test_state.write().await = TestState::Failed;
            return;
        }

        // Responder 소켓 준비 대기
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        self.generator
            .start(profile.clone(), Arc::clone(&metrics), None)
            .await;

        self.transition_to_running(profile, state).await;
    }

    /// Namespace 모드: client/server NS를 생성하고 격리된 환경에서 실행.
    async fn start_ns_mode(
        &mut self,
        profile: TestProfile,
        metrics: Arc<Collector>,
        state: Arc<AppState>,
    ) -> anyhow::Result<()> {
        // 권한 확인
        net_meter_ns::check_capability()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut ns = NamespaceManager::new(&profile.netns_prefix);
        ns.setup().await.map_err(|e| anyhow::anyhow!("{}", e))?;

        let server_ns_name = ns.server_ns.clone();
        let client_ns_name = ns.client_ns.clone();
        let server_ip = ns.server_ip.clone();

        // Responder: server NS 내에서 바인드
        if let Err(e) = self
            .responder
            .start_in_ns(
                &server_ns_name,
                profile.target_port,
                Arc::clone(&metrics),
                profile.response_body_bytes,
                profile.tcp_quickack,
            )
            .await
        {
            ns.teardown().await;
            return Err(anyhow::anyhow!("Responder start_in_ns failed: {}", e));
        }

        // Responder 소켓 준비 대기
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Generator: client NS에서 server NS IP로 연결
        let mut ns_profile = profile.clone();
        ns_profile.target_host = server_ip;

        self.generator
            .start(ns_profile.clone(), Arc::clone(&metrics), Some(client_ns_name))
            .await;

        self.ns_manager = Some(ns);
        self.transition_to_running(ns_profile, state).await;
        Ok(())
    }

    /// Running 상태로 전환하고 duration 기반 자동 종료를 등록한다.
    async fn transition_to_running(&self, profile: TestProfile, state: Arc<AppState>) {
        *state.test_state.write().await = TestState::Running;
        info!("Test is running");

        if profile.duration_secs > 0 {
            let state_clone = Arc::clone(&state);
            let duration = profile.duration_secs;
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(duration)).await;
                let current = *state_clone.test_state.read().await;
                if current == TestState::Running {
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
        self.responder.stop();

        if let Some(mut ns) = self.ns_manager.take() {
            ns.teardown().await;
        }

        // 시험 결과 저장
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let start_instant = state.test_start_time.write().await.take();
        let elapsed_secs = start_instant.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let started_at_secs = now_secs.saturating_sub(elapsed_secs);

        let profile = state.active_profile.read().await.clone();
        let final_snapshot = state.latest_snapshot.read().await.clone();

        if let Some(profile) = profile {
            let result = TestResult {
                id: uuid::Uuid::new_v4().to_string(),
                profile,
                started_at_secs,
                ended_at_secs: now_secs,
                elapsed_secs,
                final_snapshot,
            };
            let mut results = state.test_results.write().await;
            results.insert(0, result); // 최신 순
            if results.len() > 50 {
                results.truncate(50); // 최대 50개 보존
            }
        }

        *state.test_start_time.write().await = None;
        *state.test_state.write().await = TestState::Completed;
        info!("Test completed");
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}
