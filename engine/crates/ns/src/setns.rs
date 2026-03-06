use std::net::SocketAddr;

use net_meter_core::NetMeterError;
use nix::sched::{setns, CloneFlags};

/// 현재 스레드의 네트워크 네임스페이스를 임시로 교체하여 TcpListener를 생성한다.
///
/// # 주의
/// 반드시 `spawn_blocking` 내부에서 호출해야 한다.
/// tokio의 스레드 풀 오염을 막기 위해 호출 완료 후 항상 원래 NS로 복구한다.
///
/// # 동작
/// 1. 현재(호스트) NS fd 저장
/// 2. 타깃 NS 진입
/// 3. `std::net::TcpListener` 바인드
/// 4. 호스트 NS 복구
/// 5. TcpListener 반환 (소켓 FD는 타깃 NS 소속 유지)
pub fn bind_listener_in_ns(
    ns_name: &str,
    addr: SocketAddr,
) -> Result<std::net::TcpListener, NetMeterError> {
    let ns_path = format!("/var/run/netns/{}", ns_name);

    // 현재 NS 저장
    let orig = std::fs::File::open("/proc/self/ns/net").map_err(NetMeterError::Io)?;
    // 타깃 NS 진입 (&file: AsFd)
    let ns_file = std::fs::File::open(&ns_path)
        .map_err(|e| NetMeterError::Namespace(format!("open ns {}: {}", ns_name, e)))?;
    setns(&ns_file, CloneFlags::CLONE_NEWNET)
        .map_err(|e| NetMeterError::Namespace(format!("setns {}: {}", ns_name, e)))?;

    // 소켓 생성 및 바인드
    let listener = std::net::TcpListener::bind(addr).map_err(|e| {
        // 실패해도 NS 복구 시도
        let _ = setns(&orig, CloneFlags::CLONE_NEWNET);
        NetMeterError::Io(e)
    })?;

    // 호스트 NS 복구
    setns(&orig, CloneFlags::CLONE_NEWNET)
        .map_err(|e| NetMeterError::Namespace(format!("restore host ns: {}", e)))?;

    Ok(listener)
}

/// 주어진 네임스페이스 내에서 `tokio::net::TcpSocket`을 생성한다.
///
/// 반환된 소켓은 해당 NS에 소속되어 있어, connect() 시 해당 NS의 라우팅을 사용한다.
/// 반드시 `spawn_blocking` 내부에서 호출해야 한다.
pub fn create_socket_in_ns(ns_name: &str) -> Result<tokio::net::TcpSocket, NetMeterError> {
    let ns_path = format!("/var/run/netns/{}", ns_name);

    let orig = std::fs::File::open("/proc/self/ns/net").map_err(NetMeterError::Io)?;
    let ns_file = std::fs::File::open(&ns_path)
        .map_err(|e| NetMeterError::Namespace(format!("open ns {}: {}", ns_name, e)))?;
    setns(&ns_file, CloneFlags::CLONE_NEWNET)
        .map_err(|e| NetMeterError::Namespace(format!("setns {}: {}", ns_name, e)))?;

    let socket = tokio::net::TcpSocket::new_v4().map_err(|e| {
        let _ = setns(&orig, CloneFlags::CLONE_NEWNET);
        NetMeterError::Io(e)
    })?;

    setns(&orig, CloneFlags::CLONE_NEWNET)
        .map_err(|e| NetMeterError::Namespace(format!("restore host ns: {}", e)))?;

    Ok(socket)
}
