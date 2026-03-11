#[path = "../result.rs"]
mod result;
#[path = "../schema.rs"]
mod schema;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use net_meter_core::{MetricsSnapshot, TestConfig, TestState};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::result::TestResult;
use crate::schema::TestStatus;

#[derive(Parser)]
#[command(name = "net-meter-cli", about = "net-meter control CLI")]
struct Cli {
    /// Control API base URL
    #[arg(long, default_value = "http://127.0.0.1:9090")]
    url: String,

    /// Print raw JSON responses where applicable
    #[arg(long)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Health,
    Status,
    Start {
        /// TestConfig JSON file path
        #[arg(long, short)]
        file: PathBuf,
    },
    Stop,
    Metrics,
    Results,
    DeleteResult {
        #[arg(long)]
        id: String,
    },
    Events,
    Monitor {
        /// Refresh interval in seconds
        #[arg(long, default_value_t = 1)]
        interval: u64,
        /// Exit automatically when the active test reaches a terminal state
        #[arg(long)]
        watch_until_done: bool,
    },
    Run {
        /// TestConfig JSON file path
        #[arg(long, short)]
        file: PathBuf,
        /// Refresh interval in seconds
        #[arg(long, default_value_t = 1)]
        interval: u64,
    },
}

#[derive(Deserialize, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    match cli.command {
        Command::Health => {
            let health: HealthResponse = get_json(&client, &cli.url, "/api/health").await?;
            if cli.json {
                print_json(&health)?;
            } else {
                println!("status={} version={}", health.status, health.version);
            }
        }
        Command::Status => {
            let status: TestStatus = get_json(&client, &cli.url, "/api/status").await?;
            if cli.json {
                print_json(&status)?;
            } else {
                print_status(&status);
            }
        }
        Command::Start { file } => {
            let config = load_test_config(&file)?;
            let response: serde_json::Value =
                post_json(&client, &cli.url, "/api/test/start", &config).await?;
            if cli.json {
                print_json(&response)?;
            } else {
                println!("started {}", config.name);
            }
        }
        Command::Stop => {
            let response: serde_json::Value = post_empty(&client, &cli.url, "/api/test/stop").await?;
            if cli.json {
                print_json(&response)?;
            } else {
                println!("stop requested");
            }
        }
        Command::Metrics => {
            let metrics: MetricsSnapshot = get_json(&client, &cli.url, "/api/metrics").await?;
            if cli.json {
                print_json(&metrics)?;
            } else {
                print_metrics(&metrics);
            }
        }
        Command::Results => {
            let results: Vec<TestResult> = get_json(&client, &cli.url, "/api/results").await?;
            if cli.json {
                print_json(&results)?;
            } else {
                print_results(&results);
            }
        }
        Command::DeleteResult { id } => {
            delete(&client, &cli.url, &format!("/api/results/{id}")).await?;
            println!("deleted result {}", id);
        }
        Command::Events => {
            stream_events(&client, &cli.url, cli.json).await?;
        }
        Command::Monitor { interval, watch_until_done } => {
            monitor(&client, &cli.url, interval, watch_until_done, cli.json).await?;
        }
        Command::Run { file, interval } => {
            let config = load_test_config(&file)?;
            post_json::<_, serde_json::Value>(&client, &cli.url, "/api/test/start", &config).await?;
            wait_for_start(&client, &cli.url).await?;
            monitor(&client, &cli.url, interval, true, cli.json).await?;
        }
    }

    Ok(())
}

fn load_test_config(path: &PathBuf) -> anyhow::Result<TestConfig> {
    let body = fs::read_to_string(path)?;
    let config: TestConfig = serde_json::from_str(&body)?;
    Ok(config)
}

async fn monitor(
    client: &Client,
    base_url: &str,
    interval_secs: u64,
    watch_until_done: bool,
    json: bool,
) -> anyhow::Result<()> {
    loop {
        let status: TestStatus = get_json(client, base_url, "/api/status").await?;
        let metrics: MetricsSnapshot = get_json(client, base_url, "/api/metrics").await?;

        if json {
            let payload = serde_json::json!({
                "status": status,
                "metrics": metrics,
            });
            print_json(&payload)?;
        } else {
            print!("\x1B[2J\x1B[H");
            print_status_summary(&status, &metrics);
        }

        let terminal = matches!(status.state, TestState::Completed | TestState::Failed | TestState::Idle);
        if watch_until_done && terminal {
            break;
        }

        tokio::time::sleep(Duration::from_secs(interval_secs.max(1))).await;
    }

    Ok(())
}

