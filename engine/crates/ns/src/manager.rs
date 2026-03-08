use std::collections::HashMap;

use net_meter_core::{Association, ClientDef, NetMeterError, ServerDef};
use tracing::{info, warn};

use crate::veth;

/// 시험용 client/server 네트워크 네임스페이스를 생성/관리한다.
///
/// # 네트워크 토폴로지
/// ```text
/// [client NS]              [host]              [server NS]
///   {client_inner}  ←→  {upper_iface}
///                         (operator가 브릿지/스위치로 연결)
///                         {lower_iface}  ←→  {server_inner}
///   client IPs                                 server IPs
/// ```
///
/// **호스트 측 상하단 인터페이스의 브릿지 연결은 운영자가 직접 설정한다.**
/// 프로그램은 NS/veth pair만 생성하고 IP를 할당한다.
///
/// **전제:** client IP와 server IP는 운영자가 설정한 L2 도메인에 맞게 지정해야 한다.
///
/// # 권한
/// CAP_NET_ADMIN 또는 root 필요.
pub struct NamespaceManager {
    pub client_ns: String,
    pub server_ns: String,
    pub upper_iface: String,
    pub lower_iface: String,
    client_inner: String,
    server_inner: String,
    ready: bool,
}

impl NamespaceManager {
    pub fn new(prefix: &str, upper_iface: &str, lower_iface: &str) -> Self {
        Self {
            client_ns: format!("{}-client", prefix),
            server_ns: format!("{}-server", prefix),
            upper_iface: upper_iface.to_string(),
            lower_iface: lower_iface.to_string(),
            client_inner: format!("{}-p", upper_iface),
            server_inner: format!("{}-p", lower_iface),
            ready: false,
        }
    }

    /// Namespace와 veth pair를 생성한다. IP 할당 및 브릿지 연결은 하지 않는다.
    ///
    /// 호스트 측 upper/lower 인터페이스를 브릿지/스위치로 연결하는 것은
    /// 운영자의 책임이다 (프로그램 시작 전에 미리 설정해야 함).
    pub async fn setup(&mut self) -> Result<(), NetMeterError> {
        info!(
            client_ns = %self.client_ns,
            server_ns = %self.server_ns,
            upper_iface = %self.upper_iface,
            lower_iface = %self.lower_iface,
            "Setting up network namespaces (veth pairs only — bridge is operator's responsibility)"
        );

        // 1. namespace 생성
        create_ns(&self.client_ns).await?;
        create_ns(&self.server_ns).await?;

        // 2. client 측 veth pair: upper_iface(host) ←→ client_inner(client NS)
        veth::create_pair(&self.upper_iface, &self.client_inner).await?;
        veth::move_to_ns(&self.client_inner, &self.client_ns).await?;
        veth::bring_up(&self.upper_iface).await?;
        veth::bring_up_in_ns(&self.client_ns, &self.client_inner).await?;
        veth::bring_up_in_ns(&self.client_ns, "lo").await?;

        // 3. server 측 veth pair: lower_iface(host) ←→ server_inner(server NS)
        veth::create_pair(&self.lower_iface, &self.server_inner).await?;
        veth::move_to_ns(&self.server_inner, &self.server_ns).await?;
        veth::bring_up(&self.lower_iface).await?;
        veth::bring_up_in_ns(&self.server_ns, &self.server_inner).await?;
        veth::bring_up_in_ns(&self.server_ns, "lo").await?;

        self.ready = true;
        info!("Network namespaces ready (connect {} ↔ {} via bridge/switch manually)", self.upper_iface, self.lower_iface);
        Ok(())
    }

    /// ClientDef/ServerDef/Association으로부터 IP를 할당한다.
    ///
    /// # 서버 IP 결정
    /// - `ServerDef.ip`가 Some: 해당 IP를 server NS에 할당
    /// - `ServerDef.ip`가 None: `10.10.1.{201,202,...}` 순서대로 자동 할당
    ///
    /// 서버 IP prefix는 /24 고정. 클라이언트는 ClientDef.cidr의 prefix를 사용한다.
    ///
    /// # 반환값
    /// - `pair_addrs`: assoc_id → "server_ip:port"
    /// - `server_binds`: server_id → "server_ip:port"
    /// - `client_ip_lists`: assoc_id → Vec<client_ip>
    pub async fn setup_network(
        &self,
        clients: &[ClientDef],
        servers: &[ServerDef],
        associations: &[Association],
    ) -> Result<
        (
            HashMap<String, String>,
            HashMap<String, String>,
            HashMap<String, Vec<String>>,
        ),
        NetMeterError,
    > {
        let client_map: HashMap<&str, &ClientDef> =
            clients.iter().map(|c| (c.id.as_str(), c)).collect();
        let server_map: HashMap<&str, &ServerDef> =
            servers.iter().map(|s| (s.id.as_str(), s)).collect();

        let mut server_ip_map: HashMap<String, String> = HashMap::new();
        let mut client_ip_lists: HashMap<String, Vec<String>> = HashMap::new();
        // 자동 할당 서버 IP 풀: 10.10.1.201, 10.10.1.202, ...
        let mut auto_octet: u8 = 201;

        // --- 서버 IP 할당 ---
        for server in servers {
            let ip = if let Some(explicit) = &server.ip {
                explicit.clone()
            } else {
                let ip = format!("10.10.1.{}", auto_octet);
                auto_octet = auto_octet.checked_add(1).ok_or_else(|| {
                    NetMeterError::Namespace(
                        "Too many auto-assigned server IPs (max ~54 in 10.10.1.201~254)".into(),
                    )
                })?;
                ip
            };

            // server NS에 IP 할당 (이미 존재하면 skip — 두 번째 시험 실행 시)
            let result = veth::set_ip_in_ns(&self.server_ns, &self.server_inner, &ip, 24).await;
            match result {
                Ok(()) => info!(server_id = %server.id, %ip, "Assigned server IP"),
                Err(e) if veth::is_addr_exists_error(&e.to_string()) => {
                    warn!(server_id = %server.id, %ip, "Server IP already assigned, skipping");
                }
                Err(e) => return Err(e),
            }

            server_ip_map.insert(server.id.clone(), ip);
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
            let client_iface = &self.client_inner;

            // VLAN subif 설정 (있을 경우)
            let actual_iface = if let Some(vlan) = &assoc.vlan {
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
                info!(assoc_id = %assoc.id, subif = %subif, "Created VLAN subif");
                subif
            } else {
                client_iface.to_string()
            };

            let ips = veth::assign_client_ips_in_ns(
                &self.client_ns,
                &actual_iface,
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

    /// namespace와 veth pair를 정리한다.
    ///
    /// 브릿지는 운영자가 관리하므로 삭제하지 않는다.
    /// veth pair 삭제 시 호스트 측 인터페이스가 브릿지에서 자동으로 해제된다.
    pub async fn teardown(&mut self) {
        if !self.ready {
            return;
        }
        info!("Tearing down network namespaces");

        // veth pair 삭제 (브릿지 포트 자동 해제됨)
        for iface in [self.upper_iface.as_str(), self.lower_iface.as_str()] {
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

/// CAP_NET_ADMIN 권한 체크
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
