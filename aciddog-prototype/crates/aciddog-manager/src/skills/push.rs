//! Push a catalog skill out to one or more registered aciddog instances.
//!
//! Per the design decision: copy files only. We do not auto-trigger
//! `ServerReload` and we don't edit the instance's WCL — the user
//! reloads from the existing Team screen button.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::client::Client;
use crate::error::{AppError, AppResult};
use crate::ipc::{RequestCmd, SkillFile};
use crate::model::instance::InstanceRow;
use crate::skills::catalog::{hash_files, load_skill_files, locate_skill, skill_dir};
use crate::skills::SkillPushResult;
use crate::state::AppState;

pub async fn push_skill_to_instances(
    state: &AppState,
    skill_id: &str,
    instance_ids: &[Uuid],
) -> AppResult<Vec<SkillPushResult>> {
    let (source_kind, source_ref, name, content_hash) =
        locate_skill(&state.pool, skill_id).await?;
    let dir = skill_dir(
        &state.skills_dir(),
        &source_kind,
        source_ref.as_deref(),
        &name,
    );
    if !dir.is_dir() {
        return Err(AppError::Validation(format!(
            "skill `{skill_id}` is registered but its directory is missing — re-upload it"
        )));
    }
    let files = load_skill_files(&dir)?;
    let recomputed = hash_files(&files)?;
    if recomputed != content_hash {
        tracing::warn!(
            skill_id,
            stored = %content_hash,
            on_disk = %recomputed,
            "catalog hash drift — using on-disk content"
        );
    }

    let mut out = Vec::with_capacity(instance_ids.len());
    for instance_id in instance_ids {
        match push_one(&state.pool, *instance_id, &name, &files).await {
            Ok(()) => out.push(SkillPushResult {
                instance_id: instance_id.to_string(),
                ok: true,
                message: Some(format!("wrote `{name}`")),
                error: None,
            }),
            Err(e) => out.push(SkillPushResult {
                instance_id: instance_id.to_string(),
                ok: false,
                message: None,
                error: Some(e.to_string()),
            }),
        }
    }
    for result in &out {
        if !result.ok {
            continue;
        }
        let _ = record_push(&state.pool, skill_id, &result.instance_id, &recomputed).await;
    }
    Ok(out)
}

async fn push_one(
    pool: &SqlitePool,
    instance_id: Uuid,
    name: &str,
    files: &[SkillFile],
) -> AppResult<()> {
    let row = load_instance_row(pool, instance_id).await?;
    let client = Client::from_row(&row)?;
    let resp = client
        .send_rpc(&RequestCmd::SkillWrite {
            name: name.to_string(),
            files: files.to_vec(),
        })
        .await?;
    if !resp.ok {
        return Err(AppError::Validation(
            resp.error
                .or(resp.message)
                .unwrap_or_else(|| "push failed".into()),
        ));
    }
    Ok(())
}

pub async fn load_instance_row(pool: &SqlitePool, id: Uuid) -> AppResult<InstanceRow> {
    let row = sqlx::query_as::<_, InstanceRow>(
        "SELECT id, name, transport, url, socket_path, api_key, tls_ca_path, created_at FROM instances WHERE id = ?1",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.ok_or(AppError::NotFound)
}

async fn record_push(
    pool: &SqlitePool,
    skill_id: &str,
    instance_id: &str,
    hash: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO skill_pushes(skill_id, instance_id, pushed_at, pushed_hash)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(skill_id, instance_id) DO UPDATE SET
            pushed_at = excluded.pushed_at,
            pushed_hash = excluded.pushed_hash",
    )
    .bind(skill_id)
    .bind(instance_id)
    .bind(&now)
    .bind(hash)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}