async fn wait_for_start(client: &Client, base_url: &str) -> anyhow::Result<()> {
    for _ in 0..120 {
        let status: TestStatus = get_json(client, base_url, "/api/status").await?;
        if status.state != TestState::Idle {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("test did not leave idle state within 120 seconds");
}

fn print_status(status: &TestStatus) {
    println!("state={}", status.state);
    println!(
        "runtime mode={} upper_iface={} lower_iface={}",
        network_mode_name(status.runtime.mode),
        status.runtime.upper_iface,
        status.runtime.lower_iface
    );
    if let Some(elapsed) = status.elapsed_secs {
        println!("elapsed_secs={elapsed}");
    }
    if let Some(config) = &status.config {
        println!("config={} test_type={:?} duration_secs={}", config.name, config.test_type, config.duration_secs);
    }
}

fn print_metrics(metrics: &MetricsSnapshot) {
    println!(
        "cps={:.2} rps={:.2} active_connections={} tx_per_sec={:.2} rx_per_sec={:.2}",
        metrics.cps,
        metrics.rps,
        metrics.active_connections,
        metrics.bytes_tx_per_sec,
        metrics.bytes_rx_per_sec
    );
    println!(
        "latency_p50_ms={:.2} latency_p99_ms={:.2} connect_p99_ms={:.2} ttfb_p99_ms={:.2}",
        metrics.latency_p50_ms,
        metrics.latency_p99_ms,
        metrics.connect_p99_ms,
        metrics.ttfb_p99_ms
    );
    println!(
        "connections attempted={} established={} failed={} timed_out={}",
        metrics.connections_attempted,
        metrics.connections_established,
        metrics.connections_failed,
        metrics.connections_timed_out
    );
}

fn print_results(results: &[TestResult]) {
    if results.is_empty() {
        println!("no results");
        return;
    }

    for result in results {
        println!(
            "{} {} type={:?} elapsed={}s cps={:.2} rps={:.2}",
            result.id,
            result.config.name,
            result.config.test_type,
            result.elapsed_secs,
            result.final_snapshot.cps,
            result.final_snapshot.rps
        );
    }
}

fn print_status_summary(status: &TestStatus, metrics: &MetricsSnapshot) {
    println!("net-meter monitor");
    println!();
    println!("state      : {}", status.state);
    println!("mode       : {}", network_mode_name(status.runtime.mode));
    println!(
        "ifaces     : {} / {}",
        status.runtime.upper_iface, status.runtime.lower_iface
    );
    println!(
        "profile     : {}",
        status
            .config
            .as_ref()
            .map(|config| config.name.as_str())
            .unwrap_or("-")
    );
    println!(
        "elapsed     : {}",
        status
            .elapsed_secs
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!();
    println!("cps        : {:.2}", metrics.cps);
    println!("rps        : {:.2}", metrics.rps);
    println!("active     : {}", metrics.active_connections);
    println!("tx/s       : {:.2}", metrics.bytes_tx_per_sec);
    println!("rx/s       : {:.2}", metrics.bytes_rx_per_sec);
    println!("p50/p99    : {:.2} / {:.2} ms", metrics.latency_p50_ms, metrics.latency_p99_ms);
    println!("connect p99: {:.2} ms", metrics.connect_p99_ms);
    println!("ttfb p99   : {:.2} ms", metrics.ttfb_p99_ms);
    println!(
        "errors      : failed={} timeout={} http4xx={} http5xx={}",
        metrics.connections_failed,
        metrics.connections_timed_out,
        metrics.status_4xx,
        metrics.status_5xx
    );
    if !metrics.threshold_violations.is_empty() {
        println!();
        println!("\x1b[31mviolations  : {}\x1b[0m", metrics.threshold_violations.join("; "));
    }
}

fn network_mode_name(mode: net_meter_core::NetworkMode) -> &'static str {
    match mode {
        net_meter_core::NetworkMode::Loopback => "loopback",
        net_meter_core::NetworkMode::Namespace => "namespace",
        net_meter_core::NetworkMode::ExternalPort => "external_port",
    }
}

async fn get_json<T: for<'de> Deserialize<'de>>(client: &Client, base_url: &str, path: &str) -> anyhow::Result<T> {
    let response = client.get(format!("{base_url}{path}")).send().await?;
    let response = response.error_for_status()?;
    Ok(response.json::<T>().await?)
}

async fn post_json<B: serde::Serialize, T: for<'de> Deserialize<'de>>(
    client: &Client,
    base_url: &str,
    path: &str,
    body: &B,
) -> anyhow::Result<T> {
    let response = client
        .post(format!("{base_url}{path}"))
        .json(body)
        .send()
        .await?;
    let response = response.error_for_status()?;
    Ok(response.json::<T>().await?)
}

async fn post_empty<T: for<'de> Deserialize<'de>>(client: &Client, base_url: &str, path: &str) -> anyhow::Result<T> {
    let response = client.post(format!("{base_url}{path}")).send().await?;
    let response = response.error_for_status()?;
    Ok(response.json::<T>().await?)
}

async fn delete(client: &Client, base_url: &str, path: &str) -> anyhow::Result<()> {
    client
        .delete(format!("{base_url}{path}"))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn stream_events(client: &Client, base_url: &str, json: bool) -> anyhow::Result<()> {
    let response = client
        .get(format!("{base_url}/api/events/stream"))
        .send()
        .await?
        .error_for_status()?;

    let mut buffer = String::new();

    let mut response = response;
    while let Some(chunk) = response.chunk().await? {
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let frame = buffer[..pos].to_string();
            buffer.drain(..pos + 2);

            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if json {
                        println!("{data}");
                    } else {
                        print_event_line(data);
                    }
                }
            }
        }
    }

    Ok(())
}

fn print_event_line(data: &str) {
    match serde_json::from_str::<serde_json::Value>(data) {
        Ok(value) => {
            let ty = value.get("type").and_then(|v| v.as_str()).unwrap_or("event");
            println!("[{ty}] {data}");
        }
        Err(_) => println!("{data}"),
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
