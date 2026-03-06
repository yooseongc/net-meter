use std::net::SocketAddr;

use net_meter_core::NetMeterError;
use tracing::{info, warn};

use crate::veth;

/// 시험용 client/server 네트워크 네임스페이스를 생성/관리한다.
///
/// # 네트워크 토폴로지
/// ```text
/// [client NS: 10.10.0.2/30]
///   veth-c1
///       |  (veth pair)
///   veth-c0 [host: 10.10.0.1/30]  <-- control
///   veth-s0 [host: 10.20.0.1/30]
///       |  (veth pair)
///   veth-s1
/// [server NS: 10.20.0.2/30]
/// ```
///
/// # 권한
/// namespace 생성/삭제에는 CAP_NET_ADMIN 또는 root 권한이 필요하다.
pub struct NamespaceManager {
    pub client_ns: String,
    pub server_ns: String,
    /// client NS의 IP (generator가 연결 출발점)
    pub client_ip: String,
    /// server NS의 IP (responder가 bind할 주소)
    pub server_ip: String,
    ready: bool,
}

impl NamespaceManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            client_ns: format!("{}-client", prefix),
            server_ns: format!("{}-server", prefix),
            client_ip: "10.10.0.2".to_string(),
            server_ip: "10.20.0.2".to_string(),
            ready: false,
        }
    }

    /// namespace와 veth pair를 생성하고 IP/route를 설정한다.
    pub async fn setup(&mut self) -> Result<(), NetMeterError> {
        info!(
            client_ns = %self.client_ns,
            server_ns = %self.server_ns,
            "Setting up network namespaces"
        );

        // 1. namespace 생성
        create_ns(&self.client_ns).await?;
        create_ns(&self.server_ns).await?;

        // 2. client 측 veth pair
        veth::create_pair("veth-c0", "veth-c1").await?;
        veth::move_to_ns("veth-c1", &self.client_ns).await?;
        veth::set_ip("veth-c0", "10.10.0.1", 30).await?;
        veth::set_ip_in_ns(&self.client_ns, "veth-c1", "10.10.0.2", 30).await?;
        veth::bring_up("veth-c0").await?;
        veth::bring_up_in_ns(&self.client_ns, "veth-c1").await?;
        veth::bring_up_in_ns(&self.client_ns, "lo").await?;

        // 3. server 측 veth pair
        veth::create_pair("veth-s0", "veth-s1").await?;
        veth::move_to_ns("veth-s1", &self.server_ns).await?;
        veth::set_ip("veth-s0", "10.20.0.1", 30).await?;
        veth::set_ip_in_ns(&self.server_ns, "veth-s1", "10.20.0.2", 30).await?;
        veth::bring_up("veth-s0").await?;
        veth::bring_up_in_ns(&self.server_ns, "veth-s1").await?;
        veth::bring_up_in_ns(&self.server_ns, "lo").await?;

        // 4. IP 포워딩 활성화 (host에서 client NS ↔ server NS 패킷 전달)
        enable_ip_forwarding().await?;

        // 5. client NS 라우팅: server NS(10.20.0.0/30)는 host(10.10.0.1) 경유
        veth::add_route_in_ns(&self.client_ns, "10.20.0.0/30", "10.10.0.1").await?;

        // 6. server NS 라우팅: client NS(10.10.0.0/30)는 host(10.20.0.1) 경유
        veth::add_route_in_ns(&self.server_ns, "10.10.0.0/30", "10.20.0.1").await?;

        self.ready = true;
        info!("Network namespaces ready");
        Ok(())
    }

    /// namespace와 veth 인터페이스를 정리한다. 오류가 있어도 최대한 정리한다.
    pub async fn teardown(&mut self) {
        if !self.ready {
            return;
        }
        info!("Tearing down network namespaces");

        for iface in ["veth-c0", "veth-s0"] {
            if let Err(e) = delete_link(iface).await {
                warn!(iface, error = %e, "Failed to delete link");
            }
        }
        for ns in [&self.client_ns.clone(), &self.server_ns.clone()] {
            if let Err(e) = delete_ns(ns).await {
                warn!(ns, error = %e, "Failed to delete namespace");
            }
        }

        self.ready = false;
        info!("Network namespaces cleaned up");
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// server NS에서 TcpListener를 생성한다 (spawn_blocking 내부에서 호출).
    pub fn bind_listener_in_server_ns(&self, port: u16) -> Result<std::net::TcpListener, NetMeterError> {
        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().map_err(|e| {
            NetMeterError::Namespace(format!("invalid addr: {}", e))
        })?;
        crate::setns::bind_listener_in_ns(&self.server_ns, addr)
    }
}

async fn create_ns(name: &str) -> Result<(), NetMeterError> {
    run_ip(&["netns", "add", name]).await
}

async fn delete_ns(name: &str) -> Result<(), NetMeterError> {
    run_ip(&["netns", "del", name]).await
}

async fn delete_link(name: &str) -> Result<(), NetMeterError> {
    run_ip(&["link", "del", name]).await
}

/// 호스트의 IPv4 포워딩을 활성화한다.
async fn enable_ip_forwarding() -> Result<(), NetMeterError> {
    let output = tokio::process::Command::new("sysctl")
        .args(["-w", "net.ipv4.ip_forward=1"])
        .output()
        .await
        .map_err(NetMeterError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetMeterError::Namespace(format!(
            "sysctl ip_forward failed: {}",
            stderr.trim()
        )));
    }
    info!("IP forwarding enabled");
    Ok(())
}

pub(crate) async fn run_ip(args: &[&str]) -> Result<(), NetMeterError> {
    let output = tokio::process::Command::new("ip")
        .args(args)
        .output()
        .await
        .map_err(NetMeterError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetMeterError::Namespace(format!(
            "ip {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(())
}

/// CAP_NET_ADMIN 권한 체크: root(uid=0) 또는 CAP_NET_ADMIN 소지 여부 확인.
pub fn check_capability() -> Result<(), NetMeterError> {
    if nix::unistd::getuid().is_root() {
        return Ok(());
    }
    // /proc/self/status에서 CapEff 비트를 읽어 CAP_NET_ADMIN(12번 비트) 체크
    let status = std::fs::read_to_string("/proc/self/status").map_err(NetMeterError::Io)?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("CapEff:\t") {
            if let Ok(cap_eff) = u64::from_str_radix(rest.trim(), 16) {
                const CAP_NET_ADMIN: u64 = 1 << 12;
                if cap_eff & CAP_NET_ADMIN != 0 {
                    return Ok(());
                }
            }
        }
    }
    Err(NetMeterError::Namespace(
        "Namespace management requires root or CAP_NET_ADMIN. Try: sudo net-meter".to_string(),
    ))
}
