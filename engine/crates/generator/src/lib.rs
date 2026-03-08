pub mod http1;
pub mod http2;
pub mod tcp;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{PayloadProfile, Protocol, TestConfig};
use net_meter_metrics::Collector;
use rustls::ClientConfig;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// 트래픽 발생기.
///
/// `start()`로 TestConfig의 모든 association을 동시에 구동하고,
/// `stop()`으로 모든 association 워커를 종료한다.
pub struct Generator {
    handles: Vec<JoinHandle<()>>,
    shutdown_txs: Vec<oneshot::Sender<()>>,
}

impl Generator {
    pub fn new() -> Self {
        Self { handles: Vec::new(), shutdown_txs: Vec::new() }
    }

    /// 모든 association 워커를 시작한다.
    ///
    /// `pair_addrs`: assoc_id → "host:port" 맵 (오케스트레이터가 계산)
    /// `proto_collectors`: Protocol → Arc<Collector> 맵 (per-protocol 집계용)
    /// `client_ns`: Some(name)이면 해당 NS로 진입 후 실행
    /// `tls_client_config`: TLS가 활성화된 server에서 사용할 ClientConfig
    /// `client_ips`: assoc_id → Vec<client_ip> — per-워커 소스 IP 목록 (비어있으면 IP 바인딩 없음)
    pub async fn start(
        &mut self,
        config: &TestConfig,
        global: Arc<Collector>,
        proto_collectors: &HashMap<Protocol, Arc<Collector>>,
        pair_addrs: &HashMap<String, String>,
        client_ns: Option<String>,
        tls_client_config: Option<Arc<ClientConfig>>,
        client_ips: &HashMap<String, Vec<String>>,
    ) {
        info!(
            associations = config.associations.len(),
            test_type = ?config.test_type,
            use_ns = client_ns.is_some(),
            "Generator starting all association workers"
        );

        let server_map = config.server_map();
        let client_map = config.client_map();

        for assoc in &config.associations {
            let server_def = match server_map.get(&assoc.server_id) {
                Some(s) => s.clone(),
                None => {
                    error!(assoc_id = %assoc.id, server_id = %assoc.server_id, "No ServerDef found, skipping");
                    continue;
                }
            };

            let client_def = match client_map.get(&assoc.client_id) {
                Some(c) => c.clone(),
                None => {
                    error!(assoc_id = %assoc.id, client_id = %assoc.client_id, "No ClientDef found, skipping");
                    continue;
                }
            };

            let addr = match pair_addrs.get(&assoc.id) {
                Some(a) => a.clone(),
                None => {
                    error!(assoc_id = %assoc.id, "No address resolved for association, skipping");
                    continue;
                }
            };

            let protocol = server_def.protocol;
            let load = assoc.effective_load(&config.default_load).clone();
            let payload = assoc.payload.clone();
            let test_type = config.test_type;
            let duration_secs = config.duration_secs;

            let p = proto_collectors
                .get(&protocol)
                .cloned()
                .unwrap_or_else(Collector::new);

            // 워커 수 결정: per-client IP 목록이 있으면 그 수, 없으면 client_def.effective_count()
            let ip_list = client_ips.get(&assoc.id).cloned().unwrap_or_default();
            let worker_count = if !ip_list.is_empty() {
                ip_list.len()
            } else {
                client_def.effective_count() as usize
            };

            // server_def.tls가 true이고 TLS config가 있을 때만 TLS 사용
            let assoc_tls = if server_def.tls { tls_client_config.clone() } else { None };

            for worker_idx in 0..worker_count {
                let g = Arc::clone(&global);
                let p = Arc::clone(&p);
                let addr = addr.clone();
                let load = load.clone();
                let payload = payload.clone();
                let tls = assoc_tls.clone();

                // 이 워커에 할당된 소스 IP (없으면 None = 바인딩 안 함)
                let src_ip: Option<IpAddr> = ip_list
                    .get(worker_idx)
                    .and_then(|s| s.parse().ok());

                let (tx, rx) = oneshot::channel();
                self.shutdown_txs.push(tx);

                let handle = if let Some(ref ns) = client_ns {
                    let ns_name = ns.clone();
                    let assoc_id = assoc.id.clone();
                    tokio::spawn(async move {
                        info!(%assoc_id, %ns_name, %protocol, worker_idx, ?src_ip, "Association worker starting (NS mode)");
                        let _ = tokio::task::spawn_blocking(move || {
                            run_pair_in_ns(
                                test_type, &addr, protocol, payload, load,
                                g, p, rx, duration_secs, &ns_name, tls, src_ip,
                            )
                        })
                        .await;
                    })
                } else {
                    let assoc_id = assoc.id.clone();
                    tokio::spawn(async move {
                        info!(%assoc_id, %protocol, worker_idx, ?src_ip, "Association worker starting (local mode)");
                        run_pair(
                            test_type, &addr, protocol, payload, load,
                            g, p, rx, duration_secs, tls, src_ip,
                        )
                        .await;
                    })
                };

                self.handles.push(handle);
            }
        }
    }

    /// 모든 association 워커를 중지하고 종료를 기다린다.
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
    test_type: net_meter_core::TestType,
    addr: &str,
    protocol: Protocol,
    payload: PayloadProfile,
    load: net_meter_core::LoadConfig,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    duration_secs: u64,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let deadline = make_deadline(duration_secs);
    dispatch(test_type, addr, protocol, &payload, &load, global, proto, shutdown, deadline, tls, src_ip).await;
}

// ---------------------------------------------------------------------------
// NS 모드: spawn_blocking 내부에서 setns + current_thread 런타임
// ---------------------------------------------------------------------------

fn run_pair_in_ns(
    test_type: net_meter_core::TestType,
    addr: &str,
    protocol: Protocol,
    payload: PayloadProfile,
    load: net_meter_core::LoadConfig,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    rx: oneshot::Receiver<()>,
    duration_secs: u64,
    ns_name: &str,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let orig = match std::fs::File::open("/proc/self/ns/net") {
        Ok(f) => f,
        Err(e) => { tracing::error!("Failed to open host ns: {}", e); return; }
    };

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
        dispatch(test_type, addr, protocol, &payload, &load, global, proto, rx, deadline, tls, src_ip).await;
    });

    if let Err(e) = nix::sched::setns(&orig, nix::sched::CloneFlags::CLONE_NEWNET) {
        tracing::error!("Failed to restore host ns: {}", e);
    }
}

// ---------------------------------------------------------------------------
// 프로토콜 × 페이로드 디스패치
// ---------------------------------------------------------------------------

async fn dispatch(
    test_type: net_meter_core::TestType,
    addr: &str,
    protocol: Protocol,
    payload: &PayloadProfile,
    load: &net_meter_core::LoadConfig,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    match (protocol, payload) {
        (Protocol::Tcp, PayloadProfile::Tcp(p)) => {
            tcp::run(test_type, addr, load, p, global, proto, shutdown, deadline, src_ip).await;
        }
        (Protocol::Http1, PayloadProfile::Http(p)) => {
            http1::run(test_type, addr, load, p, global, proto, shutdown, deadline, tls, src_ip).await;
        }
        (Protocol::Http2, PayloadProfile::Http(p)) => {
            http2::run(test_type, addr, load, p, global, proto, shutdown, deadline, tls, src_ip).await;
        }
        (proto, payload) => {
            tracing::error!(
                ?proto,
                payload_type = match payload {
                    PayloadProfile::Tcp(_) => "tcp",
                    PayloadProfile::Http(_) => "http",
                },
                "Protocol/payload mismatch — skipping association"
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
