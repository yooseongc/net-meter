use axum::routing::get;
use axum::Json;
use axum::Router;
use serde_json::json;

use crate::state::AppState;

pub fn routes() -> Router<std::sync::Arc<AppState>> {
    Router::new().route("/health", get(health_handler))
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
