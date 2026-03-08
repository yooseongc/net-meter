use std::collections::HashMap;

use net_meter_core::{Association, ClientDef, NetMeterError, ServerDef};
use tracing::{info, warn};

use crate::veth;

/// External Port 모드에서 할당한 리소스 추적.
pub struct ExternalPortState {
    pub upper_iface: String,
    pub lower_iface: String,
}

/// 정책 라우팅 상태 — 시험 종료 시 정리에 사용한다.
pub struct PolicyRoutingState {
    pub upper_table: u32,
    pub lower_table: u32,
    pub client_cidrs: Vec<String>,
    pub server_cidrs: Vec<String>,
}

/// External Port 모드용 정책 라우팅 테이블 번호.
const EXT_UPPER_TABLE: u32 = 191;
const EXT_LOWER_TABLE: u32 = 192;

/// External Port 모드를 설정한다.
/// 물리 NIC(또는 브릿지)에 Promiscuous 모드와 MTU를 설정한다.
/// IP 할당은 시험 시작 시 `assign_ext_port_network()`에서 수행한다.
pub async fn setup_external_port(
    upper_iface: &str,
    lower_iface: &str,
    mtu: u16,
) -> Result<ExternalPortState, NetMeterError> {
    info!(%upper_iface, %lower_iface, mtu, "Setting up external port mode");

    for iface in [upper_iface, lower_iface] {
        if !veth::check_iface(iface).await {
            return Err(NetMeterError::Namespace(format!(
                "Interface '{}' not found. Check the interface name.",
                iface
            )));
        }
        veth::bring_up(iface).await?;
        veth::set_mtu(iface, mtu).await?;
        info!(%iface, mtu, "Interface configured (up, mtu)");
    }

    info!("External port setup complete");
    Ok(ExternalPortState {
        upper_iface: upper_iface.to_string(),
        lower_iface: lower_iface.to_string(),
    })
}

