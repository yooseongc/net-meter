use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{HttpPayload, LoadConfig, TestType};
use net_meter_metrics::{ActiveConnectionGuard, Collector};
use rustls::ClientConfig;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tracing::debug;

// boxed reader/writer — TLS와 평문 스트림을 통합 처리
type DynReader = Box<dyn tokio::io::AsyncRead + Unpin + Send>;
type DynWriter = Box<dyn tokio::io::AsyncWrite + Unpin + Send>;

/// HTTP/1.1 트래픽 발생 진입점
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
    match test_type {
        TestType::Cps => run_cps(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip).await,
        TestType::Cc => run_cc(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip).await,
        TestType::Bw => run_bw(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip).await,
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
// CPS: rate limiter 없이 connect→transact→close를 최대 속도로 반복
// num_conn개의 병렬 루프를 실행 (기본 1 = 순차)
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
    let method = payload.method.as_str().to_string();
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    if num_conn <= 1 {
        // 단일 순차 루프
        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) {
                break;
            }
            let cycle = single_request(
                &addr, &method, &path, req_body,
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
        // 병렬 루프
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut handles = Vec::with_capacity(num_conn);
        for _ in 0..num_conn {
            let running = Arc::clone(&running);
            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            let a = addr.clone();
            let me = method.clone();
            let pa = path.clone();
            let tls = tls.clone();
            handles.push(tokio::spawn(async move {
                loop {
                    if !running.load(std::sync::atomic::Ordering::Relaxed) { break; }
                    if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                    single_request(&a, &me, &pa, req_body, Arc::clone(&g), Arc::clone(&p), connect_to, response_to, tls.clone(), src_ip).await;
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
// CC: num_conn개의 keep-alive 세션 유지. 1초 간격 경량 요청. 연결 수 측정 집중.
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
    let method = payload.method.as_str().to_string();
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let me = method.clone();
        let pa = path.clone();
        let tls = tls.clone();
        handles.push(tokio::spawn(async move {
            // CC: body=0 (헤더만), 1초 간격 — 연결 유지에 집중
            cc_keep_alive_session(&a, &me, &pa, g, p, connect_to, response_to, deadline, tls, src_ip).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// BW: num_conn개의 keep-alive 세션을 동시에 유지. 최대 처리량.
// ---------------------------------------------------------------------------

async fn run_bw(
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
    let method = payload.method.as_str().to_string();
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let me = method.clone();
        let pa = path.clone();
        let tls = tls.clone();
        handles.push(tokio::spawn(async move {
            keep_alive_session(&a, &me, &pa, req_body, g, p, connect_to, response_to, deadline, tls, src_ip).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// 단일 요청 (CPS, Connection: close)
// ---------------------------------------------------------------------------

async fn single_request(
    addr: &str,
    method: &str,
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
    let tcp = match timeout(connect_timeout, connect_tcp(addr, src_ip)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            debug!(addr, error = %e, "HTTP/1.1 connect failed");
            record_failed(&global, &proto);
            return;
        }
        Err(_) => {
            debug!(addr, "HTTP/1.1 connect timed out");
            record_failed(&global, &proto);
            record_timeout(&global, &proto);
            return;
        }
    };

    // Optional TLS handshake
    let result = if let Some(cfg) = tls {
        let connector = TlsConnector::from(cfg);
        let sn = rustls::pki_types::ServerName::try_from("localhost")
            .unwrap()
            .to_owned();
        match connector.connect(sn, tcp).await {
            Ok(tls_stream) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                let r = timeout(
                    response_timeout,
                    send_and_receive(tls_stream, addr, method, path, req_body_bytes, &global, &proto),
                )
                .await;
                drop(_guard);
                r
            }
            Err(e) => {
                debug!(addr, error = %e, "TLS handshake failed");
                record_failed(&global, &proto);
                return;
            }
        }
    } else {
        let us = connect_start.elapsed().as_micros() as u64;
        record_established(&global, &proto);
        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
        global.record_connect_latency(us);
        proto.record_connect_latency(us);
        let r = timeout(
            response_timeout,
            send_and_receive(tcp, addr, method, path, req_body_bytes, &global, &proto),
        )
        .await;
        drop(_guard);
        r
    };

    match result {
        Ok(Ok((status, bytes_rx))) => {
            let total_us = total_start.elapsed().as_micros() as u64;
            record_response(&global, &proto, status, bytes_rx, total_us);
        }
        Ok(Err(e)) => debug!(addr, error = %e, "HTTP/1.1 IO error"),
        Err(_) => {
            debug!(addr, "HTTP/1.1 response timed out");
            record_timeout(&global, &proto);
        }
    }
}

async fn send_and_receive<S>(
    mut stream: S,
    host: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    global: &Arc<Collector>,
    proto: &Arc<Collector>,
) -> std::io::Result<(u16, u64)>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let header = if req_body_bytes > 0 {
        format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: net-meter/0.1\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
            method, path, host, req_body_bytes
        )
    } else {
        format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: net-meter/0.1\r\n\r\n",
            method, path, host
        )
    };
    stream.write_all(header.as_bytes()).await?;

    if req_body_bytes > 0 {
        let body = vec![0u8; req_body_bytes];
        stream.write_all(&body).await?;
    }

    let tx = (header.len() + req_body_bytes) as u64;
    global.record_request(tx);
    proto.record_request(tx);

    let ttfb_start = Instant::now();
    let mut buf = vec![0u8; 8192];
    let mut total_rx: u64 = 0;
    let mut status_code: u16 = 0;
    let mut first_byte = true;

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 { break; }

        if first_byte {
            let ttfb_us = ttfb_start.elapsed().as_micros() as u64;
            global.record_ttfb(ttfb_us);
            proto.record_ttfb(ttfb_us);
            if let Ok(s) = std::str::from_utf8(&buf[..n.min(32)]) {
                if s.starts_with("HTTP/") {
                    status_code = s
                        .split_whitespace()
                        .nth(1)
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(0);
                }
            }
            first_byte = false;
        }
        total_rx += n as u64;
    }

    Ok((status_code, total_rx))
}

// ---------------------------------------------------------------------------
// CC 워커: keep-alive 연결을 유지하며 1초 간격 경량 GET 요청
// ---------------------------------------------------------------------------

async fn cc_keep_alive_session(
    addr: &str,
    method: &str,
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
        let tcp = match timeout(connect_timeout, connect_tcp(addr, src_ip)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                debug!(addr, error = %e, "HTTP/1.1 CC connect failed");
                record_failed(&global, &proto);
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            Err(_) => {
                debug!(addr, "HTTP/1.1 CC connect timed out");
                record_failed(&global, &proto);
                record_timeout(&global, &proto);
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        let (mut reader, mut writer) = if let Some(cfg) = &tls {
            let connector = TlsConnector::from(Arc::clone(cfg));
            let sn = rustls::pki_types::ServerName::try_from("localhost").unwrap().to_owned();
            match connector.connect(sn, tcp).await {
                Ok(tls_stream) => {
                    let us = connect_start.elapsed().as_micros() as u64;
                    record_established(&global, &proto);
                    global.record_connect_latency(us);
                    proto.record_connect_latency(us);
                    let (r, w) = tokio::io::split(tls_stream);
                    (Box::new(r) as DynReader, Box::new(w) as DynWriter)
                }
                Err(e) => {
                    debug!(addr, error = %e, "TLS handshake failed");
                    record_failed(&global, &proto);
                    continue;
                }
            }
        } else {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            let (r, w) = tokio::io::split(tcp);
            (Box::new(r) as DynReader, Box::new(w) as DynWriter)
        };

        // 가드: abort/break 어느 경로로든 active_connections 감소 보장
        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
        let mut buffered = BufReader::new(&mut reader);

        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

            let total_start = Instant::now();
            let result = timeout(
                response_timeout,
                do_keepalive_request(&mut buffered, &mut writer, addr, method, path, 0, &global, &proto),
            ).await;

            match result {
                Ok(Ok((status, bytes_rx, reuse))) => {
                    let total_us = total_start.elapsed().as_micros() as u64;
                    record_response(&global, &proto, status, bytes_rx, total_us);
                    if !reuse { break; }
                }
                Ok(Err(e)) => {
                    debug!(addr, error = %e, "HTTP/1.1 CC IO error");
                    break;
                }
                Err(_) => {
                    debug!(addr, "HTTP/1.1 CC response timed out");
                    record_timeout(&global, &proto);
                    break;
                }
            }

            // CC: 1초 간격 — 연결 유지가 목적, 처리량은 최소화
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        // _guard drop here → record_connection_closed()
    }
}

// ---------------------------------------------------------------------------
// BW 워커: TCP 연결을 재사용하는 keep-alive 세션 (최대 속도)
// ---------------------------------------------------------------------------

async fn keep_alive_session(
    addr: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
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
        let tcp = match timeout(connect_timeout, connect_tcp(addr, src_ip)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                debug!(addr, error = %e, "HTTP/1.1 connect failed");
                record_failed(&global, &proto);
                continue;
            }
            Err(_) => {
                debug!(addr, "HTTP/1.1 connect timed out");
                record_failed(&global, &proto);
                record_timeout(&global, &proto);
                continue;
            }
        };

        // Optional TLS + split into (DynReader, DynWriter)
        let (mut reader, mut writer) = if let Some(cfg) = &tls {
            let connector = TlsConnector::from(Arc::clone(cfg));
            let sn = rustls::pki_types::ServerName::try_from("localhost")
                .unwrap()
                .to_owned();
            match connector.connect(sn, tcp).await {
                Ok(tls_stream) => {
                    let us = connect_start.elapsed().as_micros() as u64;
                    record_established(&global, &proto);
                    global.record_connect_latency(us);
                    proto.record_connect_latency(us);
                    let (r, w) = tokio::io::split(tls_stream);
                    (Box::new(r) as DynReader, Box::new(w) as DynWriter)
                }
                Err(e) => {
                    debug!(addr, error = %e, "TLS handshake failed");
                    record_failed(&global, &proto);
                    continue;
                }
            }
        } else {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            let (r, w) = tokio::io::split(tcp);
            (Box::new(r) as DynReader, Box::new(w) as DynWriter)
        };

        // 가드: abort/break 어느 경로로든 active_connections 감소 보장
        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
        let mut buffered = BufReader::new(&mut reader);

        // 동일 TCP/TLS 연결에서 keep-alive 반복 요청
        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

            let total_start = Instant::now();
            let result = timeout(
                response_timeout,
                do_keepalive_request(&mut buffered, &mut writer, addr, method, path, req_body_bytes, &global, &proto),
            ).await;

            match result {
                Ok(Ok((status, bytes_rx, reuse))) => {
                    let total_us = total_start.elapsed().as_micros() as u64;
                    record_response(&global, &proto, status, bytes_rx, total_us);
                    if !reuse { break; }
                }
                Ok(Err(e)) => {
                    debug!(addr, error = %e, "HTTP/1.1 keep-alive IO error");
                    break;
                }
                Err(_) => {
                    debug!(addr, "HTTP/1.1 keep-alive response timed out");
                    record_timeout(&global, &proto);
                    break;
                }
            }
        }
        // _guard drop here → record_connection_closed()
    }
}

/// 기존 연결에서 HTTP/1.1 요청 1회 수행.
/// 반환: (status_code, rx_bytes, 연결_재사용_가능)
async fn do_keepalive_request(
    reader: &mut BufReader<&mut DynReader>,
    writer: &mut DynWriter,
    host: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    global: &Arc<Collector>,
    proto: &Arc<Collector>,
) -> std::io::Result<(u16, u64, bool)> {
    let header = if req_body_bytes > 0 {
        format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: keep-alive\r\nUser-Agent: net-meter/0.1\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
            method, path, host, req_body_bytes
        )
    } else {
        format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: keep-alive\r\nUser-Agent: net-meter/0.1\r\n\r\n",
            method, path, host
        )
    };
    writer.write_all(header.as_bytes()).await?;
    if req_body_bytes > 0 {
        let body = vec![0u8; req_body_bytes];
        writer.write_all(&body).await?;
    }
    let tx = (header.len() + req_body_bytes) as u64;
    global.record_request(tx);
    proto.record_request(tx);

    let ttfb_start = Instant::now();
    let mut status_code: u16 = 0;
    let mut content_length: Option<usize> = None;
    let mut server_keep_alive = true;
    let mut first_line = true;
    let mut total_rx: u64 = 0;

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "connection closed"));
        }
        total_rx += n as u64;

        if first_line {
            let ttfb_us = ttfb_start.elapsed().as_micros() as u64;
            global.record_ttfb(ttfb_us);
            proto.record_ttfb(ttfb_us);
            status_code = line.split_whitespace()
                .nth(1)
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            first_line = false;
        }

        let trimmed = line.trim_end();
        if trimmed.is_empty() { break; }

        let lower = trimmed.to_lowercase();
        if lower.starts_with("content-length:") {
            content_length = trimmed[15..].trim().parse().ok();
        } else if lower.starts_with("connection:") {
            server_keep_alive = trimmed[11..].trim().to_lowercase() != "close";
        }
    }

    let reuse = if let Some(len) = content_length {
        if len > 0 {
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).await?;
            total_rx += len as u64;
        }
        server_keep_alive
    } else {
        false
    };

    Ok((status_code, total_rx, reuse))
}

// ---------------------------------------------------------------------------
// 헬퍼
// ---------------------------------------------------------------------------

fn build_path(base: &str, extra_bytes: Option<usize>) -> String {
    match extra_bytes {
        None | Some(0) => base.to_string(),
        Some(n) => format!("{}?x={}", base, "a".repeat(n)),
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
