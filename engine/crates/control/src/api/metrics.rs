use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use net_meter_core::MetricsSnapshot;

use crate::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/metrics/ws", get(ws_handler))
}

/// 최신 메트릭 스냅샷 반환
async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<MetricsSnapshot> {
    let snapshot = state.latest_snapshot.read().await.clone();
    Json(snapshot)
}

/// WebSocket으로 실시간 메트릭 스트림 (1초 간격)
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.snapshot_tx.subscribe();

    loop {
        match rx.recv().await {
            Ok(snapshot) => {
                let msg = match serde_json::to_string(&snapshot) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if socket.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }
}
