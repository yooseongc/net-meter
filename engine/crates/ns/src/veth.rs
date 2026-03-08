use net_meter_core::{NetMeterError, VlanProto};

use crate::manager::run_ip;

/// veth pair 생성
pub async fn create_pair(name1: &str, name2: &str) -> Result<(), NetMeterError> {
    run_ip(&["link", "add", name1, "type", "veth", "peer", "name", name2]).await
}

/// 인터페이스를 네임스페이스로 이동
pub async fn move_to_ns(iface: &str, ns: &str) -> Result<(), NetMeterError> {
    run_ip(&["link", "set", iface, "netns", ns]).await
}

/// 호스트 네임스페이스의 인터페이스에 IP 설정
pub async fn set_ip(iface: &str, addr: &str, prefix_len: u8) -> Result<(), NetMeterError> {
    let cidr = format!("{}/{}", addr, prefix_len);
    run_ip(&["addr", "add", &cidr, "dev", iface]).await
}

/// 특정 네임스페이스 내의 인터페이스에 IP 설정
pub async fn set_ip_in_ns(
    ns: &str,
    iface: &str,
    addr: &str,
    prefix_len: u8,
) -> Result<(), NetMeterError> {
    let cidr = format!("{}/{}", addr, prefix_len);
    run_ip(&["netns", "exec", ns, "ip", "addr", "add", &cidr, "dev", iface]).await
}

/// 호스트 네임스페이스의 인터페이스 활성화
pub async fn bring_up(iface: &str) -> Result<(), NetMeterError> {
    run_ip(&["link", "set", iface, "up"]).await
}

/// 특정 네임스페이스 내의 인터페이스 활성화
pub async fn bring_up_in_ns(ns: &str, iface: &str) -> Result<(), NetMeterError> {
    run_ip(&["netns", "exec", ns, "ip", "link", "set", iface, "up"]).await
}

/// 호스트 네임스페이스에 라우트 추가
pub async fn add_route(dest_cidr: &str, via: &str) -> Result<(), NetMeterError> {
    run_ip(&["route", "add", dest_cidr, "via", via]).await
}

/// 특정 네임스페이스에 라우트 추가
pub async fn add_route_in_ns(ns: &str, dest_cidr: &str, via: &str) -> Result<(), NetMeterError> {
    run_ip(&["netns", "exec", ns, "ip", "route", "add", dest_cidr, "via", via]).await
}



/// 특정 네임스페이스 내 인터페이스에 클라이언트 IP 대역을 할당한다.
///
/// `base_ip`부터 `count`개의 IP를 alias로 추가하며, 이미 존재하는 IP는 건너뛴다.
/// 반환: 할당된 IP 문자열 목록 (base_ip 포함 count개)
pub async fn assign_client_ips_in_ns(
    ns: &str,
    iface: &str,
    base_ip: &str,
    count: u32,
    prefix_len: u8,
) -> Result<Vec<String>, NetMeterError> {
    let base: std::net::Ipv4Addr = base_ip.parse().map_err(|e| {
        NetMeterError::Namespace(format!("Invalid base_ip '{}': {}", base_ip, e))
    })?;
    let base_u32 = u32::from(base);

    let mut ips = Vec::with_capacity(count as usize);
    for i in 0..count {
        let ip = std::net::Ipv4Addr::from(base_u32.wrapping_add(i));
        let ip_str = ip.to_string();
        let cidr = format!("{}/{}", ip_str, prefix_len);
        let result = run_ip(&["netns", "exec", ns, "ip", "addr", "add", &cidr, "dev", iface]).await;
        if let Err(e) = result {
            // 이미 할당된 IP는 무시 (커널 버전에 따라 메시지가 다름)
            if !is_addr_exists_error(&e.to_string()) {
                return Err(e);
            }
        }
        ips.push(ip_str);
    }
    Ok(ips)
}

