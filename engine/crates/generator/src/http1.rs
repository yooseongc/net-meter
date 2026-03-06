use std::sync::Arc;
use std::time::{Duration, Instant};

use net_meter_core::{TestProfile, TestType};
use net_meter_metrics::Collector;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Semaphore};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::debug;

/// path_extra_bytes만큼 쿼리 파라미터를 덧붙여 URL을 생성한다.
/// 동일한 바이트 수를 보장하기 위해 고정 문자('a')로 패딩한다.
fn build_path(base: &str, extra_bytes: Option<usize>) -> String {
    match extra_bytes {
        None | Some(0) => base.to_string(),
        Some(n) => {
            // "?x=" (3 bytes) + n bytes of padding
            let padding: String = "a".repeat(n);
            format!("{}?x={}", base, padding)
        }
    }
}

/// HTTP/1.1 트래픽 발생 메인 진입점
pub async fn run(
    profile: TestProfile,
    metrics: Arc<Collector>,
    shutdown: oneshot::Receiver<()>,
) {
    let addr = format!("{}:{}", profile.target_host, profile.target_port);
    let deadline = if profile.duration_secs > 0 {
        Some(Instant::now() + Duration::from_secs(profile.duration_secs))
    } else {
        None
    };

    match profile.test_type {
        TestType::Cps => run_cps(addr, profile, metrics, shutdown, deadline).await,
        TestType::Cc => run_cc(addr, profile, metrics, shutdown, deadline).await,
        TestType::Bw => run_bw(addr, profile, metrics, shutdown, deadline).await,
    }
}

// ---------------------------------------------------------------------------
// CPS 시험: tokio interval + 세마포어 backpressure
// ---------------------------------------------------------------------------

