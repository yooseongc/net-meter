use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{Method, Request};
use net_meter_core::{HttpMethod, HttpPayload, LoadConfig, TestType};
use net_meter_metrics::Collector;
use rustls::ClientConfig;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tracing::debug;

/// HTTP/2 트래픽 발생 진입점 (h2c 또는 TLS h2)
pub async fn run(
    test_type: TestType,
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let num_conn = load.effective_num_connections() as usize;
    let streams = payload.h2_max_concurrent_streams.unwrap_or(1) as usize;
    match test_type {
        TestType::Cps => run_cps(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip).await,
        TestType::Cc => run_cc(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip).await,
        TestType::Bw => run_bw(addr, load, payload, num_conn, streams, global, proto, shutdown, deadline, tls, src_ip).await,
    }
}

/// src_ip를 bind한 TCP 연결 수립
async fn connect_tcp(addr: &str, src_ip: Option<IpAddr>) -> std::io::Result<TcpStream> {
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
// CPS: rate limiter 없이 h2 연결→요청→close 루프를 최대 속도로 반복
// ---------------------------------------------------------------------------

async fn run_cps(
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    num_conn: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    if num_conn <= 1 {
        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
            let cycle = single_request_h2(
                &addr, &host, method.clone(), &path, req_body,
                Arc::clone(&global), Arc::clone(&proto),
                connect_to, response_to, tls.clone(), src_ip,
            );
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                _ = cycle => {}
            }
        }
    } else {
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut handles = Vec::with_capacity(num_conn);
        for _ in 0..num_conn {
            let running = Arc::clone(&running);
            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            let a = addr.clone();
            let h = host.clone();
            let me = method.clone();
            let pa = path.clone();
            let tls = tls.clone();
            handles.push(tokio::spawn(async move {
                loop {
                    if !running.load(std::sync::atomic::Ordering::Relaxed) { break; }
                    if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                    single_request_h2(&a, &h, me.clone(), &pa, req_body, Arc::clone(&g), Arc::clone(&p), connect_to, response_to, tls.clone(), src_ip).await;
                }
            }));
        }
        tokio::select! {
            _ = &mut shutdown => {}
            _ = wait_deadline(deadline) => {}
        }
        running.store(false, std::sync::atomic::Ordering::Relaxed);
        for h in handles { h.abort(); }
    }
}

// ---------------------------------------------------------------------------
// CC: num_conn 연결 유지. 1초 간격 단일 스트림 요청. 연결 수 측정 집중.
// ---------------------------------------------------------------------------

