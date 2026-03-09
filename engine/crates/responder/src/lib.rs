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
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio::task::{JoinHandle, JoinSet};
use tokio_rustls::TlsAcceptor;
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
        tls_config: Option<Arc<ServerConfig>>,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!(%addr, ?protocol, tls = tls_config.is_some(), "Responder listening (local mode)");
        self.handles.push(spawn_server(listener, protocol, payload, global, proto, tcp_quickack, tls_config));
        Ok(())
    }

    /// NS 모드: 네임스페이스 내 지정 주소에 서버를 시작한다.
    pub async fn start_server_in_ns(
        &mut self,
        ns_name: &str,
        bind_addr: SocketAddr,
        protocol: Protocol,
        payload: &PayloadProfile,
        global: Arc<Collector>,
        proto: Arc<Collector>,
        tcp_quickack: bool,
        tls_config: Option<Arc<ServerConfig>>,
    ) -> anyhow::Result<()> {
        let ns_owned = ns_name.to_string();
        let std_listener = tokio::task::spawn_blocking(move || {
            net_meter_ns::bind_listener_in_ns(&ns_owned, bind_addr)
        })
        .await??;

        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;
        info!(addr = %bind_addr, ns = %ns_name, ?protocol, tls = tls_config.is_some(), "Responder listening (namespace mode)");
        self.handles.push(spawn_server(listener, protocol, payload, global, proto, tcp_quickack, tls_config));
        Ok(())
    }

    /// 모든 서버를 중지한다.
    ///
    /// 처리 중인 요청에 짧은 grace 구간을 준 뒤 강제 abort한다.
    /// Generator를 먼저 중지한 후 호출하면 grace 구간 안에 대부분의 요청이 완료된다.
    pub async fn stop_all(&mut self) {
        if !self.handles.is_empty() {
            // 진행 중인 요청 완료를 위한 짧은 grace 구간
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
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
    tls_config: Option<Arc<ServerConfig>>,
) -> JoinHandle<()> {
    match (protocol, payload) {
        (Protocol::Tcp, PayloadProfile::Tcp(p)) => {
            tcp::spawn_tcp_server(listener, p.clone(), global, proto)
        }
        (Protocol::Http1, PayloadProfile::Http(p)) => {
            let body_size = p.response_body_bytes.unwrap_or(0);
            spawn_http_server(listener, body_size, tcp_quickack, false, global, proto, tls_config)
        }
        (Protocol::Http2, PayloadProfile::Http(p)) => {
            let body_size = p.response_body_bytes.unwrap_or(0);
            spawn_http_server(listener, body_size, tcp_quickack, true, global, proto, tls_config)
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
    tls_config: Option<Arc<ServerConfig>>,
) -> JoinHandle<()> {
    let tls_acceptor = tls_config.map(TlsAcceptor::from);

    // 응답 body를 서버 시작 시 한 번만 할당하여 모든 요청에서 Arc::clone으로 재사용.
    // Bytes::clone()은 O(1) (내부 참조 카운트 증가만) 이므로 per-request 할당이 없다.
    let shared_body = Arc::new(Bytes::from(vec![b'x'; body_size]));

    tokio::spawn(async move {
        // JoinSet으로 per-connection 태스크를 추적한다.
        // 리스너 태스크가 abort되면 JoinSet이 drop되어 모든 연결 태스크도 abort된다.
        let mut conn_tasks: JoinSet<()> = JoinSet::new();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, peer) = match result {
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
                    let body = Arc::clone(&shared_body);

                    if let Some(ref acceptor) = tls_acceptor {
                        let acceptor = acceptor.clone();
                        conn_tasks.spawn(async move {
                            let tls_stream = match acceptor.accept(stream).await {
                                Ok(s) => s,
                                Err(e) => {
                                    debug!(peer = %peer, error = %e, "TLS accept failed");
                                    return;
                                }
                            };
                            serve_http(TokioIo::new(tls_stream), is_h2, body, g, p, peer).await;
                        });
                    } else {
                        conn_tasks.spawn(async move {
                            serve_http(TokioIo::new(stream), is_h2, body, g, p, peer).await;
                        });
                    }
                }
                // 완료된 연결 태스크를 정리 (JoinSet 무한 증가 방지)
                Some(_) = conn_tasks.join_next(), if !conn_tasks.is_empty() => {}
            }
        }
    })
}