async fn run_cps(
    addr: String,
    profile: TestProfile,
    metrics: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    let target_cps = profile.target_cps.unwrap_or(100).max(1);
    let max_inflight = profile
        .max_inflight
        .unwrap_or(target_cps.saturating_mul(2).min(65535)) as usize;

    let connect_to = Duration::from_millis(profile.connect_timeout_ms.unwrap_or(5000));
    let response_to = Duration::from_millis(profile.response_timeout_ms.unwrap_or(30000));

    // 세마포어: in-flight 연결 수 제한 (backpressure)
    let sem = Arc::new(Semaphore::new(max_inflight));

    // tokio interval: 목표 CPS에 맞는 tick 주기.
    // MissedTickBehavior::Skip: 처리가 늦어지면 따라잡지 않고 스킵 → CPS 상한 유지.
    let tick_interval = Duration::from_secs_f64(1.0 / target_cps as f64);
    let mut ticker = interval(tick_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            _ = ticker.tick() => {
                if let Some(dl) = deadline {
                    if Instant::now() >= dl { break; }
                }

                // 세마포어 비차단 획득: 가득 찼으면 이번 tick 스킵
                let permit = match Arc::clone(&sem).try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        debug!("backpressure: semaphore full, skipping tick");
                        continue;
                    }
                };

                let m = Arc::clone(&metrics);
                let a = addr.clone();
                let method = profile.method.as_str().to_string();
                let path = build_path(&profile.path, profile.path_extra_bytes);
                let req_body = profile.request_body_bytes.unwrap_or(0);

                tokio::spawn(async move {
                    let _permit = permit; // drop on task exit → 세마포어 반환
                    single_request(&a, &method, &path, req_body, m, connect_to, response_to).await;
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CC 시험: 목표 동시 연결 수 유지
// ---------------------------------------------------------------------------

async fn run_cc(
    addr: String,
    profile: TestProfile,
    metrics: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    let target_cc = profile.target_cc.unwrap_or(100) as usize;
    let connect_to = Duration::from_millis(profile.connect_timeout_ms.unwrap_or(5000));
    let response_to = Duration::from_millis(profile.response_timeout_ms.unwrap_or(30000));

    // target_cc 개의 워커를 띄워 각자 keep-alive 루프 실행
    let mut handles = Vec::with_capacity(target_cc);
    for _ in 0..target_cc {
        let m = Arc::clone(&metrics);
        let a = addr.clone();
        let method = profile.method.as_str().to_string();
        let path = build_path(&profile.path, profile.path_extra_bytes);
        let req_body = profile.request_body_bytes.unwrap_or(0);

        handles.push(tokio::spawn(async move {
            keep_alive_loop(&a, &method, &path, req_body, m, connect_to, response_to, deadline).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }

    for h in handles {
        h.abort();
    }
}

// ---------------------------------------------------------------------------
// BW 시험: 최대 처리량 (CC와 유사, 동시 연결로 대역폭 포화)
// ---------------------------------------------------------------------------

async fn run_bw(
    addr: String,
    profile: TestProfile,
    metrics: Arc<Collector>,
    mut shutdown: oneshot::Receiver<()>,
    deadline: Option<Instant>,
) {
    let concurrency = profile.target_cc.unwrap_or(50) as usize;
    let connect_to = Duration::from_millis(profile.connect_timeout_ms.unwrap_or(5000));
    let response_to = Duration::from_millis(profile.response_timeout_ms.unwrap_or(30000));

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let m = Arc::clone(&metrics);
        let a = addr.clone();
        let method = profile.method.as_str().to_string();
        let path = build_path(&profile.path, profile.path_extra_bytes);
        let req_body = profile.request_body_bytes.unwrap_or(0);

        handles.push(tokio::spawn(async move {
            keep_alive_loop(&a, &method, &path, req_body, m, connect_to, response_to, deadline).await;
        }));
    }

    tokio::select! {
        _ = &mut shutdown => {}
        _ = wait_deadline(deadline) => {}
    }

    for h in handles {
        h.abort();
    }
}

// ---------------------------------------------------------------------------
// 단일 HTTP/1.1 요청 (Connection: close)
// ---------------------------------------------------------------------------

async fn single_request(
    addr: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    metrics: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
) {
    let total_start = Instant::now();
    metrics.record_connection_attempt();

    // 1. TCP Connect (with timeout)
    let connect_start = Instant::now();
    let stream = match timeout(connect_timeout, TcpStream::connect(addr)).await {
        Ok(Ok(s)) => {
            let connect_us = connect_start.elapsed().as_micros() as u64;
            metrics.record_connection_established();
            metrics.record_connect_latency(connect_us);
            s
        }
        Ok(Err(e)) => {
            debug!(addr, error = %e, "TCP connect failed");
            metrics.record_connection_failed();
            return;
        }
        Err(_) => {
            debug!(addr, "TCP connect timed out");
            metrics.record_connection_failed();
            metrics.record_timeout();
            return;
        }
    };

    // 2. 요청 전송 + 응답 수신 (with timeout)
    let result = timeout(
        response_timeout,
        send_and_receive(stream, addr, method, path, req_body_bytes, &metrics),
    )
    .await;

    match result {
        Ok(Ok((status, bytes_rx))) => {
            let total_us = total_start.elapsed().as_micros() as u64;
            metrics.record_response(status, bytes_rx, total_us);
        }
        Ok(Err(_e)) => {
            // 소켓 오류: 응답 실패로 계산하지 않음 (연결은 성립했었음)
            debug!(addr, "Request/response IO error");
        }
        Err(_) => {
            debug!(addr, "Response timed out");
            metrics.record_timeout();
        }
    }

    metrics.record_connection_closed();
}

/// 요청 전송 → TTFB 계측 → 응답 전체 수신
async fn send_and_receive(
    mut stream: TcpStream,
    host: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    metrics: &Arc<Collector>,
) -> std::io::Result<(u16, u64)> {
    // 요청 헤더 작성
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

    // 요청 body 전송
    if req_body_bytes > 0 {
        let body = vec![0u8; req_body_bytes];
        stream.write_all(&body).await?;
    }

    metrics.record_request((header.len() + req_body_bytes) as u64);

    // 응답 수신
    let ttfb_start = Instant::now();
    let mut buf = vec![0u8; 8192];
    let mut total_rx: u64 = 0;
    let mut status_code: u16 = 0;
    let mut first_byte = true;

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        if first_byte {
            // TTFB: 첫 번째 데이터 수신 시점
            metrics.record_ttfb(ttfb_start.elapsed().as_micros() as u64);
            // 상태 코드 파싱 (HTTP/1.x SSS ...)
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
// 헬퍼
// ---------------------------------------------------------------------------

/// CC/BW 워커: 연결 → 요청 → 응답을 반복
async fn keep_alive_loop(
    addr: &str,
    method: &str,
    path: &str,
    req_body_bytes: usize,
    metrics: Arc<Collector>,
    connect_timeout: Duration,
    response_timeout: Duration,
    deadline: Option<Instant>,
) {
    loop {
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                break;
            }
        }
        single_request(addr, method, path, req_body_bytes, Arc::clone(&metrics), connect_timeout, response_timeout)
            .await;
    }
}

/// deadline까지 대기하는 future (None이면 영원히 대기)
async fn wait_deadline(deadline: Option<Instant>) {
    if let Some(dl) = deadline {
        let remaining = dl.saturating_duration_since(Instant::now());
        tokio::time::sleep(remaining).await;
    } else {
        std::future::pending::<()>().await;
    }
}
