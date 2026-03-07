pub mod tcp;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use net_meter_core::{PayloadProfile, Protocol};
use net_meter_metrics::Collector;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// 가상 서버 관리자.
///
/// 여러 서버 인스턴스(TCP/HTTP1/HTTP2)를 관리한다.
/// 시험당 `start_server` 또는 `start_server_in_ns`를 여러 번 호출해
/// 고유 서버 엔드포인트마다 리스너를 시작한다.
pub struct Responder {
    handles: Vec<JoinHandle<()>>,
}

impl Responder {
    pub fn new() -> Self {
        Self { handles: Vec::new() }
    }

    /// 로컬 모드: 지정 주소에 서버를 시작한다.
    pub async fn start_server(
        &mut self,
        addr: SocketAddr,
        protocol: Protocol,
        payload: &PayloadProfile,
        global: Arc<Collector>,
        proto: Arc<Collector>,
        tcp_quickack: bool,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!(%addr, ?protocol, "Responder listening (local mode)");
        self.handles.push(spawn_server(listener, protocol, payload, global, proto, tcp_quickack));
        Ok(())
    }

    /// NS 모드: 네임스페이스 내 지정 주소에 서버를 시작한다.
    ///
    /// spawn_blocking 내부에서 setns → bind → NS 복구를 수행해
    /// 소켓 FD를 server NS에 귀속시킨 뒤 tokio TcpListener로 변환한다.
    pub async fn start_server_in_ns(
        &mut self,
        ns_name: &str,
        bind_addr: SocketAddr,
        protocol: Protocol,
        payload: &PayloadProfile,
        global: Arc<Collector>,
        proto: Arc<Collector>,
        tcp_quickack: bool,
    ) -> anyhow::Result<()> {
        let ns_owned = ns_name.to_string();
        let std_listener = tokio::task::spawn_blocking(move || {
            net_meter_ns::bind_listener_in_ns(&ns_owned, bind_addr)
        })
        .await??;

        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;
        info!(addr = %bind_addr, ns = %ns_name, ?protocol, "Responder listening (namespace mode)");
        self.handles.push(spawn_server(listener, protocol, payload, global, proto, tcp_quickack));
        Ok(())
    }

    /// 모든 서버를 중지한다.
    pub fn stop_all(&mut self) {
        for h in self.handles.drain(..) {
            h.abort();
        }
    }
}

impl Default for Responder {
    fn default() -> Self {
        Self::new()
    }
}

/// 프로토콜과 페이로드에 맞는 서버를 스폰한다.
fn spawn_server(
    listener: TcpListener,
    protocol: Protocol,
    payload: &PayloadProfile,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    tcp_quickack: bool,
) -> JoinHandle<()> {
    match (protocol, payload) {
        (Protocol::Tcp, PayloadProfile::Tcp(p)) => {
            tcp::spawn_tcp_server(listener, p.clone(), global, proto)
        }
        (Protocol::Http1, PayloadProfile::Http(p)) => {
            let body_size = p.response_body_bytes.unwrap_or(0);
            spawn_http_server(listener, body_size, tcp_quickack, false, global, proto)
        }
        (Protocol::Http2, PayloadProfile::Http(p)) => {
            let body_size = p.response_body_bytes.unwrap_or(0);
            spawn_http_server(listener, body_size, tcp_quickack, true, global, proto)
        }
        _ => {
            tracing::error!(?protocol, "Protocol/payload mismatch in responder — listener not started");
            tokio::spawn(async {})
        }
    }
}

fn spawn_http_server(
    listener: TcpListener,
    body_size: usize,
    tcp_quickack: bool,
    is_h2: bool,
    global: Arc<Collector>,
    proto: Arc<Collector>,
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

            if tcp_quickack {
                if let Err(e) = set_quickack(&stream) {
                    warn!(peer = %peer, error = %e, "Failed to set TCP_QUICKACK");
                }
            }

            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            if is_h2 {
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                        let g2 = Arc::clone(&g);
                        let p2 = Arc::clone(&p);
                        async move { handle_http(req, g2, p2, body_size).await }
                    });
                    if let Err(e) = http2::Builder::new(TokioExecutor::new())
                        .serve_connection(io, svc)
                        .await
                    {
                        debug!(peer = %peer, error = %e, "h2 connection closed");
                    }
                });
            } else {
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
                        let g2 = Arc::clone(&g);
                        let p2 = Arc::clone(&p);
                        async move { handle_http(req, g2, p2, body_size).await }
                    });
                    if let Err(e) = http1::Builder::new()
                        .keep_alive(true)
                        .serve_connection(io, svc)
                        .await
                    {
                        debug!(peer = %peer, error = %e, "HTTP/1.1 connection closed");
                    }
                });
            }
        }
    })
}

async fn handle_http(
    _req: Request<hyper::body::Incoming>,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    body_size: usize,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let body = Bytes::from(vec![b'x'; body_size]);
    global.record_server_request(body_size as u64);
    proto.record_server_request(body_size as u64);

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", body_size.to_string())
        .body(Full::new(body))
        .unwrap())
}

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
    if ret == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

#[cfg(not(target_os = "linux"))]
fn set_quickack(_stream: &tokio::net::TcpStream) -> std::io::Result<()> {
    Ok(())
}