async fn run_cc(
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    num_conn: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let h = host.clone();
        let me = method.clone();
        let pa = path.clone();
        let tls = tls.clone();
        handles.push(tokio::spawn(async move {
            h2_cc_worker(&a, &h, me, &pa, g, p, connect_to, response_to, deadline, tls, src_ip).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// BW: num_conn 연결 × streams_per_conn 스트림 유지 (최대 처리량)
// ---------------------------------------------------------------------------

async fn run_bw(
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    num_conn: usize,
    streams_per_conn: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let h = host.clone();
        let me = method.clone();
        let pa = path.clone();
        let tls = tls.clone();
        handles.push(tokio::spawn(async move {
            connection_worker(&a, &h, me, &pa, req_body, g, p, connect_to, response_to, deadline, streams_per_conn, tls, src_ip).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// 단일 h2 요청 (CPS)
// ---------------------------------------------------------------------------

async fn single_request_h2(
    addr: &str,
    host: &str,
    method: Method,
    path: &str,
    req_body_bytes: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    let total_start = Instant::now();
    record_attempt(&global, &proto);

    let connect_start = Instant::now();
    let send_req = match timeout(connect_timeout, connect_h2(addr, &tls, src_ip)).await {
        Ok(Ok(sr)) => {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            sr
        }
        Ok(Err(e)) => {
            debug!(addr, error = %e, "h2 handshake failed");
            record_failed(&global, &proto);
            return;
        }
        Err(_) => {
            debug!(addr, "h2 connect timed out");
            record_failed(&global, &proto);
            record_timeout(&global, &proto);
            return;
        }
    };

    let result = timeout(
        response_timeout,
        send_h2_stream(&send_req, host, method, path, req_body_bytes, &global, &proto),
    )
    .await;

    match result {
        Ok(Ok((status, bytes_rx))) => {
            record_response(&global, &proto, status, bytes_rx, total_start.elapsed().as_micros() as u64);
        }
        Ok(Err(e)) => debug!(addr, error = %e, "h2 stream error"),
        Err(_) => {
            debug!(addr, "h2 stream timed out");
            record_timeout(&global, &proto);
        }
    }
    record_closed(&global, &proto);
}

// ---------------------------------------------------------------------------
// h2 CC 워커: 연결 유지, 1초 간격 단일 요청
// ---------------------------------------------------------------------------

async fn h2_cc_worker(
    addr: &str,
    host: &str,
    method: Method,
    path: &str,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
    deadline: Option<Instant>,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

        record_attempt(&global, &proto);
        let connect_start = Instant::now();
        let send_req = match timeout(connect_timeout, connect_h2(addr, &tls, src_ip)).await {
            Ok(Ok(sr)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                sr
            }
            Ok(Err(e)) => {
                debug!(error = %e, "h2 CC connect failed, retrying");
                record_failed(&global, &proto);
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            Err(_) => {
                debug!("h2 CC connect timed out, retrying");
                record_failed(&global, &proto);
                record_timeout(&global, &proto);
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

            let total_start = Instant::now();
            let result = timeout(
                response_timeout,
                send_h2_stream(&send_req, host, method.clone(), path, 0, &global, &proto),
            ).await;

            match result {
                Ok(Ok((status, bytes_rx))) => {
                    record_response(&global, &proto, status, bytes_rx, total_start.elapsed().as_micros() as u64);
                }
                Ok(Err(_)) => break,
                Err(_) => {
                    record_timeout(&global, &proto);
                    break;
                }
            }

            // CC: 1초 간격 — 연결 유지가 목적
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        record_closed(&global, &proto);
    }
}

// ---------------------------------------------------------------------------
// h2 BW 연결 워커 (최대 처리량)
// ---------------------------------------------------------------------------

async fn connection_worker(
    addr: &str,
    host: &str,
    method: Method,
    path: &str,
    req_body_bytes: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
    deadline: Option<Instant>,
    concurrent_streams: usize,
    tls: Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

        record_attempt(&global, &proto);
        let connect_start = Instant::now();
        let send_req = match timeout(connect_timeout, connect_h2(addr, &tls, src_ip)).await {
            Ok(Ok(sr)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                sr
            }
            Ok(Err(e)) => {
                debug!(error = %e, "h2 connect failed, retrying");
                record_failed(&global, &proto);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(_) => {
                debug!("h2 connect timed out, retrying");
                record_failed(&global, &proto);
                record_timeout(&global, &proto);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        let mut stream_handles = Vec::with_capacity(concurrent_streams);
        for _ in 0..concurrent_streams {
            let sr = send_req.clone();
            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            let h = host.to_string();
            let me = method.clone();
            let pa = path.to_string();

            stream_handles.push(tokio::spawn(async move {
                loop {
                    if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                    let total_start = Instant::now();
                    let result = timeout(
                        response_timeout,
                        send_h2_stream(&sr, &h, me.clone(), &pa, req_body_bytes, &g, &p),
                    ).await;
                    match result {
                        Ok(Ok((status, bytes_rx))) => {
                            record_response(&g, &p, status, bytes_rx, total_start.elapsed().as_micros() as u64);
                        }
                        Ok(Err(_)) => break,
                        Err(_) => record_timeout(&g, &p),
                    }
                }
            }));
        }

        for h in stream_handles { let _ = h.await; }
        record_closed(&global, &proto);
    }
}

// ---------------------------------------------------------------------------
// 헬퍼
// ---------------------------------------------------------------------------

/// h2c 또는 TLS h2 연결을 수립하고 SendRequest를 반환한다.
async fn connect_h2(
    addr: &str,
    tls: &Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
) -> anyhow::Result<h2::client::SendRequest<Bytes>> {
    let tcp = connect_tcp(addr, src_ip).await?;

    if let Some(cfg) = tls {
        let connector = TlsConnector::from(Arc::clone(cfg));
        let sn = rustls::pki_types::ServerName::try_from("localhost")
            .unwrap()
            .to_owned();
        let tls_stream = connector.connect(sn, tcp).await?;
        let (send_req, conn) = h2::client::Builder::new()
            .initial_window_size(1 << 20)
            .handshake::<_, Bytes>(tls_stream)
            .await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("h2 TLS connection closed: {}", e);
            }
        });
        Ok(send_req)
    } else {
        let (send_req, conn) = h2::client::Builder::new()
            .initial_window_size(1 << 20)
            .handshake::<_, Bytes>(tcp)
            .await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("h2c connection closed: {}", e);
            }
        });
        Ok(send_req)
    }
}

async fn send_h2_stream(
    send_req: &h2::client::SendRequest<Bytes>,
    host: &str,
    method: Method,
    path: &str,
    req_body_bytes: usize,
    global: &Arc<Collector>,
    proto: &Arc<Collector>,
) -> anyhow::Result<(u16, u64)> {
    let end_stream = req_body_bytes == 0;
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", host)
        .header("user-agent", "net-meter/0.1")
        .body(())
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut sr = send_req.clone().ready().await.map_err(|e| anyhow::anyhow!(e))?;
    let (response_future, mut send_stream) = sr
        .send_request(request, end_stream)
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut tx_bytes: u64 = 64;
    if req_body_bytes > 0 {
        let body = Bytes::from(vec![0u8; req_body_bytes]);
        tx_bytes += body.len() as u64;
        send_stream.send_data(body, true).map_err(|e| anyhow::anyhow!(e))?;
    }
    global.record_request(tx_bytes);
    proto.record_request(tx_bytes);

    let ttfb_start = Instant::now();
    let response = response_future.await.map_err(|e| anyhow::anyhow!(e))?;
    let ttfb_us = ttfb_start.elapsed().as_micros() as u64;
    global.record_ttfb(ttfb_us);
    proto.record_ttfb(ttfb_us);

    let status = response.status().as_u16();
    let mut recv_body = response.into_body();
    let mut bytes_rx = 0u64;

    while let Some(chunk) = recv_body.data().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!(e))?;
        bytes_rx += chunk.len() as u64;
        let _ = recv_body.flow_control().release_capacity(chunk.len());
    }

    Ok((status, bytes_rx))
}

fn build_path(base: &str, extra_bytes: Option<usize>) -> String {
    match extra_bytes {
        None | Some(0) => base.to_string(),
        Some(n) => format!("{}?x={}", base, "a".repeat(n)),
    }
}

fn to_http_method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
    }
}

#[inline] fn record_attempt(g: &Collector, p: &Collector) {
    g.record_connection_attempt(); p.record_connection_attempt();
}
#[inline] fn record_established(g: &Collector, p: &Collector) {
    g.record_connection_established(); p.record_connection_established();
}
#[inline] fn record_failed(g: &Collector, p: &Collector) {
    g.record_connection_failed(); p.record_connection_failed();
}
#[inline] fn record_timeout(g: &Collector, p: &Collector) {
    g.record_timeout(); p.record_timeout();
}
#[inline] fn record_closed(g: &Collector, p: &Collector) {
    g.record_connection_closed(); p.record_connection_closed();
}
#[inline] fn record_response(g: &Collector, p: &Collector, status: u16, bytes: u64, us: u64) {
    g.record_response(status, bytes, us); p.record_response(status, bytes, us);
}

async fn wait_deadline(deadline: Option<Instant>) {
    if let Some(dl) = deadline {
        tokio::time::sleep(dl.saturating_duration_since(Instant::now())).await;
    } else {
        std::future::pending::<()>().await;
    }
}
