use net_meter_core::NetMeterError;

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
