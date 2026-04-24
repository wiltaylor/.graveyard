pub mod agents;
pub mod chat;
pub mod health;
pub mod instances;

use std::path::Path;

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn build_router(state: AppState, static_dir: &Path) -> Router {
    let api = Router::new()
        .route("/instances", get(instances::list).post(instances::create))
        .route("/instances/:id", delete(instances::delete))
        .route("/instances/:id/agents", get(agents::list))
        .route("/instances/:id/health", get(health::probe))
        .route(
            "/instances/:id/agents/:agent/history",
            get(chat::history),
        )
        .route(
            "/instances/:id/agents/:agent/messages",
            post(chat::send),
        )
        .route("/instances/:id/agents/:agent/events", get(chat::events));

    let static_service = ServeDir::new(static_dir).append_index_html_on_directories(true);

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
