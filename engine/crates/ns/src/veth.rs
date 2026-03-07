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
            // "File exists" 오류는 이미 할당된 IP이므로 무시
            let msg = e.to_string();
            if !msg.contains("File exists") && !msg.contains("RTNETLINK") {
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