/// 특정 네임스페이스 내 부모 인터페이스에 단일 VLAN 서브인터페이스를 생성한다.
///
/// 커널 모듈 `8021q`가 로드되어 있어야 한다.
/// 반환: 생성된 서브인터페이스 이름 (예: "veth-c1.100")
pub async fn add_vlan_subif_in_ns(
    ns: &str,
    parent: &str,
    vid: u16,
    proto: VlanProto,
) -> Result<String, NetMeterError> {
    let subif = format!("{}.{}", parent, vid);
    let proto_str = proto.kernel_str();
    run_ip(&[
        "netns", "exec", ns, "ip", "link", "add",
        "link", parent,
        "name", &subif,
        "type", "vlan",
        "id", &vid.to_string(),
        "proto", proto_str,
    ])
    .await?;
    run_ip(&["netns", "exec", ns, "ip", "link", "set", &subif, "up"]).await?;
    Ok(subif)
}

// ---------------------------------------------------------------------------
// 호스트 레벨 유틸리티 (External Port 모드용)
// ---------------------------------------------------------------------------

/// 호스트 인터페이스의 IP를 모두 제거한다 (flush).
pub async fn flush_iface(iface: &str) -> Result<(), NetMeterError> {
    run_ip(&["addr", "flush", "dev", iface]).await
}

/// 호스트 인터페이스에서 특정 IP를 제거한다.
pub async fn del_ip(iface: &str, addr: &str, prefix_len: u8) -> Result<(), NetMeterError> {
    let cidr = format!("{}/{}", addr, prefix_len);
    run_ip(&["addr", "del", &cidr, "dev", iface]).await
}

/// 호스트 인터페이스에 IP 대역을 할당한다.
///
/// base_ip 부터 count개의 IP를 순차적으로 추가. 이미 존재하는 IP는 건너뜀.
/// 반환: 할당된 IP 문자열 목록.
pub async fn assign_ips(
    iface: &str,
    base_ip: &str,
    count: u32,
    prefix_len: u8,
) -> Result<Vec<String>, NetMeterError> {
    let base: std::net::Ipv4Addr = base_ip.parse().map_err(|e| {
        NetMeterError::Namespace(format!("Invalid base_ip '{}': {}", base_ip, e))
    })?;
    let base_u32 = u32::from(base);
    let mut ips = Vec::with_capacity(count as usize);
    for i in 0..count {
        let ip = std::net::Ipv4Addr::from(base_u32.wrapping_add(i));
        let ip_str = ip.to_string();
        let cidr = format!("{}/{}", ip_str, prefix_len);
        let result = run_ip(&["addr", "add", &cidr, "dev", iface]).await;
        if let Err(e) = result {
            if !is_addr_exists_error(&e.to_string()) {
                return Err(e);
            }
        }
        ips.push(ip_str);
    }
    Ok(ips)
}

/// 호스트 인터페이스에 단일 VLAN 서브인터페이스를 생성한다.
///
/// 반환: 생성된 서브인터페이스 이름 (예: "eth1.100")
pub async fn add_vlan_subif(
    parent: &str,
    vid: u16,
    proto: VlanProto,
) -> Result<String, NetMeterError> {
    let subif = format!("{}.{}", parent, vid);
    let proto_str = proto.kernel_str();
    run_ip(&[
        "link", "add", "link", parent, "name", &subif,
        "type", "vlan", "id", &vid.to_string(), "proto", proto_str,
    ])
    .await?;
    run_ip(&["link", "set", &subif, "up"]).await?;
    Ok(subif)
}

/// 호스트 인터페이스에 QinQ (이중 태그) 서브인터페이스를 생성한다.
///
/// 반환: 최종 서브인터페이스 이름 (예: "eth1.100.200")
pub async fn add_qinq_subif(
    parent: &str,
    outer_vid: u16,
    inner_vid: u16,
    outer_proto: VlanProto,
) -> Result<String, NetMeterError> {
    let outer_subif = add_vlan_subif(parent, outer_vid, outer_proto).await?;
    let inner_subif = add_vlan_subif(&outer_subif, inner_vid, VlanProto::Dot1Q).await?;
    Ok(inner_subif)
}

/// 호스트 인터페이스(링크)를 삭제한다.
pub async fn del_link(name: &str) -> Result<(), NetMeterError> {
    run_ip(&["link", "del", name]).await
}

