use serde::Serialize;

/// 실시간 이벤트 로그에 브로드캐스트되는 이벤트 타입.
///
/// SSE 엔드포인트(`GET /api/events/stream`)를 통해 프론트엔드로 스트리밍된다.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestEvent {
    /// 시험 시작
    TestStarted { config_name: String, test_type: String, duration_secs: u64 },
    /// 시험 중지
    TestStopped { reason: String },
    /// Ramp-up 단계 시작
    RampUpStarted { ramp_up_secs: u64 },
    /// Ramp-up 완료 (전속력 전환)
    RampUpComplete,
    /// NS 환경 준비 완료
    NsSetupComplete,
    /// NS 환경 정리 완료
    NsTeardownComplete,
    /// 임계값 위반 감지
    ThresholdViolation { violations: Vec<String> },
    /// 오류
    Error { message: String },
}
