//! HTTP routes for the skills catalog and marketplaces.
//!
//! All routes go through `auth::require_user_or_instance`, which
//! accepts either a logged-in human (session cookie) or a registered
//! aciddog instance presenting its `api_key` as a bearer token. The
//! route layer is intentionally thin — heavy lifting lives in the
//! `skills` module.

use std::io::{Cursor, Read};

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use base64::Engine;
use bytes::Bytes;
use futures::stream::StreamExt;
use serde::Deserialize;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;

use crate::auth::Principal;
use crate::error::{AppError, AppResult};
use crate::ipc::SkillFile;
use crate::skills::catalog::{
    delete_skill, hash_files, is_valid_skill_name, list_skills, store_skill_dir, upsert_skill,
};
use crate::skills::import::{import_from_instance, list_skills_on_instance, RemoteSkillSummary};
use crate::skills::marketplace::{
    add_marketplace, install_skill, list_available_skills, list_marketplaces, refresh_skill,
    remove_marketplace, update_marketplace, ProgressSink,
};
use crate::skills::push::push_skill_to_instances;
use crate::skills::{MarketplaceAvailableSkill, MarketplaceSummary, SkillPushResult, SkillSummary};
use crate::state::AppState;

#[derive(Debug, serde::Serialize)]
pub struct SkillsListResponse {
    pub skills: Vec<SkillSummary>,
}

pub async fn list(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
) -> AppResult<Json<SkillsListResponse>> {
    tracing::debug!(principal = %principal.label(), "GET /api/skills");
    let skills = list_skills(&state.pool).await?;
    Ok(Json(SkillsListResponse { skills }))
}

#[derive(Debug, serde::Serialize)]
pub struct UploadResponse {
    pub id: String,
}

/// Accept a single `.zip` archive containing one top-level directory
/// whose name passes `is_valid_skill_name` and contains `SKILL.md`.
pub async fn upload(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    mut multipart: Multipart,
) -> AppResult<Json<UploadResponse>> {
    tracing::info!(principal = %principal.label(), "POST /api/skills/upload");
    let mut zip_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("multipart: {e}")))?
    {
        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::Validation(format!("read field: {e}")))?
            .to_vec();
        if !bytes.is_empty() {
            zip_bytes = Some(bytes);
            break;
        }
    }
    let bytes = zip_bytes.ok_or_else(|| AppError::Validation("no file in upload".into()))?;
    let (name, files) = unpack_skill_zip(&bytes)?;
    store_skill_dir(&state.skills_dir(), "uploaded", None, &name, &files)?;
    let id = upsert_skill(&state.pool, "uploaded", None, &name, &files, None, None).await?;
    Ok(Json(UploadResponse { id }))
}

fn unpack_skill_zip(bytes: &[u8]) -> AppResult<(String, Vec<SkillFile>)> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| AppError::Validation(format!("not a valid zip: {e}")))?;
    let mut top_level: Option<String> = None;
    let mut files: Vec<SkillFile> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Validation(format!("zip entry {i}: {e}")))?;
        if !entry.is_file() {
            continue;
        }
        let raw_name = entry
            .enclosed_name()
            .ok_or_else(|| AppError::Validation(format!("zip entry {i} has unsafe name")))?;
        let path_str = raw_name.to_string_lossy().replace('\\', "/");
        let mut comps = path_str.splitn(2, '/');
        let head = comps.next().unwrap_or("");
        let rest = comps.next().unwrap_or("");
        if head.is_empty() || rest.is_empty() {
            // file at zip root — reject; we require exactly one
            // top-level directory.
            return Err(AppError::Validation(
                "zip must contain exactly one top-level directory".into(),
            ));
        }
        match &top_level {
            None => top_level = Some(head.to_string()),
            Some(existing) if existing == head => {}
            Some(existing) => {
                return Err(AppError::Validation(format!(
                    "zip has multiple top-level dirs: `{existing}`, `{head}`"
                )));
            }
        }
        if rest.contains("..") {
            return Err(AppError::Validation(format!(
                "zip entry `{path_str}` contains `..`"
            )));
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| AppError::Validation(format!("read zip entry: {e}")))?;
        files.push(SkillFile {
            rel_path: rest.to_string(),
            contents_b64: base64::engine::general_purpose::STANDARD.encode(&buf),
        });
    }
    let name = top_level.ok_or_else(|| AppError::Validation("zip is empty".into()))?;
    if !is_valid_skill_name(&name) {
        return Err(AppError::Validation(format!(
            "top-level dir name `{name}` is not a valid skill id (lowercase alnum + - / _)"
        )));
    }
    if !files.iter().any(|f| f.rel_path == "SKILL.md") {
        return Err(AppError::Validation(
            "zip is missing SKILL.md at the top of the skill directory".into(),
        ));
    }
    // Round-trip the hash so callers see the same shape.
    let _ = hash_files(&files)?;
    Ok((name, files))
}