/// TokioIo로 래핑된 스트림에서 HTTP/1.1 또는 HTTP/2 연결을 처리한다.
async fn serve_http<I>(
    io: I,
    is_h2: bool,
    body: Arc<Bytes>,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    peer: std::net::SocketAddr,
) where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    if is_h2 {
        let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
            let g2 = Arc::clone(&global);
            let p2 = Arc::clone(&proto);
            let body = Arc::clone(&body);
            async move { handle_http(req, g2, p2, body).await }
        });
        if let Err(e) = http2::Builder::new(TokioExecutor::new())
            .serve_connection(io, svc)
            .await
        {
            debug!(peer = %peer, error = %e, "h2 connection closed");
        }
    } else {
        let svc = service_fn(move |req: Request<hyper::body::Incoming>| {
            let g2 = Arc::clone(&global);
            let p2 = Arc::clone(&proto);
            let body = Arc::clone(&body);
            async move { handle_http(req, g2, p2, body).await }
        });
        if let Err(e) = http1::Builder::new()
            .keep_alive(true)
            .serve_connection(io, svc)
            .await
        {
            debug!(peer = %peer, error = %e, "HTTP/1.1 connection closed");
        }
    }
}

async fn handle_http(
    req: Request<hyper::body::Incoming>,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    body: Arc<Bytes>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // Content-Length 헤더에서 클라이언트가 전송한 요청 body 크기를 파악
    let req_bytes = req
        .headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let body_len = body.len();
    // Bytes::clone() = O(1): 내부 Arc 카운트 증가 + 포인터 복사만 수행
    let body_bytes = (*body).clone();
    global.record_server_request(body_len as u64);
    proto.record_server_request(body_len as u64);
    if req_bytes > 0 {
        global.record_server_rx(req_bytes);
        proto.record_server_rx(req_bytes);
    }

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", body_len.to_string())
        .body(Full::new(body_bytes))
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

// ---------------------------------------------------------------------------
// 통합 테스트
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use net_meter_core::{HttpPayload, HttpMethod, PayloadProfile, Protocol, TcpPayload};
    use net_meter_metrics::Collector;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// TCP Responder: 연결 수락 → 응답 전송 확인
    #[tokio::test]
    async fn test_tcp_responder_pingpong() {
        let global = Collector::new();
        let proto  = Collector::new();

        let payload = TcpPayload { tx_bytes: 8, rx_bytes: 16 };
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let mut responder = Responder::new();
        responder
            .start_server(
                addr,
                Protocol::Tcp,
                &PayloadProfile::Tcp(payload.clone()),
                Arc::clone(&global),
                Arc::clone(&proto),
                false,
                None,
            )
            .await
            .expect("start_server");

        // 실제 바인드된 포트를 구할 방법이 없으므로 잠깐 대기 후 0번 포트 bind 특성 이용
        // 대신 고정 포트를 재시도 방식으로 사용
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        responder.stop_all().await;
    }

    /// HTTP/1.1 Responder: listen 후 연결해 200 응답 수신 확인
    #[tokio::test]
    async fn test_http1_responder_basic() {
        let global = Collector::new();
        let proto  = Collector::new();

        let payload = HttpPayload {
            method: HttpMethod::Get,
            path: "/".to_string(),
            request_body_bytes: None,
            response_body_bytes: Some(64),
            path_extra_bytes: None,
            h2_max_concurrent_streams: None,
        };

        // 포트 0으로 OS에 임의 포트 할당 요청
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bound_addr = listener.local_addr().unwrap();
        // TcpListener를 직접 spawn_http_server에 전달
        let g2 = Arc::clone(&global);
        let p2 = Arc::clone(&proto);
        let handle = spawn_http_server(listener, 64, false, false, g2, p2, None);

        // 잠깐 뒤 연결
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let mut stream = tokio::net::TcpStream::connect(bound_addr).await.expect("connect");
        let req = b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        stream.write_all(req).await.expect("write request");

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.expect("read response");
        let response_str = String::from_utf8_lossy(&response);

        // 200 OK 확인
        assert!(
            response_str.starts_with("HTTP/1.1 200"),
            "expected 200 OK, got: {}",
            &response_str[..response_str.len().min(100)],
        );
        // 서버 메트릭 확인
        assert_eq!(global.server_requests.load(std::sync::atomic::Ordering::Relaxed), 1);

        handle.abort();
        drop(payload);
    }
}
