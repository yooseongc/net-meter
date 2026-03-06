pub mod health;
pub mod metrics;
pub mod profiles;
pub mod test;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing::warn;

use crate::state::AppState;

/// API 라우터를 구성한다.
///
/// `web_dir`이 Some이면 해당 디렉터리의 정적 파일을 `/` 경로에서 서빙한다.
/// React SPA를 위해 알 수 없는 경로는 `index.html`로 fallback한다.
pub fn router(state: Arc<AppState>, web_dir: Option<PathBuf>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .nest("/api", api_routes())
        .layer(cors)
        .with_state(state);

    if let Some(dir) = web_dir {
        if dir.is_dir() {
            let index_html = dir.join("index.html");
            let serve_dir =
                ServeDir::new(&dir).not_found_service(ServeFile::new(&index_html));
            app = app.fallback_service(serve_dir);
        } else {
            warn!(path = %dir.display(), "web-dir not found, static file serving disabled");
        }
    }

    app
}

fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .merge(health::routes())
        .merge(test::routes())
        .merge(metrics::routes())
        .merge(profiles::routes())
}
