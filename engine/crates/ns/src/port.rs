use net_meter_core::NetMeterError;
use tracing::{info, warn};

use crate::veth;

/// External Port 모드에서 할당한 리소스 추적.
pub struct ExternalPortState {
    pub upper_iface: String,
    pub lower_iface: String,
}

/// External Port 모드를 설정한다.
/// 물리 NIC에 Promiscuous 모드와 MTU를 설정한다.
/// IP 할당은 수행하지 않는다.
pub async fn setup_external_port(
    upper_iface: &str,
    lower_iface: &str,
    mtu: u16,
) -> Result<ExternalPortState, NetMeterError> {
    info!(%upper_iface, %lower_iface, mtu, "Setting up external port mode");

    for iface in [upper_iface, lower_iface] {
        if !veth::check_iface(iface).await {
            return Err(NetMeterError::Namespace(format!(
                "Interface '{}' not found. Check the interface name.",
                iface
            )));
        }
        veth::bring_up(iface).await?;
        veth::set_mtu(iface, mtu).await?;
        veth::set_promisc(iface, true).await?;
        info!(%iface, mtu, "Interface configured (up, mtu, promisc on)");
    }

    info!("External port setup complete");
    Ok(ExternalPortState {
        upper_iface: upper_iface.to_string(),
        lower_iface: lower_iface.to_string(),
    })
}

/// External Port 모드를 정리한다.
pub async fn teardown_external_port(state: &ExternalPortState) {
    info!("Tearing down external port configuration");

    for iface in [state.upper_iface.as_str(), state.lower_iface.as_str()] {
        if let Err(e) = veth::set_promisc(iface, false).await {
            warn!(%iface, error = %e, "Failed to disable promisc mode");
        }
    }

    info!("External port teardown complete");
}
