use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{Method, Request};
use net_meter_core::{HttpMethod, HttpPayload, LoadConfig, TestType};
use net_meter_metrics::Collector;
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Semaphore};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::debug;

/// HTTP/2 h2c 트래픽 발생 진입점
pub async fn run(
    test_type: TestType,
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    match test_type {
        TestType::Cps => run_cps(addr, load, payload, global, proto, shutdown, deadline).await,
        TestType::Cc => run_cc(addr, load, payload, global, proto, shutdown, deadline).await,
        TestType::Bw => run_bw(addr, load, payload, global, proto, shutdown, deadline).await,
    }
}

// ---------------------------------------------------------------------------
// CPS: 초당 신규 h2c 연결
// ---------------------------------------------------------------------------

async fn run_cps(
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    let target_cps = load.effective_cps();
    let max_inflight = load.effective_max_inflight() as usize;
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();

    let ramp_up_secs = load.ramp_up_secs;
    let sem = Arc::new(Semaphore::new(max_inflight));
    let tick_interval = Duration::from_secs_f64(1.0 / target_cps as f64);
    let mut ticker = interval(tick_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let ramp_start = Instant::now();
    let mut token_acc: f64 = if ramp_up_secs == 0 { 1.0 } else { 0.0 };

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            _ = ticker.tick() => {
                if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

                if ramp_up_secs > 0 {
                    let scale = (ramp_start.elapsed().as_secs_f64() / ramp_up_secs as f64).min(1.0);
                    token_acc = (token_acc + scale).min(1.0);
                    if token_acc < 1.0 { continue; }
                    token_acc -= 1.0;
                }

                let permit = match Arc::clone(&sem).try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => { debug!("h2 backpressure: semaphore full"); continue; }
                };

                let g = Arc::clone(&global);
                let p = Arc::clone(&proto);
                let a = addr.clone();
                let h = host.clone();
                let me = method.clone();
                let pa = path.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    single_request_h2c(&a, &h, me, &pa, req_body, g, p, connect_to, response_to).await;
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CC: 동시 h2c 연결 유지
// ---------------------------------------------------------------------------

async fn run_cc(
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    let target_cc = load.effective_cc() as usize;
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(target_cc);
    for _ in 0..target_cc {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let h = host.clone();
        let me = method.clone();
        let pa = path.clone();
        handles.push(tokio::spawn(async move {
            connection_worker(&a, &h, me, &pa, req_body, g, p, connect_to, response_to, deadline, 1).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// BW: 다중 연결 × 다중 스트림
// ---------------------------------------------------------------------------

async fn run_bw(
    addr: &str,
    load: &LoadConfig,
    payload: &HttpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    let concurrency = load.effective_cc() as usize;
    let streams_per_conn = payload.h2_max_concurrent_streams.unwrap_or(10) as usize;
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let host = addr.to_string();
    let method = to_http_method(payload.method);
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let h = host.clone();
        let me = method.clone();
        let pa = path.clone();
        handles.push(tokio::spawn(async move {
            connection_worker(&a, &h, me, &pa, req_body, g, p, connect_to, response_to, deadline, streams_per_conn).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// 단일 h2c 요청 (CPS)
// ---------------------------------------------------------------------------

async fn single_request_h2c(
    addr: &str,
    host: &str,
    method: Method,
    path: &str,
    req_body_bytes: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
) {
    let total_start = Instant::now();
    record_attempt(&global, &proto);

    let connect_start = Instant::now();
    let send_req = match timeout(connect_timeout, connect_h2c(addr)).await {
        Ok(Ok(sr)) => {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            sr
        }
        Ok(Err(e)) => {
            debug!(addr, error = %e, "h2c handshake failed");
            record_failed(&global, &proto);
            return;
        }
        Err(_) => {
            debug!(addr, "h2c connect timed out");
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
// h2c 연결 워커 (CC/BW)
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
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

        record_attempt(&global, &proto);
        let connect_start = Instant::now();
        let send_req = match timeout(connect_timeout, connect_h2c(addr)).await {
            Ok(Ok(sr)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                sr
            }
            Ok(Err(e)) => {
                debug!(error = %e, "h2c connect failed, retrying");
                record_failed(&global, &proto);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(_) => {
                debug!("h2c connect timed out, retrying");
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

async fn connect_h2c(addr: &str) -> anyhow::Result<h2::client::SendRequest<Bytes>> {
    let stream = TcpStream::connect(addr).await?;
    let (send_req, connection) = h2::client::Builder::new()
        .initial_window_size(1 << 20)
        .handshake::<_, Bytes>(stream)
        .await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            debug!("h2 connection closed: {}", e);
        }
    });
    Ok(send_req)
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
