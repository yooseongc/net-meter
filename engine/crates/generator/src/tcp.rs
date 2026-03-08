use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{LoadConfig, TcpPayload, TestType};
use net_meter_metrics::Collector;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::debug;

/// TCP 트래픽 발생 진입점
pub async fn run(
    test_type: TestType,
    addr: &str,
    load: &LoadConfig,
    payload: &TcpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    src_ip: Option<IpAddr>,
) {
    let num_conn = load.effective_num_connections() as usize;
    match test_type {
        TestType::Cps => run_cps(addr, load, payload, num_conn, global, proto, shutdown, deadline, src_ip).await,
        TestType::Cc | TestType::Bw => run_cc_bw(addr, load, payload, num_conn, global, proto, shutdown, deadline, src_ip).await,
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
// CPS — rate limiter 없이 ping-pong 루프를 최대 속도로 반복
// ---------------------------------------------------------------------------

async fn run_cps(
    addr: &str,
    load: &LoadConfig,
    payload: &TcpPayload,
    num_conn: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    src_ip: Option<IpAddr>,
) {
    let connect_to = load.connect_timeout();
    let response_to = load.response_timeout();
    let tx_bytes = payload.tx_bytes;
    let rx_bytes = payload.rx_bytes;
    let addr = addr.to_string();

    if num_conn <= 1 {
        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
            let cycle = tcp_pingpong(
                &addr, tx_bytes, rx_bytes,
                Arc::clone(&global), Arc::clone(&proto),
                connect_to, response_to, src_ip,
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
            handles.push(tokio::spawn(async move {
                loop {
                    if !running.load(std::sync::atomic::Ordering::Relaxed) { break; }
                    if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                    tcp_pingpong(&a, tx_bytes, rx_bytes, Arc::clone(&g), Arc::clone(&p), connect_to, response_to, src_ip).await;
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
// CC/BW — num_conn개의 스트리밍 워커 동시 유지
// ---------------------------------------------------------------------------

async fn run_cc_bw(
    addr: &str,
    load: &LoadConfig,
    payload: &TcpPayload,
    num_conn: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
    src_ip: Option<IpAddr>,
) {
    let connect_to = load.connect_timeout();
    let tx_bytes = payload.tx_bytes;
    let rx_bytes = payload.rx_bytes;
    let addr = addr.to_string();

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        handles.push(tokio::spawn(async move {
            tcp_stream_worker(&a, tx_bytes, rx_bytes, g, p, connect_to, deadline, src_ip).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// 단일 ping-pong (CPS): connect → send tx_bytes → recv rx_bytes → close
// ---------------------------------------------------------------------------

async fn tcp_pingpong(
    addr: &str,
    tx_bytes: usize,
    rx_bytes: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
    src_ip: Option<IpAddr>,
) {
    let total_start = Instant::now();
    record_attempt(&global, &proto);

    let connect_start = Instant::now();
    let stream = match timeout(connect_timeout, connect_tcp(addr, src_ip)).await {
        Ok(Ok(s)) => {
            let us = connect_start.elapsed().as_micros() as u64;
            record_established(&global, &proto);
            global.record_connect_latency(us);
            proto.record_connect_latency(us);
            s
        }
        Ok(Err(e)) => {
            debug!(addr, error = %e, "TCP connect failed");
            record_failed(&global, &proto);
            return;
        }
        Err(_) => {
            debug!(addr, "TCP connect timed out");
            record_failed(&global, &proto);
            record_timeout(&global, &proto);
            return;
        }
    };

    let result = timeout(
        response_timeout,
        do_pingpong(stream, tx_bytes, rx_bytes, &global, &proto),
    )
    .await;

    match result {
        Ok(Ok(bytes_rx)) => {
            let total_us = total_start.elapsed().as_micros() as u64;
            record_response(&global, &proto, 0, bytes_rx, total_us);
        }
        Ok(Err(e)) => debug!(addr, error = %e, "TCP IO error"),
        Err(_) => {
            debug!(addr, "TCP response timed out");
            record_timeout(&global, &proto);
        }
    }
    record_closed(&global, &proto);
}

async fn do_pingpong(
    mut stream: TcpStream,
    tx_bytes: usize,
    rx_bytes: usize,
    global: &Arc<Collector>,
    proto: &Arc<Collector>,
) -> std::io::Result<u64> {
    if tx_bytes > 0 {
        let buf = vec![0u8; tx_bytes];
        stream.write_all(&buf).await?;
        let tx = tx_bytes as u64;
        global.record_request(tx);
        proto.record_request(tx);
    }

    let ttfb_start = Instant::now();
    let mut total_rx = 0u64;
    let mut buf = vec![0u8; 65536];
    let mut first = true;

    while total_rx < rx_bytes as u64 {
        let n = stream.read(&mut buf).await?;
        if n == 0 { break; }
        if first {
            let ttfb_us = ttfb_start.elapsed().as_micros() as u64;
            global.record_ttfb(ttfb_us);
            proto.record_ttfb(ttfb_us);
            first = false;
        }
        total_rx += n as u64;
    }

    Ok(total_rx)
}

// ---------------------------------------------------------------------------
// 스트리밍 워커 (CC/BW): connect → loop { send + recv } → deadline
// ---------------------------------------------------------------------------

async fn tcp_stream_worker(
    addr: &str,
    tx_bytes: usize,
    rx_bytes: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
    connect_timeout: Duration,
    deadline: Option<Instant>,
    src_ip: Option<IpAddr>,
) {
    loop {
        if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

        record_attempt(&global, &proto);
        let connect_start = Instant::now();
        let mut stream = match timeout(connect_timeout, connect_tcp(addr, src_ip)).await {
            Ok(Ok(s)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                s
            }
            Ok(Err(e)) => {
                debug!(error = %e, "TCP stream connect failed, retrying");
                record_failed(&global, &proto);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(_) => {
                debug!("TCP stream connect timed out, retrying");
                record_failed(&global, &proto);
                record_timeout(&global, &proto);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        let chunk = if tx_bytes > 0 { vec![0u8; tx_bytes] } else { vec![] };
        let mut rx_buf = if rx_bytes > 0 { vec![0u8; rx_bytes.max(65536)] } else { vec![] };

        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

            let total_start = Instant::now();

            if !chunk.is_empty() {
                if stream.write_all(&chunk).await.is_err() { break; }
                let tx = chunk.len() as u64;
                global.record_request(tx);
                proto.record_request(tx);
            }

            if !rx_buf.is_empty() {
                let mut received = 0usize;
                let first_byte_start = Instant::now();
                let mut first = true;

                while received < rx_bytes {
                    match stream.read(&mut rx_buf[received..]).await {
                        Ok(0) => { break; }
                        Ok(n) => {
                            if first {
                                let ttfb_us = first_byte_start.elapsed().as_micros() as u64;
                                global.record_ttfb(ttfb_us);
                                proto.record_ttfb(ttfb_us);
                                first = false;
                            }
                            received += n;
                        }
                        Err(_) => { break; }
                    }
                }
                let total_us = total_start.elapsed().as_micros() as u64;
                record_response(&global, &proto, 0, received as u64, total_us);
            } else if !chunk.is_empty() {
                let total_us = total_start.elapsed().as_micros() as u64;
                record_response(&global, &proto, 0, 0, total_us);
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        record_closed(&global, &proto);
    }
}

// ---------------------------------------------------------------------------
// 헬퍼
// ---------------------------------------------------------------------------

#[inline]
fn record_attempt(g: &Collector, p: &Collector) {
    g.record_connection_attempt();
    p.record_connection_attempt();
}

#[inline]
fn record_established(g: &Collector, p: &Collector) {
    g.record_connection_established();
    p.record_connection_established();
}

#[inline]
fn record_failed(g: &Collector, p: &Collector) {
    g.record_connection_failed();
    p.record_connection_failed();
}

#[inline]
fn record_timeout(g: &Collector, p: &Collector) {
    g.record_timeout();
    p.record_timeout();
}

#[inline]
fn record_closed(g: &Collector, p: &Collector) {
    g.record_connection_closed();
    p.record_connection_closed();
}

#[inline]
fn record_response(g: &Collector, p: &Collector, status: u16, bytes: u64, latency_us: u64) {
    g.record_response(status, bytes, latency_us);
    p.record_response(status, bytes, latency_us);
}

async fn wait_deadline(deadline: Option<Instant>) {
    if let Some(dl) = deadline {
        let remaining = dl.saturating_duration_since(Instant::now());
        tokio::time::sleep(remaining).await;
    } else {
        std::future::pending::<()>().await;
    }
}
