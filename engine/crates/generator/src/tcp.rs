use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{LoadConfig, TcpPayload, TestType};
use net_meter_metrics::{ActiveConnectionGuard, Collector};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::debug;

use crate::common::{
    self, connect_tcp, record_attempt, record_established, record_failed, record_response,
    record_timeout, wait_deadline,
};

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
        TestType::Cc => run_cc(addr, load, payload, num_conn, global, proto, shutdown, deadline, src_ip).await,
        TestType::Bw => run_bw(addr, load, payload, num_conn, global, proto, shutdown, deadline, src_ip).await,
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
        // 병렬 루프 — CC/BW와 동일하게 abort()로 중단
        let mut handles = Vec::with_capacity(num_conn);
        for _ in 0..num_conn {
            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            let a = addr.clone();
            handles.push(tokio::spawn(async move {
                loop {
                    if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                    tcp_pingpong(&a, tx_bytes, rx_bytes, Arc::clone(&g), Arc::clone(&p), connect_to, response_to, src_ip).await;
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
// CC — num_conn개의 연결을 유지. 데이터 교환 최소화, 연결 수 측정 집중.
// ---------------------------------------------------------------------------

async fn run_cc(
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
    let addr = addr.to_string();
    let tx_bytes = payload.tx_bytes;
    let rx_bytes = payload.rx_bytes;

    let mut handles = Vec::with_capacity(num_conn);
    for _ in 0..num_conn {
        let g = Arc::clone(&global);
        let p = Arc::clone(&proto);
        let a = addr.clone();
        handles.push(tokio::spawn(async move {
            tcp_cc_worker(&a, tx_bytes, rx_bytes, g, p, connect_to, deadline, src_ip).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }
    for h in handles { h.abort(); }
}

// ---------------------------------------------------------------------------
// BW — num_conn개의 스트리밍 워커 동시 유지 (최대 처리량)
// ---------------------------------------------------------------------------

async fn run_bw(
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
            tcp_bw_worker(&a, tx_bytes, rx_bytes, g, p, connect_to, deadline, src_ip).await;
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

    let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
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
}

async fn do_pingpong(
    mut stream: tokio::net::TcpStream,
    tx_bytes: usize,
    rx_bytes: usize,
    global: &Arc<Collector>,
    proto: &Arc<Collector>,
) -> std::io::Result<u64> {
    // 대용량 송신도 64KiB 청크 단위로 처리 — 단일 Vec 할당 방지
    if tx_bytes > 0 {
        common::write_zeroes(&mut stream, tx_bytes).await?;
        let tx = tx_bytes as u64;
        global.record_request(tx);
        proto.record_request(tx);
    }

    let ttfb_start = Instant::now();
    let mut total_rx = 0u64;
    // 8KiB 고정 읽기 버퍼 — 루프로 rx_bytes 전체 수신
    let mut buf = vec![0u8; 8192];
    let mut first = true;

    while total_rx < rx_bytes as u64 {
        let to_read = ((rx_bytes as u64 - total_rx) as usize).min(buf.len());
        let n = stream.read(&mut buf[..to_read]).await?;
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
// CC 워커: connect → 연결 유지 (데이터 전송 최소화) → deadline
//
// payload=0 이면 순수 idle 연결 유지 (TCP_KEEPALIVE 의존).
// payload>0 이면 연결 유지하며 주기적 소량 교환.
// ---------------------------------------------------------------------------

async fn tcp_cc_worker(
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
        let stream = match timeout(connect_timeout, connect_tcp(addr, src_ip)).await {
            Ok(Ok(s)) => {
                let us = connect_start.elapsed().as_micros() as u64;
                record_established(&global, &proto);
                global.record_connect_latency(us);
                proto.record_connect_latency(us);
                s
            }
            Ok(Err(e)) => {
                debug!(error = %e, "TCP CC connect failed, retrying");
                record_failed(&global, &proto);
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            Err(_) => {
                debug!("TCP CC connect timed out, retrying");
                record_failed(&global, &proto);
                record_timeout(&global, &proto);
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
        // 연결 유지: payload가 없으면 순수 idle, 있으면 1초 간격으로 소량 교환
        if tx_bytes == 0 && rx_bytes == 0 {
            // idle 유지: 100ms 마다 deadline 체크
            let _ = stream;
            loop {
                if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } else {
            // 64KiB 고정 IO 버퍼 — 연결 수명 동안 재사용
            let mut io_buf = vec![0u8; common::SEND_CHUNK_SIZE];
            let mut stream = stream;
            loop {
                if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }
                let total_start = Instant::now();

                // 대용량 송신도 청크 단위 — 단일 Vec 할당 방지
                if tx_bytes > 0 {
                    if common::write_zeroes(&mut stream, tx_bytes).await.is_err() { break; }
                    global.record_request(tx_bytes as u64);
                    proto.record_request(tx_bytes as u64);
                }
                if rx_bytes > 0 {
                    let mut received = 0usize;
                    loop {
                        let to_read = (rx_bytes - received).min(io_buf.len());
                        match stream.read(&mut io_buf[..to_read]).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                received += n;
                                if received >= rx_bytes { break; }
                            }
                        }
                    }
                    let us = total_start.elapsed().as_micros() as u64;
                    record_response(&global, &proto, 0, received as u64, us);
                }

                // CC는 1초 간격 유지 (연결 수 측정이 목적)
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        // _guard drop here
    }
}

// ---------------------------------------------------------------------------
// BW 워커: connect → loop { send + recv } → deadline
// ---------------------------------------------------------------------------

async fn tcp_bw_worker(
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

        let _guard = ActiveConnectionGuard::new(Arc::clone(&global), Arc::clone(&proto));
        // 64KiB 고정 IO 버퍼 — 연결 수명 동안 재사용
        let mut io_buf = vec![0u8; common::SEND_CHUNK_SIZE];

        loop {
            if deadline.map(|d| Instant::now() >= d).unwrap_or(false) { break; }

            let total_start = Instant::now();

            // 대용량 송신도 청크 단위 — 단일 Vec 할당 방지
            if tx_bytes > 0 {
                if stream.write_all(&common::ZERO_CHUNK[..tx_bytes.min(common::SEND_CHUNK_SIZE)]).await.is_err() { break; }
                // tx_bytes > SEND_CHUNK_SIZE인 경우 나머지 청크 전송
                if tx_bytes > common::SEND_CHUNK_SIZE {
                    if common::write_zeroes(&mut stream, tx_bytes - common::SEND_CHUNK_SIZE).await.is_err() { break; }
                }
                let tx = tx_bytes as u64;
                global.record_request(tx);
                proto.record_request(tx);
            }

            if rx_bytes > 0 {
                let mut received = 0usize;
                let first_byte_start = Instant::now();
                let mut first = true;

                while received < rx_bytes {
                    let to_read = (rx_bytes - received).min(io_buf.len());
                    match stream.read(&mut io_buf[..to_read]).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if first {
                                let ttfb_us = first_byte_start.elapsed().as_micros() as u64;
                                global.record_ttfb(ttfb_us);
                                proto.record_ttfb(ttfb_us);
                                first = false;
                            }
                            received += n;
                        }
                        Err(_) => break,
                    }
                }
                let total_us = total_start.elapsed().as_micros() as u64;
                record_response(&global, &proto, 0, received as u64, total_us);
            } else if tx_bytes > 0 {
                let total_us = total_start.elapsed().as_micros() as u64;
                record_response(&global, &proto, 0, 0, total_us);
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        // _guard drop here
    }
}

// connect_tcp, wait_deadline, record_* → crate::common 사용
