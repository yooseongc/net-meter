mod api;
mod event;
mod orchestrator;
mod result;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use net_meter_core::{MetricsSnapshot, TestState, Thresholds};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::time::{interval, Duration};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::event::TestEvent;

#[derive(Parser)]
#[command(name = "net-meter", about = "Network performance measurement tool")]
struct Cli {
    /// Control API 서버 바인드 주소
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Control API 서버 포트
    #[arg(long, short, default_value_t = 9090)]
    port: u16,

    /// 로그 레벨 (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// 프론트엔드 정적 파일 디렉터리 (빌드 산출물 경로).
    /// 지정하지 않으면 바이너리 옆의 `static/` 디렉터리를 자동 탐색한다.
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 로깅 초기화
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(format!("net_meter={}", cli.log_level))
        }))
        .init();

    info!("net-meter control plane starting");

    // 정적 파일 디렉터리 결정:
    //   1. --web-dir 명시 → 그대로 사용
    //   2. 생략 → 바이너리 옆 static/ 탐색
    let web_dir = cli.web_dir.or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("static")))
            .filter(|p| p.is_dir())
    });

    if let Some(ref dir) = web_dir {
        info!(path = %dir.display(), "Serving frontend from");
    } else {
        info!("No web-dir found; only API endpoints are served");
    }

    let state = state::AppState::new();

    // 백그라운드: 1초 간격으로 메트릭 집계, 임계값 체크, 브로드캐스트
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            let mut snapshot = {
                let mut agg = state_clone.aggregator.lock().await;
                agg.tick()
            };

            // 시험 상태 및 임계값 체크
            let test_state = *state_clone.test_state.read().await;
            snapshot.is_ramping_up = test_state == TestState::RampingUp;

            if test_state == TestState::Running || test_state == TestState::RampingUp {
                if let Some(ref config) = *state_clone.active_config.read().await {
                    let violations = check_thresholds(&snapshot, &config.thresholds);
                    if !violations.is_empty() {
                        snapshot.threshold_violations = violations.clone();
                        let _ = state_clone.event_tx.send(TestEvent::ThresholdViolation {
                            violations: violations.clone(),
                        });
                        if config.thresholds.auto_stop_on_fail {
                            let mut orch = state_clone.orchestrator.lock().await;
                            orch.stop(Arc::clone(&state_clone)).await;
                        }
                    }
                }
            }

            *state_clone.latest_snapshot.write().await = snapshot.clone();
            let _ = state_clone.snapshot_tx.send(snapshot);
        }
    });

    let addr = format!("{}:{}", cli.host, cli.port);
    let listener = reuseport_listener(&addr)?;
    info!(addr = %addr, "Control API server listening");

    let app = api::router(Arc::clone(&state), web_dir);
    axum::serve(listener, app).await?;

    Ok(())
}

/// 임계값 위반 항목 목록 반환 (빈 배열이면 정상)
fn check_thresholds(snap: &MetricsSnapshot, t: &Thresholds) -> Vec<String> {
    let mut v = Vec::new();

    if let Some(min) = t.min_cps {
        // 시험 시작 직후 (CPS == 0)는 false positive 방지를 위해 건너뜀
        if snap.cps > 0.0 && snap.cps < min {
            v.push(format!("CPS {:.1} < min {:.1}", snap.cps, min));
        }
    }

    let attempted = snap.connections_attempted;
    if attempted > 0 {
        let err_pct = snap.connections_failed as f64 / attempted as f64 * 100.0;
        if let Some(max_err) = t.max_error_rate_pct {
            if err_pct > max_err {
                v.push(format!("Error rate {:.1}% > max {:.1}%", err_pct, max_err));
            }
        }
    }

    if let Some(max_p99) = t.max_latency_p99_ms {
        if snap.latency_p99_ms > 0.0 && snap.latency_p99_ms > max_p99 {
            v.push(format!(
                "Latency p99 {:.1}ms > {:.1}ms",
                snap.latency_p99_ms, max_p99
            ));
        }
    }

    v
}

/// SO_REUSEADDR + SO_REUSEPORT를 설정한 TcpListener를 반환한다.
///
/// SO_REUSEPORT: 이전 프로세스가 완전히 종료되기 전에도 같은 포트로 바인드 가능.
/// SO_REUSEADDR: TIME_WAIT 상태의 소켓이 남아있어도 즉시 재바인드 가능.
fn reuseport_listener(addr: &str) -> anyhow::Result<TcpListener> {
    let addr: SocketAddr = addr.parse()?;
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    let std_listener: std::net::TcpListener = socket.into();
    Ok(TcpListener::from_std(std_listener)?)
}