pub async fn delete(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    tracing::info!(principal = %principal.label(), id, "DELETE /api/skills/:id");
    delete_skill(&state.pool, &state.skills_dir(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct PushRequest {
    pub instance_ids: Vec<Uuid>,
}

#[derive(Debug, serde::Serialize)]
pub struct PushResponse {
    pub results: Vec<SkillPushResult>,
}

pub async fn push(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<String>,
    Json(req): Json<PushRequest>,
) -> AppResult<Json<PushResponse>> {
    tracing::info!(principal = %principal.label(), id, count = req.instance_ids.len(), "POST /api/skills/:id/push");
    if req.instance_ids.is_empty() {
        return Err(AppError::Validation("instance_ids is empty".into()));
    }
    let results = push_skill_to_instances(&state, &id, &req.instance_ids).await?;
    Ok(Json(PushResponse { results }))
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub instance_id: Uuid,
    pub skill_name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ImportResponse {
    pub id: String,
}

pub async fn import(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Json(req): Json<ImportRequest>,
) -> AppResult<Json<ImportResponse>> {
    tracing::info!(principal = %principal.label(), instance = %req.instance_id, skill = %req.skill_name, "POST /api/skills/import");
    let id = import_from_instance(&state, req.instance_id, &req.skill_name).await?;
    Ok(Json(ImportResponse { id }))
}

#[derive(Debug, serde::Serialize)]
pub struct ListInstanceSkillsResponse {
    pub skills: Vec<RemoteSkillSummary>,
}

/// Helper for the import dialog: list skills installed on an instance
/// before the user picks one. Mounted at
/// `GET /api/skills/instances/:id` so it sits next to the rest of the
/// catalog API rather than tunneled through `/instances/:id/...`.
pub async fn list_on_instance(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(instance_id): Path<Uuid>,
) -> AppResult<Json<ListInstanceSkillsResponse>> {
    tracing::debug!(principal = %principal.label(), %instance_id, "GET /api/skills/instances/:id");
    let skills = list_skills_on_instance(&state.pool, instance_id).await?;
    Ok(Json(ListInstanceSkillsResponse { skills }))
}

pub async fn update_skill(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    tracing::info!(principal = %principal.label(), id, "POST /api/skills/:id/update");
    refresh_skill(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Serialize)]
pub struct MarketplacesListResponse {
    pub marketplaces: Vec<MarketplaceSummary>,
}

pub async fn list_market(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
) -> AppResult<Json<MarketplacesListResponse>> {
    tracing::debug!(principal = %principal.label(), "GET /api/marketplaces");
    let marketplaces = list_marketplaces(&state.pool).await?;
    Ok(Json(MarketplacesListResponse { marketplaces }))
}

#[derive(Debug, Deserialize)]
pub struct AddMarketplaceRequest {
    pub git_url: String,
    #[serde(default, rename = "ref")]
    pub ref_spec: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// `POST /api/marketplaces` — chunked NDJSON response. Each line is one
/// of `{"event":"progress","message":"..."}`,
/// `{"event":"done","summary":{...}}`, or
/// `{"event":"error","error":"..."}`. The browser reads the body as a
/// stream so the modal can render staged progress (clone → resolve →
/// enumerate) instead of looking frozen during a long git clone.
pub async fn add_market(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Json(req): Json<AddMarketplaceRequest>,
) -> Response {
    tracing::info!(principal = %principal.label(), git_url = %req.git_url, "POST /api/marketplaces");
    stream_marketplace_op(move |progress| async move {
        add_marketplace(
            &state,
            &req.git_url,
            req.ref_spec.as_deref(),
            req.name.as_deref(),
            &progress,
        )
        .await
    })
}

pub async fn delete_market(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    tracing::info!(principal = %principal.label(), id, "DELETE /api/marketplaces/:id");
    remove_marketplace(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_market(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<String>,
) -> Response {
    tracing::info!(principal = %principal.label(), id, "POST /api/marketplaces/:id/update");
    stream_marketplace_op(move |progress| async move {
        update_marketplace(&state, &id, &progress).await
    })
}

#[derive(Debug, serde::Serialize)]
pub struct AvailableSkillsResponse {
    pub skills: Vec<MarketplaceAvailableSkill>,
}

pub async fn list_market_skills(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<Json<AvailableSkillsResponse>> {
    tracing::debug!(principal = %principal.label(), id, "GET /api/marketplaces/:id/skills");
    let skills = list_available_skills(&state, &id).await?;
    Ok(Json(AvailableSkillsResponse { skills }))
}

#[derive(Debug, serde::Serialize)]
pub struct InstallFromMarketResponse {
    pub id: String,
}

pub async fn install_market_skill(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<Principal>,
    Path((id, name)): Path<(String, String)>,
) -> AppResult<Json<InstallFromMarketResponse>> {
    tracing::info!(principal = %principal.label(), id, name, "POST /api/marketplaces/:id/skills/:name/install");
    let row_id = install_skill(&state, &id, &name).await?;
    Ok(Json(InstallFromMarketResponse { id: row_id }))
}

/// Drive a long-running marketplace operation through a tokio mpsc
/// channel and emit each progress message as one NDJSON line. The
/// final line is either `{"event":"done","summary":{...}}` or
/// `{"event":"error","error":"..."}`.
fn stream_marketplace_op<F, Fut>(work: F) -> Response
where
    F: FnOnce(ProgressSink) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = AppResult<MarketplaceSummary>> + Send + 'static,
{
    let (progress, rx) = ProgressSink::channel();
    let (line_tx, line_rx) = tokio::sync::mpsc::unbounded_channel::<Result<Bytes, std::io::Error>>();
    // Forward each progress message as a NDJSON `progress` line.
    let line_tx_progress = line_tx.clone();
    tokio::spawn(async move {
        let mut rx = rx;
        while let Some(message) = rx.recv().await {
            let line = serde_json::json!({
                "event": "progress",
                "message": message,
            });
            let _ = line_tx_progress
                .send(Ok(serialize_line(&line)));
        }
    });
    // Run the actual work in the background, then emit a terminal
    // `done` or `error` line.
    tokio::spawn(async move {
        let outcome = work(progress).await;
        let line = match outcome {
            Ok(summary) => serde_json::json!({
                "event": "done",
                "summary": summary,
            }),
            Err(e) => serde_json::json!({
                "event": "error",
                "error": e.to_string(),
            }),
        };
        let _ = line_tx.send(Ok(serialize_line(&line)));
    });
    let stream = UnboundedReceiverStream::new(line_rx).map(|res| res);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(stream))
        .expect("build streaming response")
}

fn serialize_line(value: &serde_json::Value) -> Bytes {
    let mut s = serde_json::to_string(value).expect("serialize ndjson line");
    s.push('\n');
    Bytes::from(s)
}
