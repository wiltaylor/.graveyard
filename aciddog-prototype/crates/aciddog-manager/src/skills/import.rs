//! Pull a skill from a registered aciddog instance into the manager
//! catalog. Round-trips through `RequestCmd::SkillRead`, which emits
//! the full directory contents over the wire.

use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::{RequestCmd, SkillFile};
use crate::skills::catalog::{is_valid_skill_name, store_skill_dir, upsert_skill};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct WireSkillContents {
    name: String,
    files: Vec<WireSkillFile>,
}

#[derive(Debug, Deserialize)]
struct WireSkillFile {
    rel_path: String,
    contents_b64: String,
}

pub async fn import_from_instance(
    state: &AppState,
    instance_id: Uuid,
    skill_name: &str,
) -> AppResult<String> {
    if !is_valid_skill_name(skill_name) {
        return Err(AppError::Validation(format!(
            "invalid skill name `{skill_name}`"
        )));
    }
    let row = crate::skills::push::load_instance_row(&state.pool, instance_id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::SkillRead {
            name: skill_name.to_string(),
        })
        .await?;
    if !resp.ok {
        return Err(AppError::Validation(
            resp.error
                .or(resp.message)
                .unwrap_or_else(|| "remote returned not-ok with no message".into()),
        ));
    }
    let data = resp
        .data
        .ok_or_else(|| AppError::Validation("remote returned ok but no `data`".into()))?;
    let contents: WireSkillContents = serde_json::from_value(data)
        .map_err(|e| AppError::Validation(format!("decoding SkillRead reply: {e}")))?;
    let files: Vec<SkillFile> = contents
        .files
        .into_iter()
        .map(|f| SkillFile {
            rel_path: f.rel_path,
            contents_b64: f.contents_b64,
        })
        .collect();
    if files.is_empty() {
        return Err(AppError::Validation(format!(
            "skill `{skill_name}` on instance has no files"
        )));
    }

    let source_ref = instance_id.to_string();
    let _path = store_skill_dir(
        &state.skills_dir(),
        "imported",
        Some(&source_ref),
        &contents.name,
        &files,
    )?;
    let id = upsert_skill(
        &state.pool,
        "imported",
        Some(&source_ref),
        &contents.name,
        &files,
        None,
        None,
    )
    .await?;
    Ok(id)
}

/// List skills installed on a registered instance — used by the
/// "Import skill" picker in the web UI before the user chooses one.
pub async fn list_skills_on_instance(
    pool: &SqlitePool,
    instance_id: Uuid,
) -> AppResult<Vec<RemoteSkillSummary>> {
    let row = crate::skills::push::load_instance_row(pool, instance_id).await?;
    let client = Client::from_row(&row)?;
    let resp = client.send_rpc(&RequestCmd::SkillsList).await?;
    if !resp.ok {
        return Err(AppError::Validation(
            resp.error
                .or(resp.message)
                .unwrap_or_else(|| "remote returned not-ok".into()),
        ));
    }
    let data = resp
        .data
        .ok_or_else(|| AppError::Validation("remote returned ok but no `data`".into()))?;
    serde_json::from_value::<Vec<RemoteSkillSummary>>(data)
        .map_err(|e| AppError::Validation(format!("decoding SkillsList reply: {e}")))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RemoteSkillSummary {
    pub name: String,
    #[serde(default)]
    pub description: String,
}
