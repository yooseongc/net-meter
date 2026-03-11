use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 시험 실행 상태 머신
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestState {
    /// 대기 중 (시험 없음)
    Idle,
    /// 준비 중 (namespace 생성, 서버 기동 등)
    Preparing,
    /// Ramp-up 진행 중 (목표 부하까지 선형 증가)
    RampingUp,
    /// 시험 진행 중 (전속력)
    Running,
    /// Ramp-down 진행 중 (부하 감소 후 종료 대기)
    RampingDown,
    /// 중지 요청됨 (정리 중)
    Stopping,
    /// 정상 완료
    Completed,
    /// 오류로 종료
    Failed,
}

impl Default for TestState {
    fn default() -> Self {
        Self::Idle
    }
}

impl std::fmt::Display for TestState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Idle => "idle",
            Self::Preparing => "preparing",
            Self::RampingUp => "ramping_up",
            Self::Running => "running",
            Self::RampingDown => "ramping_down",
            Self::Stopping => "stopping",
            Self::Completed => "completed",
            Self::Failed => "failed",
        };
        write!(f, "{}", s)
    }
}
