pub mod agents;
pub mod channels;
pub mod chat;
pub mod health;
pub mod inbox;
pub mod instances;
pub mod memory;
pub mod tasks;
pub mod team;
pub mod workflows;

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
        .route("/instances/:id/team", get(team::get))
        .route("/instances/:id/agents/:agent/history", get(chat::history))
        .route("/instances/:id/agents/:agent/messages", post(chat::send))
        .route("/instances/:id/agents/:agent/events", get(chat::events))
        // Tasks
        .route("/instances/:id/tasks", get(tasks::list).post(tasks::create))
        .route(
            "/instances/:id/tasks/:task_id",
            patch(tasks::patch).delete(tasks::delete),
        )
        .route("/instances/:id/tasks/:task_id/notes", post(tasks::add_note))
        .route("/instances/:id/tasks/:task_id/todos", post(tasks::add_todo))
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
        .route("/instances/:id/inbox/:index", delete(inbox::delete))
        .route("/instances/:id/inbox/:index/reply", post(inbox::reply))
        .route("/instances/:id/inbox/:index/forward", post(inbox::forward))
        .route("/instances/:id/events", get(inbox::events))
        // Channels
        .route("/instances/:id/channels", get(channels::list))
        .route(
            "/instances/:id/channels/:channel/history",
            get(channels::history),
        )
        .route(
            "/instances/:id/channels/:channel/messages",
            post(channels::send),
        )
        .route(
            "/instances/:id/channels/:channel/events",
            get(channels::events),
        )
        // Memory
        .route("/instances/:id/memory/banks", get(memory::list_banks))
        .route(
            "/instances/:id/memory/banks/:bank/search",
            get(memory::search),
        )
        .route(
            "/instances/:id/memory/banks/:bank/entries",
            post(memory::store),
        )
        .route(
            "/instances/:id/memory/banks/:bank/entries/:entry_id",
            patch(memory::update).delete(memory::remove),
        )
        .route(
            "/instances/:id/memory/banks/:bank/entries/:entry_id/links",
            get(memory::links),
        )
        .route(
            "/instances/:id/memory/banks/:bank/links",
            post(memory::add_link).delete(memory::remove_link),
        )
        .route(
            "/instances/:id/memory/banks/:bank/graph",
            get(memory::graph),
        )
        // Workflows
        .route(
            "/instances/:id/workflows/templates",
            get(workflows::list_templates),
        )
        .route(
            "/instances/:id/workflows/instances",
            get(workflows::list_instances).post(workflows::start),
        )
        .route(
            "/instances/:id/workflows/instances/:wf_id",
            delete(workflows::dismiss),
        )
        .route("/instances/:id/workflows/events", get(workflows::events));

    let static_service = ServeDir::new(static_dir).append_index_html_on_directories(true);

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
