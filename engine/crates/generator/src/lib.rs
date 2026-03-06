pub mod http1;

use std::sync::Arc;

use net_meter_core::TestProfile;
use net_meter_metrics::Collector;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::info;

/// 트래픽 발생기 핸들.
///
/// start()로 백그라운드 태스크를 시작하고, stop()으로 중지한다.
pub struct Generator {
    handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl Generator {
    pub fn new() -> Self {
        Self {
            handle: None,
            shutdown_tx: None,
        }
    }

    /// 시험 프로파일에 따라 트래픽 발생을 시작한다.
    ///
    /// `client_ns`: Some(name)이면 해당 네임스페이스에서 연결을 생성한다.
    ///   - spawn_blocking 스레드에서 setns 후 current_thread 런타임으로 실행.
    ///   - None이면 호스트 네임스페이스에서 직접 실행 (기존 동작).
    pub async fn start(
        &mut self,
        profile: TestProfile,
        metrics: Arc<Collector>,
        client_ns: Option<String>,
    ) {
        let (tx, rx) = oneshot::channel();
        self.shutdown_tx = Some(tx);

        let handle = if let Some(ns_name) = client_ns {
            // Namespace 모드: 전용 스레드에서 client NS 진입 후 별도 런타임 구동
            tokio::spawn(async move {
                info!(
                    test_type = ?profile.test_type,
                    target = %format!("{}:{}", profile.target_host, profile.target_port),
                    ns = %ns_name,
                    "Generator started (namespace mode)"
                );
                let _ = tokio::task::spawn_blocking(move || {
                    run_in_ns(profile, metrics, rx, &ns_name)
                })
                .await;
                info!("Generator stopped");
            })
        } else {
            // 로컬 모드: 현재 tokio 런타임에서 직접 실행
            tokio::spawn(async move {
                info!(
                    test_type = ?profile.test_type,
                    target = %format!("{}:{}", profile.target_host, profile.target_port),
                    "Generator started (local mode)"
                );
                http1::run(profile, metrics, rx).await;
                info!("Generator stopped");
            })
        };

        self.handle = Some(handle);
    }

    /// 트래픽 발생을 중지하고 완료까지 기다린다.
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

/// client 네임스페이스로 진입하여 current_thread 런타임에서 generator를 실행한다.
///
/// 이 함수는 spawn_blocking 내부에서 호출된다.
/// 완료 시 반드시 호스트 NS로 복구하여 tokio 스레드 풀 오염을 방지한다.
fn run_in_ns(
    profile: TestProfile,
    metrics: Arc<Collector>,
    rx: oneshot::Receiver<()>,
    ns_name: &str,
) {
    // 호스트 NS 저장
    let orig = match std::fs::File::open("/proc/self/ns/net") {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to open host ns: {}", e);
            return;
        }
    };

    // client NS 진입 (&file: AsFd)
    let ns_path = format!("/var/run/netns/{}", ns_name);
    let ns_file = match std::fs::File::open(&ns_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(ns = %ns_name, "Failed to open client ns: {}", e);
            return;
        }
    };
    if let Err(e) = nix::sched::setns(&ns_file, nix::sched::CloneFlags::CLONE_NEWNET) {
        tracing::error!(ns = %ns_name, "setns failed: {}", e);
        return;
    }

    // current_thread 런타임 구동 (이 스레드가 client NS에 있으므로 모든 소켓이 client NS 소속)
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("Failed to build runtime: {}", e);
            let _ = nix::sched::setns(&orig, nix::sched::CloneFlags::CLONE_NEWNET);
            return;
        }
    };

    rt.block_on(async move {
        http1::run(profile, metrics, rx).await;
    });

    // 호스트 NS 복구 (tokio 스레드 풀 오염 방지)
    if let Err(e) = nix::sched::setns(&orig, nix::sched::CloneFlags::CLONE_NEWNET) {
        tracing::error!("Failed to restore host ns: {}", e);
    }
}
