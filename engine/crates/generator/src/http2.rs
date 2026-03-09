use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{Method, Request};
use net_meter_core::{HttpMethod, HttpPayload, LoadConfig, TestType};
use net_meter_metrics::{ActiveConnectionGuard, Collector};
use rustls::ClientConfig;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tracing::debug;

use crate::common::{
    self, build_path, connect_tcp, record_attempt, record_established, record_failed,
    record_response, record_timeout, resolve_tls_sni, wait_deadline,
};

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
    tls_server_name: &str,
) {
    let num_conn = load.effective_num_connections() as usize;
    let streams = payload.h2_max_concurrent_streams.unwrap_or(1) as usize;
    match test_type {
        TestType::Cps => run_cps(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip, tls_server_name).await,
        TestType::Cc => run_cc(addr, load, payload, num_conn, global, proto, shutdown, deadline, tls, src_ip, tls_server_name).await,
        TestType::Bw => run_bw(addr, load, payload, num_conn, streams, global, proto, shutdown, deadline, tls, src_ip, tls_server_name).await,
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
    tls_server_name: &str,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();
    let sni = tls_server_name.to_string();

    if num_conn <= 1 {
        // 단일 순차 루프 — shutdown과 deadline 모두 select에 포함해 즉시 중단
        loop {
            let cycle = single_request_h2(
                &addr, &host, method.clone(), &path, req_body,
                Arc::clone(&global), Arc::clone(&proto),
                connect_to, response_to, tls.clone(), src_ip, &sni,
            );
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                _ = wait_deadline(deadline) => break,
                _ = cycle => {}
            }
        }
    } else {
        // 병렬 루프 — CC/BW와 동일하게 abort()로 중단
        let mut handles = Vec::with_capacity(num_conn);
        for _ in 0..num_conn {
            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            let a = addr.clone();
            let h = host.clone();
            let me = method.clone();
            let pa = path.clone();
            let tls = tls.clone();
            let sni = sni.clone();
            handles.push(tokio::spawn(async move {
                loop {
                    if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                    single_request_h2(&a, &h, me.clone(), &pa, req_body, Arc::clone(&g), Arc::clone(&p), connect_to, response_to, tls.clone(), src_ip, &sni).await;
                }
            }));
        }
        tokio::select! {
            _ = &mut shutdown => {}
            _ = wait_deadline(deadline) => {}
        }
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
    tls_server_name: &str,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let addr = addr.to_string();
    let sni = tls_server_name.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let h = host.clone();
        let me = method.clone();
        let pa = path.clone();
        let tls = tls.clone();
        let sni = sni.clone();
        handles.push(tokio::spawn(async move {
            h2_cc_worker(&a, &h, me, &pa, g, p, connect_to, response_to, deadline, tls, src_ip, &sni).await;
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
    tls_server_name: &str,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();
    let sni = tls_server_name.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let h = host.clone();
        let me = method.clone();
        let pa = path.clone();
        let tls = tls.clone();
        let sni = sni.clone();
        handles.push(tokio::spawn(async move {
            connection_worker(&a, &h, me, &pa, req_body, g, p, connect_to, response_to, deadline, streams_per_conn, tls, src_ip, &sni).await;
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
    tls_server_name: &str,
) {
    let total_start = Instant::now();
    record_attempt(&global, &proto);

    let connect_start = Instant::now();
    let (send_req, conn_handle) = match timeout(connect_timeout, connect_h2(addr, &tls, src_ip, tls_server_name)).await {
        Ok(Ok(pair)) => {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            pair
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

    let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
    let result = timeout(
        response_timeout,
        send_h2_stream(&send_req, host, method, path, req_body_bytes, &global, &proto),
    )
    .await;
    // 요청 완료 후 연결 드라이버 종료
    conn_handle.abort();

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
    tls_server_name: &str,
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

        record_attempt(&global, &proto);
        let connect_start = Instant::now();
        let (send_req, conn_handle) = match timeout(connect_timeout, connect_h2(addr, &tls, src_ip, tls_server_name)).await {
            Ok(Ok(pair)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                pair
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

        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
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
        conn_handle.abort();
        // _guard drop here
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
    tls_server_name: &str,
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

        record_attempt(&global, &proto);
        let connect_start = Instant::now();
        let (send_req, conn_handle) = match timeout(connect_timeout, connect_h2(addr, &tls, src_ip, tls_server_name)).await {
            Ok(Ok(pair)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                pair
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

        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
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
        // 스트림 처리 완료 후 연결 드라이버 종료
        conn_handle.abort();
        // _guard drop here
    }
}

// ---------------------------------------------------------------------------
// 헬퍼
// ---------------------------------------------------------------------------

/// h2c 또는 TLS h2 연결을 수립하고 (SendRequest, 드라이버 JoinHandle)을 반환한다.
///
/// 드라이버 JoinHandle은 호출부에서 반드시 보관해야 한다.
/// SendRequest가 drop될 때 h2 연결이 닫히고 드라이버 태스크도 자연 종료된다.
/// 필요 시 abort()로 즉시 종료할 수 있다.
async fn connect_h2(
    addr: &str,
    tls: &Option<Arc<ClientConfig>>,
    src_ip: Option<IpAddr>,
    tls_server_name: &str,
) -> anyhow::Result<(h2::client::SendRequest<Bytes>, tokio::task::JoinHandle<()>)> {
    let tcp = connect_tcp(addr, src_ip).await?;

    if let Some(cfg) = tls {
        let connector = TlsConnector::from(Arc::clone(cfg));
        let sn = resolve_tls_sni(tls_server_name);
        let tls_stream = connector.connect(sn, tcp).await?;
        let (send_req, conn) = h2::client::Builder::new()
            .initial_window_size(1 << 20)
            .handshake::<_, Bytes>(tls_stream)
            .await?;
        let handle = tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("h2 TLS connection driver finished: {}", e);
            }
        });
        Ok((send_req, handle))
    } else {
        let (send_req, conn) = h2::client::Builder::new()
            .initial_window_size(1 << 20)
            .handshake::<_, Bytes>(tcp)
            .await?;
        let handle = tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("h2c connection driver finished: {}", e);
            }
        });
        Ok((send_req, handle))
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

    // 대용량 body를 64KiB 청크 단위로 전송 — 정적 버퍼를 zero-copy 참조
    let mut tx_bytes: u64 = 64; // 헤더 근사값
    if req_body_bytes > 0 {
        let mut remaining = req_body_bytes;
        while remaining > 0 {
            let n = remaining.min(common::SEND_CHUNK_SIZE);
            let end = n == remaining;
            // from_static: 정적 배열 참조 — 추가 할당 없음
            send_stream
                .send_data(Bytes::from_static(&common::ZERO_CHUNK[..n]), end)
                .map_err(|e| anyhow::anyhow!(e))?;
            remaining -= n;
        }
        tx_bytes += req_body_bytes as u64;
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

fn to_http_method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
    }
}

// connect_tcp, wait_deadline, record_*, build_path, resolve_tls_sni → crate::common 사용
