pub mod agents;
pub mod chat;
pub mod health;
pub mod inbox;
pub mod instances;
pub mod tasks;

use std::path::Path;

use axum::routing::{delete, get, patch, post};
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
        .route("/instances/:id/agents/:agent/events", get(chat::events))
        // Tasks
        .route(
            "/instances/:id/tasks",
            get(tasks::list).post(tasks::create),
        )
        .route(
            "/instances/:id/tasks/:task_id",
            patch(tasks::patch).delete(tasks::delete),
        )
        .route(
            "/instances/:id/tasks/:task_id/notes",
            post(tasks::add_note),
        )
        .route(
            "/instances/:id/tasks/:task_id/todos",
            post(tasks::add_todo),
        )
        .route(
            "/instances/:id/tasks/:task_id/todos/:todo_id",
            patch(tasks::patch_todo).delete(tasks::delete_todo),
        )
        .route(
            "/instances/:id/tasks/:task_id/comments",
            post(tasks::add_comment),
        )
        .route(
            "/instances/:id/tasks/:task_id/comments/:comment_id",
            patch(tasks::patch_comment).delete(tasks::delete_comment),
        )
        // Inbox
        .route("/instances/:id/inbox", get(inbox::list))
        .route(
            "/instances/:id/inbox/:index",
            delete(inbox::delete),
        )
        .route(
            "/instances/:id/inbox/:index/reply",
            post(inbox::reply),
        )
        .route(
            "/instances/:id/inbox/:index/forward",
            post(inbox::forward),
        )
        .route("/instances/:id/events", get(inbox::events));

    let static_service = ServeDir::new(static_dir).append_index_html_on_directories(true);

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
