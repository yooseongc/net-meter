use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use net_meter_core::TestConfig;
use uuid::Uuid;

use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/profiles", get(list_profiles))
        .route("/profiles", post(create_profile))
        .route("/profiles/:id", delete(delete_profile))
}

async fn list_profiles(State(state): State<Arc<AppState>>) -> Json<Vec<TestConfig>> {
    let configs = state.saved_configs.read().await;
    Json(configs.values().cloned().collect())
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(mut config): Json<TestConfig>,
) -> Json<TestConfig> {
    if config.id.is_empty() {
        config.id = Uuid::new_v4().to_string();
    }
    let mut configs = state.saved_configs.write().await;
    configs.insert(config.id.clone(), config.clone());
    Json(config)
}

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut configs = state.saved_configs.write().await;
    if configs.remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
