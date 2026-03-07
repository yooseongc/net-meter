use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use net_meter_core::{TestProfile, TestState};
use serde::Serialize;
use tracing::info;

use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(status_handler))
        .route("/test/start", post(start_handler))
        .route("/test/stop", post(stop_handler))
}

#[derive(Serialize)]
struct TestStatus {
    state: TestState,
    profile: Option<TestProfile>,
    elapsed_secs: Option<u64>,
}

async fn status_handler(State(state): State<Arc<AppState>>) -> Json<TestStatus> {
    let test_state = *state.test_state.read().await;
    let profile = state.active_profile.read().await.clone();
    let elapsed_secs = state
        .test_start_time
        .read()
        .await
        .map(|t| t.elapsed().as_secs());

    Json(TestStatus {
        state: test_state,
        profile,
        elapsed_secs,
    })
}

async fn start_handler(
    State(state): State<Arc<AppState>>,
    Json(profile): Json<TestProfile>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let current = *state.test_state.read().await;
    if current != TestState::Idle && current != TestState::Completed && current != TestState::Failed
    {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Test already running" })),
        ));
    }

    info!(profile_name = %profile.name, "Received start request");

    let metrics = Arc::clone(&state.metrics);
    let state_clone = Arc::clone(&state);

    // 오케스트레이터에 위임 (non-blocking: 별도 태스크에서 실행)
    tokio::spawn(async move {
        let mut orch = state_clone.orchestrator.lock().await;
        orch.start(profile, metrics, Arc::clone(&state_clone)).await;
    });

    Ok(Json(serde_json::json!({ "status": "starting" })))
}

async fn stop_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let current = *state.test_state.read().await;
    if current == TestState::Idle || current == TestState::Completed {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "No test running" })),
        ));
    }

    info!("Received stop request");

    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let mut orch = state_clone.orchestrator.lock().await;
        orch.stop(Arc::clone(&state_clone)).await;
    });

    Ok(Json(serde_json::json!({ "status": "stopping" })))
}
