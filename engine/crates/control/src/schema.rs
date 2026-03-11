use net_meter_core::{NetworkMode, TestConfig, TestState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeConfig {
    pub mode: NetworkMode,
    pub upper_iface: String,
    pub lower_iface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestStatus {
    pub state: TestState,
    pub config: Option<TestConfig>,
    pub elapsed_secs: Option<u64>,
    pub runtime: RuntimeConfig,
}
