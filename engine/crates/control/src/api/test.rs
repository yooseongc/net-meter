use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use net_meter_core::{NetworkMode, TestConfig, TestState};
use tracing::info;

use crate::schema::{RuntimeConfig, TestStatus};
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(status_handler))
        .route("/test/start", post(start_handler))
        .route("/test/stop", post(stop_handler))
}

async fn status_handler(State(state): State<Arc<AppState>>) -> Json<TestStatus> {
    let test_state = *state.test_state.read().await;
    let config = state.active_config.read().await.clone();
    let elapsed_secs = state
        .test_start_time
        .read()
        .await
        .map(|t| t.elapsed().as_secs());

    Json(TestStatus {
        state: test_state,
        config,
        elapsed_secs,
        runtime: RuntimeConfig {
            mode: match state.server_net.mode {
                NetworkMode::Loopback => NetworkMode::Loopback,
                NetworkMode::Namespace => NetworkMode::Namespace,
                NetworkMode::ExternalPort => NetworkMode::ExternalPort,
            },
            upper_iface: state.server_net.upper_iface.clone(),
            lower_iface: state.server_net.lower_iface.clone(),
        },
    })
}

async fn start_handler(
    State(state): State<Arc<AppState>>,
    Json(config): Json<TestConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    {
        let mut current = state.test_state.write().await;
        if *current != TestState::Idle
            && *current != TestState::Completed
            && *current != TestState::Failed
        {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Test already running" })),
            ));
        }
        *current = TestState::Preparing;
    }

    info!(config_name = %config.name, associations = config.associations.len(), "Received start request");

    let mut orch = state.orchestrator.lock().await;
    orch.start(config, Arc::clone(&state)).await;

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

    let mut orch = state.orchestrator.lock().await;
    orch.stop(Arc::clone(&state)).await;

    Ok(Json(serde_json::json!({ "status": "stopping" })))
}
