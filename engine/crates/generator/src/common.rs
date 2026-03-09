/// Generator 공통 유틸리티.
///
/// http1 / http2 / tcp 세 모듈에 걸쳐 중복되던 함수들을 한 곳에 모은다.
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use net_meter_metrics::Collector;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpSocket, TcpStream};

// ---------------------------------------------------------------------------
// 버퍼 상수
// ---------------------------------------------------------------------------

/// 대용량 페이로드 전송 시 청크 단위 크기 (64 KiB).
///
/// 단일 Vec 할당 없이 이 정적 버퍼를 반복해서 전송한다.
pub const SEND_CHUNK_SIZE: usize = 65_536;
pub static ZERO_CHUNK: [u8; SEND_CHUNK_SIZE] = [0u8; SEND_CHUNK_SIZE];

// ---------------------------------------------------------------------------
// TCP 연결
// ---------------------------------------------------------------------------

/// 소스 IP를 바인딩하여 TCP 연결을 수립한다.
pub async fn connect_tcp(addr: &str, src_ip: Option<IpAddr>) -> std::io::Result<TcpStream> {
    if let Some(src) = src_ip {
        let server_addr: SocketAddr = addr
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let sock = TcpSocket::new_v4()?;
        sock.bind(SocketAddr::new(src, 0))?;
        sock.connect(server_addr).await
    } else {
        TcpStream::connect(addr).await
    }
}

// ---------------------------------------------------------------------------
// 대기
// ---------------------------------------------------------------------------

/// `deadline`까지 sleep한다. `deadline`이 `None`이면 영구 대기한다.
pub async fn wait_deadline(deadline: Option<Instant>) {
    if let Some(dl) = deadline {
        tokio::time::sleep(dl.saturating_duration_since(Instant::now())).await;
    } else {
        std::future::pending::<()>().await;
    }
}

// ---------------------------------------------------------------------------
// 대용량 송신 헬퍼
// ---------------------------------------------------------------------------

/// `n` 바이트의 0x00을 `SEND_CHUNK_SIZE` 단위 청크로 나눠 스트림에 쓴다.
///
/// 단일 `Vec<u8>` 할당 없이 정적 `ZERO_CHUNK` 버퍼를 재사용하므로
/// 대용량 페이로드(수백 MB)도 일정한 메모리 사용량으로 전송할 수 있다.
pub async fn write_zeroes<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    n: usize,
) -> std::io::Result<()> {
    let mut remaining = n;
    while remaining > 0 {
        let chunk_len = remaining.min(SEND_CHUNK_SIZE);
        writer.write_all(&ZERO_CHUNK[..chunk_len]).await?;
        remaining -= chunk_len;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP 경로 빌더 (http1 / http2 공용)
// ---------------------------------------------------------------------------

/// 기본 경로에 `extra_bytes` 만큼의 쿼리 파라미터 패딩을 추가한다.
pub fn build_path(base: &str, extra_bytes: Option<usize>) -> String {
    match extra_bytes {
        None | Some(0) => base.to_string(),
        Some(n) => format!("{}?x={}", base, "a".repeat(n)),
    }
}

// ---------------------------------------------------------------------------
// TLS SNI 헬퍼 (http1 / http2 공용)
// ---------------------------------------------------------------------------

/// TLS SNI 서버 이름을 결정한다.
///
/// IP 주소 입력 시 RFC 6066 규정에 따라 `"localhost"`로 대체한다.
pub fn resolve_tls_sni(name: &str) -> rustls::pki_types::ServerName<'static> {
    let effective = if name.parse::<std::net::IpAddr>().is_ok() { "localhost" } else { name };
    rustls::pki_types::ServerName::try_from(effective.to_string())
        .unwrap_or_else(|_| {
            rustls::pki_types::ServerName::try_from("localhost".to_string()).unwrap()
        })
}

// ---------------------------------------------------------------------------
// Dual-collector 헬퍼 — global + proto 양쪽에 동시 기록
// ---------------------------------------------------------------------------

#[inline]
pub fn record_attempt(g: &Collector, p: &Collector) {
    g.record_connection_attempt();
    p.record_connection_attempt();
}

#[inline]
pub fn record_established(g: &Collector, p: &Collector) {
    g.record_connection_established();
    p.record_connection_established();
}

#[inline]
pub fn record_failed(g: &Collector, p: &Collector) {
    g.record_connection_failed();
    p.record_connection_failed();
}

#[inline]
pub fn record_timeout(g: &Collector, p: &Collector) {
    g.record_timeout();
    p.record_timeout();
}

#[inline]
pub fn record_response(g: &Collector, p: &Collector, status: u16, bytes: u64, us: u64) {
    g.record_response(status, bytes, us);
    p.record_response(status, bytes, us);
}
