use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use net_meter_core::TestProfile;
use uuid::Uuid;

use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/profiles", get(list_profiles))
        .route("/profiles", post(create_profile))
        .route("/profiles/:id", delete(delete_profile))
}

async fn list_profiles(State(state): State<Arc<AppState>>) -> Json<Vec<TestProfile>> {
    let profiles = state.saved_profiles.read().await;
    Json(profiles.values().cloned().collect())
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(mut profile): Json<TestProfile>,
) -> Json<TestProfile> {
    // ID가 없으면 새로 발급
    if profile.id.is_empty() {
        profile.id = Uuid::new_v4().to_string();
    }
    let mut profiles = state.saved_profiles.write().await;
    profiles.insert(profile.id.clone(), profile.clone());
    Json(profile)
}

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut profiles = state.saved_profiles.write().await;
    if profiles.remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
