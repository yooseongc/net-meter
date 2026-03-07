use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{HttpPayload, LoadConfig, TestType};
use net_meter_metrics::Collector;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Semaphore};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::debug;

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
) {
    match test_type {
        TestType::Cps => run_cps(addr, load, payload, global, proto, shutdown, deadline).await,
        TestType::Cc => run_cc(addr, load, payload, global, proto, shutdown, deadline).await,
        TestType::Bw => run_bw(addr, load, payload, global, proto, shutdown, deadline).await,
    }
}

// ---------------------------------------------------------------------------
// CPS: interval + 세마포어 backpressure
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

    let method = payload.method.as_str().to_string();
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let ramp_start = Instant::now();
    // token_acc: ramp-up 중 허용 토큰 누적 (≥ 1이면 연결 1개 허용)
    let mut token_acc: f64 = if ramp_up_secs == 0 { 1.0 } else { 0.0 };

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            _ = ticker.tick() => {
                if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

                // Ramp-up: 경과 시간에 비례해 토큰 누적
                if ramp_up_secs > 0 {
                    let scale = (ramp_start.elapsed().as_secs_f64() / ramp_up_secs as f64).min(1.0);
                    token_acc = (token_acc + scale).min(1.0);
                    if token_acc < 1.0 { continue; }
                    token_acc -= 1.0;
                }

                let permit = match Arc::clone(&sem).try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => { debug!("backpressure: semaphore full"); continue; }
                };

                let g = Arc::clone(&global);
                let p = Arc::clone(&proto);
                let a = addr.clone();
                let me = method.clone();
                let pa = path.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    single_request(&a, &me, &pa, req_body, g, p, connect_to, response_to).await;
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CC: target_cc개 워커, keep-alive 루프
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
    let method = payload.method.as_str().to_string();
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(target_cc);
    for _ in 0..target_cc {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let me = method.clone();
        let pa = path.clone();
        handles.push(tokio::spawn(async move {
            keep_alive_loop(&a, &me, &pa, req_body, g, p, connect_to, response_to, deadline).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// BW: 대역폭 포화 (CC와 동일 구조)
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
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let method = payload.method.as_str().to_string();
    let path = build_path(&payload.path, payload.path_extra_bytes);
    let req_body = payload.request_body_bytes.unwrap_or(0);
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        let me = method.clone();
        let pa = path.clone();
        handles.push(tokio::spawn(async move {
            keep_alive_loop(&a, &me, &pa, req_body, g, p, connect_to, response_to, deadline).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// 단일 요청 (Connection: close)
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
) {
    let total_start = Instant::now();
    record_attempt(&global, &proto);

    let connect_start = Instant::now();
    let stream = match timeout(connect_timeout, TcpStream::connect(addr)).await {
        Ok(Ok(s)) => {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            s
        }
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

    let result = timeout(
        response_timeout,
        send_and_receive(stream, addr, method, path, req_body_bytes, &global, &proto),
    )
    .await;

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
    record_closed(&global, &proto);
}

async fn send_and_receive(
    mut stream: TcpStream,
    host: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    global: &Arc<Collector>,
    proto: &Arc<Collector>,
) -> std::io::Result<(u16, u64)> {
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
// CC/BW 워커
// ---------------------------------------------------------------------------

async fn keep_alive_loop(
    addr: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
    deadline: Option<Instant>,
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
        single_request(addr, method, path, req_body_bytes,
            Arc::clone(&global), Arc::clone(&proto),
            connect_timeout, response_timeout).await;
    }
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
