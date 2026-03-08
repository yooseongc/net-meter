use std::collections::HashMap;

use net_meter_core::{Association, ClientDef, NetMeterError, ServerDef};
use tracing::{info, warn};

use crate::veth;

/// 시험용 client/server 네트워크 네임스페이스를 생성/관리한다.
///
/// # 네트워크 토폴로지
/// ```text
/// [client NS: 10.10.1.x/24 (ClientNet 기반)]
///   veth-c1
///       |  (veth pair)
///   veth-c0 [host: 10.10.1.254/24]  <-- control
///   veth-s0 [host: 10.20.1.254/24]
///       |  (veth pair)
///   veth-s1
/// [server NS: 10.20.1.1/24, 10.20.1.2/24, ...]
/// ```
///
/// server NS는 /24 서브넷에서 여러 IP alias를 가질 수 있다.
/// client NS는 각 association의 ClientNet에서 지정한 IP 대역을 사용한다.
///
/// # 권한
/// namespace 생성/삭제에는 CAP_NET_ADMIN 또는 root 권한이 필요하다.
pub struct NamespaceManager {
    pub client_ns: String,
    pub server_ns: String,
    ready: bool,
}

impl NamespaceManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            client_ns: format!("{}-client", prefix),
            server_ns: format!("{}-server", prefix),
            ready: false,
        }
    }

    /// namespace와 veth pair를 생성하고 기본 IP/route를 설정한다.
    ///
    /// - client NS: 10.10.1.1/24 (veth-c1, 기본 IP; 추가 IP는 setup_associations에서 할당)
    /// - host: veth-c0(10.10.1.254/24), veth-s0(10.20.1.254/24)
    /// - server NS: 10.20.1.1/24 (veth-s1, 기본 IP; 추가 서버는 setup_associations에서 alias 추가)
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
        veth::set_ip("veth-c0", "10.10.1.254", 24).await?;
        veth::set_ip_in_ns(&self.client_ns, "veth-c1", "10.10.1.1", 24).await?;
        veth::bring_up("veth-c0").await?;
        veth::bring_up_in_ns(&self.client_ns, "veth-c1").await?;
        veth::bring_up_in_ns(&self.client_ns, "lo").await?;

        // 3. server 측 veth pair
        veth::create_pair("veth-s0", "veth-s1").await?;
        veth::move_to_ns("veth-s1", &self.server_ns).await?;
        veth::set_ip("veth-s0", "10.20.1.254", 24).await?;
        veth::set_ip_in_ns(&self.server_ns, "veth-s1", "10.20.1.1", 24).await?;
        veth::bring_up("veth-s0").await?;
        veth::bring_up_in_ns(&self.server_ns, "veth-s1").await?;
        veth::bring_up_in_ns(&self.server_ns, "lo").await?;

        // 4. IP 포워딩 활성화
        enable_ip_forwarding().await?;

        // 5. 라우팅
        veth::add_route_in_ns(&self.client_ns, "10.20.1.0/24", "10.10.1.254").await?;
        veth::add_route_in_ns(&self.server_ns, "10.10.1.0/24", "10.20.1.254").await?;

        self.ready = true;
        info!("Network namespaces ready");
        Ok(())
    }

    /// ClientDef/ServerDef/Association 목록으로부터 IP를 할당하고 VLAN subif를 생성한다.
    ///
    /// # 반환값
    /// - `pair_addrs`: assoc_id → "server_ip:port" — Generator가 연결할 서버 주소
    /// - `server_binds`: server_id → "server_ip:port" — Responder가 bind할 주소
    /// - `client_ip_lists`: assoc_id → Vec<client_ip_str> — 워커별 소스 IP 목록
    pub async fn setup_network(
        &self,
        clients: &[ClientDef],
        servers: &[ServerDef],
        associations: &[Association],
    ) -> Result<
        (
            HashMap<String, String>,          // pair_addrs
            HashMap<String, String>,          // server_binds
            HashMap<String, Vec<String>>,     // client_ip_lists
        ),
        NetMeterError,
    > {
        let client_map: HashMap<&str, &ClientDef> =
            clients.iter().map(|c| (c.id.as_str(), c)).collect();
        let server_map: HashMap<&str, &ServerDef> =
            servers.iter().map(|s| (s.id.as_str(), s)).collect();

        let mut server_ip_map: HashMap<String, String> = HashMap::new(); // server_id → ip
        let mut client_ip_lists: HashMap<String, Vec<String>> = HashMap::new();
        let mut next_server_ip: u8 = 1;

        // --- 서버 IP 할당 (servers 목록 순서대로) ---
        for server in servers {
            let ip = format!("10.20.1.{}", next_server_ip);
            // 10.20.1.1은 setup()에서 이미 할당됨; 2번부터 alias 추가
            if next_server_ip > 1 {
                veth::set_ip_in_ns(&self.server_ns, "veth-s1", &ip, 24).await?;
                info!(server_id = %server.id, %ip, "Added server IP alias");
            }
            server_ip_map.insert(server.id.clone(), ip);
            next_server_ip = next_server_ip.checked_add(1).ok_or_else(|| {
                NetMeterError::Namespace("Too many server endpoints (max 254)".to_string())
            })?;
        }

        // --- association별 클라이언트 IP 할당 ---
        for assoc in associations {
            let client_def = match client_map.get(assoc.client_id.as_str()) {
                Some(c) => c,
                None => {
                    return Err(NetMeterError::Namespace(format!(
                        "ClientDef '{}' not found for association '{}'",
                        assoc.client_id, assoc.id
                    )));
                }
            };

            let (base_ip, prefix_len) = client_def.parse_cidr().map_err(NetMeterError::Namespace)?;
            let client_count = client_def.effective_count();
            let client_iface = "veth-c1";

            // VLAN 설정이 있으면 VLAN subif 생성 후 해당 subif에 IP 할당
            let actual_client_iface = if let Some(vlan) = &assoc.vlan {
                let subif = if let Some(inner_vid) = vlan.inner_vid {
                    veth::add_qinq_subif_in_ns(
                        &self.client_ns,
                        client_iface,
                        vlan.outer_vid,
                        inner_vid,
                        vlan.outer_proto,
                    )
                    .await?
                } else {
                    veth::add_vlan_subif_in_ns(
                        &self.client_ns,
                        client_iface,
                        vlan.outer_vid,
                        vlan.outer_proto,
                    )
                    .await?
                };
                info!(assoc_id = %assoc.id, subif = %subif, "Created VLAN subif for client");
                subif
            } else {
                client_iface.to_string()
            };

            let ips = veth::assign_client_ips_in_ns(
                &self.client_ns,
                &actual_client_iface,
                &base_ip.to_string(),
                client_count,
                prefix_len,
            )
            .await?;

            info!(
                assoc_id = %assoc.id,
                count = ips.len(),
                cidr = %client_def.cidr,
                "Assigned client IPs"
            );
            client_ip_lists.insert(assoc.id.clone(), ips);
        }

        // assoc_id → "server_ip:port"
        let pair_addrs: HashMap<String, String> = associations
            .iter()
            .filter_map(|assoc| {
                let server = server_map.get(assoc.server_id.as_str())?;
                let ip = server_ip_map.get(&assoc.server_id)?;
                Some((assoc.id.clone(), format!("{}:{}", ip, server.port)))
            })
            .collect();

        // server_id → "server_ip:port"
        let server_binds: HashMap<String, String> = server_map
            .iter()
            .filter_map(|(server_id, server)| {
                let ip = server_ip_map.get(*server_id)?;
                Some((server_id.to_string(), format!("{}:{}", ip, server.port)))
            })
            .collect();

        Ok((pair_addrs, server_binds, client_ip_lists))
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
