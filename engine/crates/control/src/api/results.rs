use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};

use crate::result::TestResult;
use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/results", get(list_results))
        .route("/results/:id", delete(delete_result))
}

async fn list_results(State(state): State<Arc<AppState>>) -> Json<Vec<TestResult>> {
    let results = state.test_results.read().await.clone();
    Json(results)
}

async fn delete_result(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let mut results = state.test_results.write().await;
    results.retain(|r| r.id != id);
    StatusCode::NO_CONTENT
}