/// 시험 시작 시 호출: 인터페이스에 클라이언트/서버 IP를 할당하고
/// Generator/Responder에 전달할 주소 맵을 구성한다.
///
/// # 반환값
/// - `pair_addrs`:    assoc_id  → "server_ip:port"
/// - `server_binds`:  server_id → "server_ip:port"
/// - `client_ip_lists`: assoc_id → Vec<client_ip>
pub async fn assign_ext_port_network(
    upper_iface: &str,
    lower_iface: &str,
    clients: &[ClientDef],
    servers: &[ServerDef],
    associations: &[Association],
    per_assoc_count: u32,
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
    let mut auto_octet: u8 = 201;

    // --- 서버 IP: lower_iface에 할당 ---
    for server in servers {
        let ip = if let Some(explicit) = &server.ip {
            explicit.clone()
        } else {
            let ip = format!("10.10.1.{}", auto_octet);
            auto_octet = auto_octet.checked_add(1).ok_or_else(|| {
                NetMeterError::Namespace("Too many auto-assigned server IPs".into())
            })?;
            ip
        };

        let result = veth::set_ip(lower_iface, &ip, 24).await;
        match result {
            Ok(()) => info!(server_id = %server.id, %ip, %lower_iface, "Assigned server IP"),
            Err(e) if veth::is_addr_exists_error(&e.to_string()) => {
                warn!(server_id = %server.id, %ip, "Server IP already assigned, skipping");
            }
            Err(e) => return Err(e),
        }
        server_ip_map.insert(server.id.clone(), ip);
    }

    // --- 클라이언트 IP: upper_iface에 할당 ---
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
        let count = client_def.count.unwrap_or(per_assoc_count);

        let ips = veth::assign_ips(upper_iface, &base_ip.to_string(), count, prefix_len).await?;
        info!(
            assoc_id = %assoc.id,
            count = ips.len(),
            %upper_iface,
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

/// External Port 모드를 정리한다.
/// 인터페이스의 IP를 flush한다.
pub async fn teardown_external_port(state: &ExternalPortState) {
    info!("Tearing down external port configuration");

    for iface in [state.upper_iface.as_str(), state.lower_iface.as_str()] {
        if let Err(e) = veth::flush_iface(iface).await {
            warn!(%iface, error = %e, "Failed to flush IPs");
        }
    }

    info!("External port teardown complete");
}

/// 정책 라우팅을 설정한다.
///
/// - client_cidrs: Generator가 사용할 src IP 대역 (ClientDef.cidr 목록)
/// - server_ips:   Responder가 바인딩한 server IP 목록
///
/// 설정 내용:
///   table 191: default dev upper_iface  (client → DUT 방향 강제)
///   table 192: default dev lower_iface  (server → DUT 방향 강제)
///   ip rule from <client_cidr> → table 191
///   ip rule from <server_ip>/32 → table 192
pub async fn setup_policy_routing(
    upper_iface: &str,
    lower_iface: &str,
    client_cidrs: &[String],
    server_ips: &[String],
) -> Result<PolicyRoutingState, NetMeterError> {
    info!(%upper_iface, %lower_iface, "Setting up policy routing");

    // table 191: default via upper_iface
    let r = veth::add_route_table_dev(upper_iface, EXT_UPPER_TABLE).await;
    if let Err(ref e) = r {
        if !veth::is_rule_exists_error(&e.to_string()) {
            return Err(r.unwrap_err());
        }
    }
    for cidr in client_cidrs {
        let r = veth::add_ip_rule(cidr, EXT_UPPER_TABLE).await;
        if let Err(ref e) = r {
            if !veth::is_rule_exists_error(&e.to_string()) {
                return Err(r.unwrap_err());
            }
        }
        info!(%cidr, table = EXT_UPPER_TABLE, "Added ip rule (client → upper)");
    }

    // table 192: default via lower_iface
    let r = veth::add_route_table_dev(lower_iface, EXT_LOWER_TABLE).await;
    if let Err(ref e) = r {
        if !veth::is_rule_exists_error(&e.to_string()) {
            return Err(r.unwrap_err());
        }
    }
    let server_cidrs: Vec<String> = server_ips.iter().map(|ip| format!("{}/32", ip)).collect();
    for cidr in &server_cidrs {
        let r = veth::add_ip_rule(cidr, EXT_LOWER_TABLE).await;
        if let Err(ref e) = r {
            if !veth::is_rule_exists_error(&e.to_string()) {
                return Err(r.unwrap_err());
            }
        }
        info!(%cidr, table = EXT_LOWER_TABLE, "Added ip rule (server → lower)");
    }

    info!("Policy routing setup complete");
    Ok(PolicyRoutingState {
        upper_table: EXT_UPPER_TABLE,
        lower_table: EXT_LOWER_TABLE,
        client_cidrs: client_cidrs.to_vec(),
        server_cidrs,
    })
}

/// 정책 라우팅을 정리한다 (시험 종료 시 호출).
pub async fn teardown_policy_routing(state: &PolicyRoutingState) {
    info!("Tearing down policy routing");

    for cidr in &state.client_cidrs {
        if let Err(e) = veth::del_ip_rule(cidr, state.upper_table).await {
            warn!(%cidr, error = %e, "Failed to remove client ip rule");
        }
    }
    for cidr in &state.server_cidrs {
        if let Err(e) = veth::del_ip_rule(cidr, state.lower_table).await {
            warn!(%cidr, error = %e, "Failed to remove server ip rule");
        }
    }
    if let Err(e) = veth::flush_route_table(state.upper_table).await {
        warn!(table = state.upper_table, error = %e, "Failed to flush upper route table");
    }
    if let Err(e) = veth::flush_route_table(state.lower_table).await {
        warn!(table = state.lower_table, error = %e, "Failed to flush lower route table");
    }

    info!("Policy routing teardown complete");
}
