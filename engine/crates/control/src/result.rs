use net_meter_core::{MetricsSnapshot, TestConfig};
use serde::{Deserialize, Serialize};

/// 시험 완료 후 저장되는 결과 레코드
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// 고유 ID (UUID v4)
    pub id: String,

    /// 시험 설정 (전체 TestConfig 보존)
    pub config: TestConfig,

    /// 시험 시작 Unix timestamp (초)
    pub started_at_secs: u64,

    /// 시험 종료 Unix timestamp (초)
    pub ended_at_secs: u64,

    /// 실제 경과 시간 (초)
    pub elapsed_secs: u64,

    /// 시험 종료 직전 최종 MetricsSnapshot
    pub final_snapshot: MetricsSnapshot,
}
