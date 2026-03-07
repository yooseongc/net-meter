pub mod http1;
pub mod http2;
pub mod tcp;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{PayloadProfile, Protocol, TestConfig, TestType};
use net_meter_metrics::Collector;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// 트래픽 발생기.
///
/// `start()`로 TestConfig의 모든 pair를 동시에 구동하고,
/// `stop()`으로 모든 pair 워커를 종료한다.
pub struct Generator {
    handles: Vec<JoinHandle<()>>,
    shutdown_txs: Vec<oneshot::Sender<()>>,
}

impl Generator {
    pub fn new() -> Self {
        Self { handles: Vec::new(), shutdown_txs: Vec::new() }
    }

    /// 모든 pair 워커를 시작한다.
    ///
    /// `pair_addrs`: pair_id → "host:port" 맵 (오케스트레이터가 계산)
    /// `proto_collectors`: Protocol → Arc<Collector> 맵 (per-protocol 집계용)
    /// `client_ns`: Some(name)이면 해당 NS로 진입 후 실행
    pub async fn start(
        &mut self,
        config: &TestConfig,
        global: Arc<Collector>,
        proto_collectors: &HashMap<Protocol, Arc<Collector>>,
        pair_addrs: &HashMap<String, String>,
        client_ns: Option<String>,
    ) {
        info!(
            pairs = config.pairs.len(),
            test_type = ?config.test_type,
            use_ns = client_ns.is_some(),
            "Generator starting all pair workers"
        );

        for pair in &config.pairs {
            let addr = match pair_addrs.get(&pair.id) {
                Some(a) => a.clone(),
                None => {
                    error!(pair_id = %pair.id, "No address resolved for pair, skipping");
                    continue;
                }
            };

            let load = pair.effective_load(&config.default_load).clone();
            let payload = pair.payload.clone();
            let protocol = pair.protocol;
            let test_type = config.test_type;
            let duration_secs = config.duration_secs;
            let p = proto_collectors
                .get(&protocol)
                .cloned()
                .unwrap_or_else(Collector::new);
            let worker_count = pair.client_count.max(1) as usize;

            for worker_idx in 0..worker_count {
                let g = Arc::clone(&global);
                let p = Arc::clone(&p);
                let addr = addr.clone();
                let load = load.clone();
                let payload = payload.clone();

                let (tx, rx) = oneshot::channel();
                self.shutdown_txs.push(tx);

                let handle = if let Some(ref ns) = client_ns {
                    let ns_name = ns.clone();
                    let pair_id = pair.id.clone();
                    tokio::spawn(async move {
                        info!(%pair_id, %ns_name, %protocol, worker_idx, "Pair worker starting (NS mode)");
                        let _ = tokio::task::spawn_blocking(move || {
                            run_pair_in_ns(
                                test_type, &addr, protocol, payload, load,
                                g, p, rx, duration_secs, &ns_name,
                            )
                        })
                        .await;
                    })
                } else {
                    let pair_id = pair.id.clone();
                    tokio::spawn(async move {
                        info!(%pair_id, %protocol, worker_idx, "Pair worker starting (local mode)");
                        run_pair(test_type, &addr, protocol, payload, load, g, p, rx, duration_secs).await;
                    })
                };

                self.handles.push(handle);
            }
        }
    }

    /// 모든 pair 워커를 중지하고 종료를 기다린다.
    pub async fn stop(&mut self) {
        for tx in self.shutdown_txs.drain(..) {
            let _ = tx.send(());
        }
        for h in self.handles.drain(..) {
            let _ = h.await;
        }
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 로컬 모드: 현재 tokio 런타임에서 직접 실행
// ---------------------------------------------------------------------------

async fn run_pair(
    test_type: TestType,
    addr: &str,
    protocol: Protocol,
    payload: PayloadProfile,
    load: net_meter_core::LoadConfig,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    duration_secs: u64,
) {
    let deadline = make_deadline(duration_secs);
    dispatch(test_type, addr, protocol, &payload, &load, global, proto, shutdown, deadline).await;
}

// ---------------------------------------------------------------------------
// NS 모드: spawn_blocking 내부에서 setns + current_thread 런타임
// ---------------------------------------------------------------------------

fn run_pair_in_ns(
    test_type: TestType,
    addr: &str,
    protocol: Protocol,
    payload: PayloadProfile,
    load: net_meter_core::LoadConfig,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    rx: oneshot::Receiver<()>,
    duration_secs: u64,
    ns_name: &str,
) {
    // 호스트 NS 저장
    let orig = match std::fs::File::open("/proc/self/ns/net") {
        Ok(f) => f,
        Err(e) => { tracing::error!("Failed to open host ns: {}", e); return; }
    };

    // client NS 진입
    let ns_path = format!("/var/run/netns/{}", ns_name);
    let ns_file = match std::fs::File::open(&ns_path) {
        Ok(f) => f,
        Err(e) => { tracing::error!(ns = %ns_name, "Failed to open ns: {}", e); return; }
    };
    if let Err(e) = nix::sched::setns(&ns_file, nix::sched::CloneFlags::CLONE_NEWNET) {
        tracing::error!(ns = %ns_name, "setns failed: {}", e);
        return;
    }

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("Failed to build NS runtime: {}", e);
            let _ = nix::sched::setns(&orig, nix::sched::CloneFlags::CLONE_NEWNET);
            return;
        }
    };

    let deadline = make_deadline(duration_secs);
    rt.block_on(async move {
        dispatch(test_type, addr, protocol, &payload, &load, global, proto, rx, deadline).await;
    });

    // 호스트 NS 복구
    if let Err(e) = nix::sched::setns(&orig, nix::sched::CloneFlags::CLONE_NEWNET) {
        tracing::error!("Failed to restore host ns: {}", e);
    }
}

// ---------------------------------------------------------------------------
// 프로토콜 × 페이로드 디스패치
// ---------------------------------------------------------------------------

async fn dispatch(
    test_type: TestType,
    addr: &str,
    protocol: Protocol,
    payload: &PayloadProfile,
    load: &net_meter_core::LoadConfig,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    match (protocol, payload) {
        (Protocol::Tcp, PayloadProfile::Tcp(p)) => {
            tcp::run(test_type, addr, load, p, global, proto, shutdown, deadline).await;
        }
        (Protocol::Http1, PayloadProfile::Http(p)) => {
            http1::run(test_type, addr, load, p, global, proto, shutdown, deadline).await;
        }
        (Protocol::Http2, PayloadProfile::Http(p)) => {
            http2::run(test_type, addr, load, p, global, proto, shutdown, deadline).await;
        }
        (proto, payload) => {
            tracing::error!(
                ?proto,
                payload_type = match payload {
                    PayloadProfile::Tcp(_) => "tcp",
                    PayloadProfile::Http(_) => "http",
                },
                "Protocol/payload mismatch — skipping pair"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 헬퍼
// ---------------------------------------------------------------------------

fn make_deadline(duration_secs: u64) -> Option<Instant> {
    if duration_secs > 0 {
        Some(Instant::now() + Duration::from_secs(duration_secs))
    } else {
        None
    }
}

