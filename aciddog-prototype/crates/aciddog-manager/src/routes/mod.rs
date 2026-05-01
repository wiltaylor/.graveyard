pub mod agents;
pub mod auth;
pub mod channels;
pub mod chat;
pub mod cron;
pub mod health;
pub mod inbox;
pub mod instances;
pub mod memory;
pub mod skills;
pub mod tasks;
pub mod team;
pub mod users;
pub mod workflows;

use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let public_auth = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/whoami", get(auth::whoami));

    let protected = Router::new()
        // Web UI users
        .route("/users", get(users::list).post(users::create))
        .route("/users/:id", delete(users::delete))
        .route("/users/:id/password", post(users::reset_password))
        .route("/auth/password", post(auth::change_my_password))
        .route("/instances", get(instances::list).post(instances::create))
        .route("/instances/:id", delete(instances::delete))
        .route("/instances/:id/agents", get(agents::list))
        .route("/instances/:id/health", get(health::probe))
        .route("/instances/:id/team", get(team::get))
        .route("/instances/:id/team/reload", post(team::reload))
        .route(
            "/instances/:id/team/wcl",
            get(team::read_wcl).post(team::write_wcl),
        )
        .route("/instances/:id/agents/:agent/history", get(chat::history))
        .route("/instances/:id/agents/:agent/messages", post(chat::send))
        .route("/instances/:id/agents/:agent/events", get(chat::events))
        // Tasks
        .route("/instances/:id/tasks", get(tasks::list).post(tasks::create))
        .route(
            "/instances/:id/tasks/:task_id",
            patch(tasks::patch).delete(tasks::delete),
        )
        .route(
            "/instances/:id/tasks/:task_id/approve",
            post(tasks::approve),
        )
        .route(
            "/instances/:id/tasks/:task_id/reject",
            post(tasks::reject),
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
        .route("/instances/:id/workflows/events", get(workflows::events))
        // Cron
        .route("/instances/:id/cron", get(cron::list).post(cron::create))
        .route(
            "/instances/:id/cron/:job_id",
            patch(cron::patch).delete(cron::delete),
        )
        .route("/instances/:id/cron/events", get(cron::events))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_auth,
        ));

    // Skills + marketplaces accept either a session cookie or an
    // instance api_key bearer token, so agent automation on a daemon
    // can hit the same endpoints the web UI uses. See plan §"Agent
    // automation".
    let skills = Router::new()
        .route("/skills", get(skills::list))
        .route("/skills/upload", post(skills::upload))
        .route("/skills/import", post(skills::import))
        .route("/skills/instances/:id", get(skills::list_on_instance))
        .route("/skills/:id", delete(skills::delete))
        .route("/skills/:id/push", post(skills::push))
        .route("/skills/:id/update", post(skills::update_skill))
        .route(
            "/marketplaces",
            get(skills::list_market).post(skills::add_market),
        )
        .route("/marketplaces/:id", delete(skills::delete_market))
        .route("/marketplaces/:id/update", post(skills::update_market))
        .route("/marketplaces/:id/skills", get(skills::list_market_skills))
        .route(
            "/marketplaces/:id/skills/:name/install",
            post(skills::install_market_skill),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_user_or_instance,
        ));

    let api = Router::new()
        .merge(public_auth)
        .merge(protected)
        .merge(skills);

    Router::new()
        .nest("/api", api)
        .fallback(crate::static_assets::handler)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