/// 호스트 인터페이스에 static ARP 엔트리를 추가한다 (이미 있으면 덮어씀).
pub async fn set_neigh(ip: &str, mac: &str, iface: &str) -> Result<(), NetMeterError> {
    run_ip(&["neigh", "replace", ip, "lladdr", mac, "dev", iface, "nud", "permanent"]).await
}

/// 호스트 인터페이스에서 ARP 엔트리를 삭제한다.
pub async fn del_neigh(ip: &str, iface: &str) -> Result<(), NetMeterError> {
    run_ip(&["neigh", "del", ip, "dev", iface]).await
}

/// 인터페이스가 존재하는지 확인한다. `ip link show {iface}` 성공 여부로 판단.
pub async fn check_iface(iface: &str) -> bool {
    tokio::process::Command::new("ip")
        .args(["link", "show", iface])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 인터페이스의 Promiscuous 모드를 설정한다.
pub async fn set_promisc(iface: &str, on: bool) -> Result<(), NetMeterError> {
    let flag = if on { "on" } else { "off" };
    run_ip(&["link", "set", iface, "promisc", flag]).await
}

/// 인터페이스의 MTU를 설정한다.
pub async fn set_mtu(iface: &str, mtu: u16) -> Result<(), NetMeterError> {
    run_ip(&["link", "set", iface, "mtu", &mtu.to_string()]).await
}

// ---------------------------------------------------------------------------
// 네임스페이스 내 VLAN 서브인터페이스 (기존)
// ---------------------------------------------------------------------------

/// 특정 네임스페이스 내 QinQ (이중 태그) 서브인터페이스를 생성한다.
///
/// outer subif → inner subif 순서로 생성한다.
/// 반환: 최종 서브인터페이스 이름 (예: "veth-c1.100.200")
pub async fn add_qinq_subif_in_ns(
    ns: &str,
    parent: &str,
    outer_vid: u16,
    inner_vid: u16,
    outer_proto: VlanProto,
) -> Result<String, NetMeterError> {
    // outer subif 생성
    let outer_subif = add_vlan_subif_in_ns(ns, parent, outer_vid, outer_proto).await?;
    // inner subif 생성 (outer_subif 위에, 항상 Dot1Q)
    let inner_subif = add_vlan_subif_in_ns(ns, &outer_subif, inner_vid, VlanProto::Dot1Q).await?;
    Ok(inner_subif)
}

/// policy routing rule 추가: ip rule add from <cidr> lookup <table>
pub async fn add_ip_rule(from_cidr: &str, table: u32) -> Result<(), NetMeterError> {
    run_ip(&["rule", "add", "from", from_cidr, "lookup", &table.to_string()]).await
}

/// policy routing rule 삭제: ip rule del from <cidr> lookup <table>
pub async fn del_ip_rule(from_cidr: &str, table: u32) -> Result<(), NetMeterError> {
    run_ip(&["rule", "del", "from", from_cidr, "lookup", &table.to_string()]).await
}

/// 특정 라우팅 테이블에 default route 추가 (dev 직접 지정)
pub async fn add_route_table_dev(dev: &str, table: u32) -> Result<(), NetMeterError> {
    run_ip(&["route", "add", "default", "dev", dev, "table", &table.to_string()]).await
}

/// 특정 라우팅 테이블을 flush
pub async fn flush_route_table(table: u32) -> Result<(), NetMeterError> {
    run_ip(&["route", "flush", "table", &table.to_string()]).await
}

/// ip rule / route 중복 추가 에러 여부 (RTNETLINK answers: File exists)
pub(crate) fn is_rule_exists_error(msg: &str) -> bool {
    msg.contains("File exists")
}

/// IP 주소 중복 할당 에러 여부를 판단한다.
///
/// 커널 버전에 따라 에러 메시지가 다르다:
///   - 구버전: "RTNETLINK answers: File exists"
///   - 신버전: "Error: ipv4: Address already assigned."
pub(crate) fn is_addr_exists_error(msg: &str) -> bool {
    msg.contains("File exists")
        || msg.contains("Address already assigned")
        || msg.contains("RTNETLINK")
}
