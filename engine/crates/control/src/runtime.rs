use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use net_meter_core::NetworkMode;
use serde::{Deserialize, Serialize};

use crate::state::ServerNetConfig;

#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub web_dir: Option<PathBuf>,
    pub server_net: ServerNetConfig,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9090,
            log_level: "info".to_string(),
            web_dir: None,
            server_net: ServerNetConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RuntimeConfigFile {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub web_dir: Option<PathBuf>,
    #[serde(default)]
    pub network: RuntimeNetworkFile,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RuntimeNetworkFile {
    #[serde(default)]
    pub mode: Option<NetworkMode>,
    #[serde(default)]
    pub upper_iface: Option<String>,
    #[serde(default)]
    pub lower_iface: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub ns_prefix: Option<String>,
}

#[derive(Debug, Parser)]
#[command(name = "net-meter", about = "Network performance measurement tool")]
pub struct Cli {
    /// 런타임 설정 YAML 파일 경로
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Control API 서버 바인드 주소
    #[arg(long)]
    pub host: Option<String>,

    /// Control API 서버 포트
    #[arg(long, short)]
    pub port: Option<u16>,

    /// 로그 레벨 (error, warn, info, debug, trace)
    #[arg(long)]
    pub log_level: Option<String>,

    /// 프론트엔드 정적 파일 디렉터리 (빌드 산출물 경로)
    #[arg(long)]
    pub web_dir: Option<PathBuf>,

    /// 네트워크 모드 (loopback, namespace, external_port)
    #[arg(long)]
    pub mode: Option<String>,

    /// Client 측 인터페이스 이름 (NS 모드: host veth, External Port 모드: 물리 NIC)
    #[arg(long)]
    pub upper_iface: Option<String>,

    /// Server 측 인터페이스 이름 (NS 모드: host veth, External Port 모드: 물리 NIC)
    #[arg(long)]
    pub lower_iface: Option<String>,

    /// MTU (External Port 모드에서 사용)
    #[arg(long)]
    pub mtu: Option<u16>,

    /// Namespace prefix (NS 모드에서 사용)
    #[arg(long)]
    pub ns_prefix: Option<String>,
}

impl RuntimeSettings {
    pub fn load(cli: &Cli) -> Result<Self> {
        let mut settings = Self::default();

        if let Some(path) = cli.config.as_deref() {
            let file = load_runtime_file(path)?;
            settings.apply_file(file);
        }

        settings.apply_cli(cli)?;
        Ok(settings)
    }

    fn apply_file(&mut self, file: RuntimeConfigFile) {
        if let Some(host) = file.host {
            self.host = host;
        }
        if let Some(port) = file.port {
            self.port = port;
        }
        if let Some(log_level) = file.log_level {
            self.log_level = log_level;
        }
        if let Some(web_dir) = file.web_dir {
            self.web_dir = Some(web_dir);
        }
        if let Some(mode) = file.network.mode {
            self.server_net.mode = mode;
        }
        if let Some(upper_iface) = file.network.upper_iface {
            self.server_net.upper_iface = upper_iface;
        }
        if let Some(lower_iface) = file.network.lower_iface {
            self.server_net.lower_iface = lower_iface;
        }
        if let Some(mtu) = file.network.mtu {
            self.server_net.mtu = mtu;
        }
        if let Some(ns_prefix) = file.network.ns_prefix {
            self.server_net.ns_prefix = ns_prefix;
        }
    }

    fn apply_cli(&mut self, cli: &Cli) -> Result<()> {
        if let Some(host) = &cli.host {
            self.host = host.clone();
        }
        if let Some(port) = cli.port {
            self.port = port;
        }
        if let Some(log_level) = &cli.log_level {
            self.log_level = log_level.clone();
        }
        if let Some(web_dir) = &cli.web_dir {
            self.web_dir = Some(web_dir.clone());
        }
        if let Some(mode) = &cli.mode {
            self.server_net.mode = parse_network_mode(mode)?;
        }
        if let Some(upper_iface) = &cli.upper_iface {
            self.server_net.upper_iface = upper_iface.clone();
        }
        if let Some(lower_iface) = &cli.lower_iface {
            self.server_net.lower_iface = lower_iface.clone();
        }
        if let Some(mtu) = cli.mtu {
            self.server_net.mtu = mtu;
        }
        if let Some(ns_prefix) = &cli.ns_prefix {
            self.server_net.ns_prefix = ns_prefix.clone();
        }
        Ok(())
    }
}

fn parse_network_mode(value: &str) -> Result<NetworkMode> {
    match value {
        "loopback" => Ok(NetworkMode::Loopback),
        "namespace" => Ok(NetworkMode::Namespace),
        "external_port" => Ok(NetworkMode::ExternalPort),
        other => anyhow::bail!(
            "invalid network mode '{}'; expected one of: loopback, namespace, external_port",
            other
        ),
    }
}

fn load_runtime_file(path: &Path) -> Result<RuntimeConfigFile> {
    let body = fs::read_to_string(path)
        .with_context(|| format!("failed to read runtime config {}", path.display()))?;
    serde_yaml::from_str(&body)
        .with_context(|| format!("failed to parse runtime config {}", path.display()))
}
