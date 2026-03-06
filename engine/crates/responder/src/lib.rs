use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use net_meter_metrics::Collector;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// 가상 HTTP/1.1 서버.
///
/// Generator의 요청을 받아 응답하며 서버 사이드 계측을 수행한다.
/// Phase 3: hyper 1.0 직접 사용 (axum 오버헤드 제거, keep-alive 내장).
pub struct Responder {
    handle: Option<JoinHandle<()>>,
}

impl Responder {
    pub fn new() -> Self {
        Self { handle: None }
    }

    /// 호스트 네임스페이스에서 지정 주소로 HTTP 서버를 시작한다.
    pub async fn start(
        &mut self,
        addr: SocketAddr,
        metrics: Arc<Collector>,
        response_body_bytes: Option<usize>,
        tcp_quickack: bool,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!(%addr, tcp_quickack, "Responder listening (local mode)");
        self.handle = Some(spawn_server(listener, metrics, response_body_bytes.unwrap_or(0), tcp_quickack));
        Ok(())
    }

    /// server 네임스페이스 내에서 소켓을 바인드한 후 HTTP 서버를 시작한다.
    ///
    /// spawn_blocking으로 setns를 수행하여 소켓 FD를 server NS에 귀속시킨다.
    pub async fn start_in_ns(
        &mut self,
        ns_name: &str,
        port: u16,
        metrics: Arc<Collector>,
        response_body_bytes: Option<usize>,
        tcp_quickack: bool,
    ) -> anyhow::Result<()> {
        let ns_owned = ns_name.to_string();
        let ns_for_log = ns_owned.clone();
        let std_listener = tokio::task::spawn_blocking(move || {
            net_meter_ns::bind_listener_in_ns(
                &ns_owned,
                format!("0.0.0.0:{}", port).parse().unwrap(),
            )
        })
        .await??;

        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;
        info!(port, ns = %ns_for_log, tcp_quickack, "Responder listening (namespace mode)");
        self.handle = Some(spawn_server(listener, metrics, response_body_bytes.unwrap_or(0), tcp_quickack));
        Ok(())
    }

    /// 서버를 중지한다.
    pub fn stop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Default for Responder {
    fn default() -> Self {
        Self::new()
    }
}

/// TCP accept 루프를 tokio 태스크로 구동한다.
fn spawn_server(
    listener: TcpListener,
    metrics: Arc<Collector>,
    body_size: usize,
    tcp_quickack: bool,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(error = %e, "Accept error");
                    continue;
                }
            };

            // TCP_QUICKACK: Delayed ACK 비활성화 (Linux only)
            // accept 직후 소켓에 설정해야 각 연결에 적용된다.
            if tcp_quickack {
                if let Err(e) = set_quickack(&stream) {
                    warn!(peer = %peer, error = %e, "Failed to set TCP_QUICKACK");
                }
            }

            let m = Arc::clone(&metrics);
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let m2 = Arc::clone(&m);
                    async move { handle_request(req, m2, body_size).await }
                });

                if let Err(e) = http1::Builder::new()
                    .keep_alive(true)
                    .serve_connection(io, svc)
                    .await
                {
                    // 클라이언트 끊김은 정상적인 경우가 많으므로 debug 레벨
                    debug!(peer = %peer, error = %e, "Connection closed");
                }
            });
        }
    })
}

/// accept된 소켓에 TCP_QUICKACK를 설정한다 (Delayed ACK 비활성화).
///
/// TCP_QUICKACK는 Linux 전용 소켓 옵션이다.
/// false → Delayed ACK 활성화 (기본, ~40ms 지연 후 ACK)
/// true  → Delayed ACK 비활성화 (즉시 ACK, 지연 민감 시험에 사용)
#[cfg(target_os = "linux")]
fn set_quickack(stream: &tokio::net::TcpStream) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let fd = stream.as_raw_fd();
    let val: libc::c_int = 1;
    let ret = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_QUICKACK,
            &val as *const _ as *const libc::c_void,
            std::mem::size_of_val(&val) as libc::socklen_t,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn set_quickack(_stream: &tokio::net::TcpStream) -> std::io::Result<()> {
    Ok(())
}

/// 개별 HTTP 요청 처리기.
///
/// 고정 크기 body를 응답하고 서버 사이드 계측을 기록한다.
async fn handle_request(
    _req: Request<hyper::body::Incoming>,
    metrics: Arc<Collector>,
    body_size: usize,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let body = Bytes::from(vec![b'x'; body_size]);
    metrics.record_server_request(body_size as u64);

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", body_size.to_string())
        .body(Full::new(body))
        .unwrap())
}
