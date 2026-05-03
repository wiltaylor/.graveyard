use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::model::instance::{InstanceRow, InstanceSummary};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DesiredState {
    Stopped,
    Running,
}

impl DesiredState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DesiredState::Stopped => "stopped",
            DesiredState::Running => "running",
        }
    }

    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "stopped" => Ok(DesiredState::Stopped),
            "running" => Ok(DesiredState::Running),
            other => Err(AppError::Validation(format!(
                "unknown desired_state `{other}`"
            ))),
        }
    }
}

impl Default for DesiredState {
    fn default() -> Self {
        DesiredState::Stopped
    }
}

/// Full row shape — string/i64-typed; mirrors the on-disk columns.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ManagedDaemonRow {
    pub id: String,
    pub name: String,
    pub work_dir: String,
    pub socket_path: String,
    pub desired_state: String,
    pub start_on_manager_boot: i64,
    pub idle_timeout_secs: Option<i64>,
    pub start_on_demand: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Outward-facing typed view.
#[derive(Debug, Clone, Serialize)]
pub struct ManagedDaemonSummary {
    pub id: Uuid,
    pub name: String,
    pub work_dir: PathBuf,
    pub socket_path: String,
    pub desired_state: DesiredState,
    pub start_on_manager_boot: bool,
    pub idle_timeout_secs: Option<u32>,
    pub start_on_demand: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ManagedDaemonSummary {
    pub fn from_row(row: ManagedDaemonRow) -> AppResult<Self> {
        let id = Uuid::parse_str(&row.id)
            .map_err(|_| AppError::Validation(format!("invalid uuid stored: {}", row.id)))?;
        let desired_state = DesiredState::parse(&row.desired_state)?;
        let created_at = row.created_at.parse::<DateTime<Utc>>().map_err(|_| {
            AppError::Validation(format!("invalid created_at stored: {}", row.created_at))
        })?;
        let updated_at = row.updated_at.parse::<DateTime<Utc>>().map_err(|_| {
            AppError::Validation(format!("invalid updated_at stored: {}", row.updated_at))
        })?;
        let idle_timeout_secs = match row.idle_timeout_secs {
            None => None,
            Some(v) => Some(u32::try_from(v).map_err(|_| {
                AppError::Validation(format!("invalid idle_timeout_secs stored: {v}"))
            })?),
        };
        Ok(Self {
            id,
            name: row.name,
            work_dir: PathBuf::from(row.work_dir),
            socket_path: row.socket_path,
            desired_state,
            start_on_manager_boot: row.start_on_manager_boot != 0,
            idle_timeout_secs,
            start_on_demand: row.start_on_demand != 0,
            created_at,
            updated_at,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateManagedDaemon {
    pub name: String,
    pub work_dir: PathBuf,
    pub socket_path: String,
    #[serde(default)]
    pub desired_state: Option<DesiredState>,
    #[serde(default)]
    pub start_on_manager_boot: bool,
    #[serde(default)]
    pub idle_timeout_secs: Option<u32>,
    #[serde(default)]
    pub start_on_demand: bool,
}

impl CreateManagedDaemon {
    /// Validate inputs.
    ///
    /// idle_timeout_secs=Some(0) is rejected BEFORE work_dir canonicalisation
    /// so callers can exercise that branch without preparing a real directory.
    /// validate() does not mutate self; insert_with_paired_instance re-canonicalises
    /// before INSERT so the canonical absolute path is what lands in the DB and
    /// the UNIQUE constraint catches symlink/relative-path duplicates.
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation("name must not be empty".into()));
        }
        if self.socket_path.trim().is_empty() {
            return Err(AppError::Validation("socket_path must not be empty".into()));
        }
        if matches!(self.idle_timeout_secs, Some(0)) {
            return Err(AppError::Validation(
                "idle_timeout_secs must be at least 1 (or null to disable)".into(),
            ));
        }
        let canonical = std::fs::canonicalize(&self.work_dir).map_err(|_| {
            AppError::Validation(format!(
                "work_dir does not exist or is not accessible: {}",
                self.work_dir.display()
            ))
        })?;
        let meta = std::fs::metadata(&canonical).map_err(|_| {
            AppError::Validation(format!(
                "work_dir does not exist or is not accessible: {}",
                canonical.display()
            ))
        })?;
        if !meta.is_dir() {
            return Err(AppError::Validation(format!(
                "work_dir is not a directory: {}",
                canonical.display()
            )));
        }
        Ok(())
    }
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<ManagedDaemonSummary>> {
    let rows: Vec<ManagedDaemonRow> = sqlx::query_as(
        "SELECT id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at
         FROM managed_daemons ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(ManagedDaemonSummary::from_row)
        .collect()
}

pub async fn fetch(pool: &SqlitePool, id: Uuid) -> AppResult<ManagedDaemonRow> {
    let row: Option<ManagedDaemonRow> = sqlx::query_as(
        "SELECT id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at
         FROM managed_daemons WHERE id = ?1",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    row.ok_or(AppError::NotFound)
}

pub async fn fetch_summary(pool: &SqlitePool, id: Uuid) -> AppResult<ManagedDaemonSummary> {
    let row = fetch(pool, id).await?;
    ManagedDaemonSummary::from_row(row)
}

pub async fn list_start_on_boot(pool: &SqlitePool) -> AppResult<Vec<ManagedDaemonRow>> {
    let rows: Vec<ManagedDaemonRow> = sqlx::query_as(
        "SELECT id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at
         FROM managed_daemons WHERE start_on_manager_boot = 1 ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn set_desired_state(pool: &SqlitePool, id: Uuid, state: DesiredState) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let result =
        sqlx::query("UPDATE managed_daemons SET desired_state = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(state.as_str())
            .bind(&now)
            .bind(id.to_string())
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

/// Map a sqlx unique-violation into an AppError::Validation that names the
/// offending column. Falls back to AppError::Db on any other error.
///
/// We pattern-match the error message because SQLite surfaces the offending
/// column list via "UNIQUE constraint failed: <table>.<col>" and that is the
/// most reliable signal across sqlx 0.8 — the numeric extended code (2067)
/// is also checked but the message is what disambiguates which UNIQUE constraint
/// fired (managed_daemons.name vs managed_daemons.work_dir vs instances.name).
fn map_insert_err(e: sqlx::Error, daemon_name: &str, work_dir_display: String) -> AppError {
    if let sqlx::Error::Database(db) = &e {
        if db.is_unique_violation() {
            let msg = db.message();
            if msg.contains("managed_daemons.name") {
                return AppError::Validation(format!(
                    "a managed daemon named `{daemon_name}` already exists"
                ));
            }
            if msg.contains("managed_daemons.work_dir") {
                return AppError::Validation(format!(
                    "a managed daemon already uses work_dir `{work_dir_display}`"
                ));
            }
            if msg.contains("instances.name") {
                return AppError::Validation(format!(
                    "an instance named `{daemon_name}` already exists"
                ));
            }
            return AppError::Validation(format!("unique constraint violated: {msg}"));
        }
    }
    AppError::Db(e)
}

pub async fn insert_with_paired_instance(
    pool: &SqlitePool,
    input: CreateManagedDaemon,
) -> AppResult<(ManagedDaemonSummary, InstanceSummary)> {
    input.validate()?;
    // validate() doesn't mutate the input; canonicalise here so the absolute
    // path is what lands in the DB (UNIQUE on work_dir then catches relative-
    // path / symlink aliases).
    let canonical_work_dir = std::fs::canonicalize(&input.work_dir).map_err(|_| {
        AppError::Validation(format!(
            "work_dir does not exist or is not accessible: {}",
            input.work_dir.display()
        ))
    })?;
    let work_dir_str = canonical_work_dir.to_string_lossy().into_owned();

    let daemon_id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    let desired_state = input.desired_state.unwrap_or_default();
    let desired_state_str = desired_state.as_str();
    let start_on_manager_boot_int: i64 = if input.start_on_manager_boot { 1 } else { 0 };
    let start_on_demand_int: i64 = if input.start_on_demand { 1 } else { 0 };
    let idle_timeout_db: Option<i64> = input.idle_timeout_secs.map(|v| v as i64);

    let daemon_id_str = daemon_id.to_string();
    let instance_id_str = instance_id.to_string();

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO managed_daemons (id, name, work_dir, socket_path, desired_state, start_on_manager_boot, idle_timeout_secs, start_on_demand, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&daemon_id_str)
    .bind(&input.name)
    .bind(&work_dir_str)
    .bind(&input.socket_path)
    .bind(desired_state_str)
    .bind(start_on_manager_boot_int)
    .bind(idle_timeout_db)
    .bind(start_on_demand_int)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&mut *tx)
    .await
    .map_err(|e| map_insert_err(e, &input.name, work_dir_str.clone()))?;

    // The instances row is written via raw SQL inside the same tx (rather
    // than crate::model::instance::insert) because CreateInstance does not
    // accept managed_by_id.
    sqlx::query(
        "INSERT INTO instances (id, name, transport, url, socket_path, api_key, tls_ca_path, managed_by_id, created_at)
         VALUES (?1, ?2, 'unix', NULL, ?3, NULL, NULL, ?4, ?5)",
    )
    .bind(&instance_id_str)
    .bind(&input.name)
    .bind(&input.socket_path)
    .bind(&daemon_id_str)
    .bind(&now_str)
    .execute(&mut *tx)
    .await
    .map_err(|e| map_insert_err(e, &input.name, work_dir_str.clone()))?;

    tx.commit().await?;

    let daemon_summary = ManagedDaemonSummary {
        id: daemon_id,
        name: input.name.clone(),
        work_dir: canonical_work_dir,
        socket_path: input.socket_path.clone(),
        desired_state,
        start_on_manager_boot: input.start_on_manager_boot,
        idle_timeout_secs: input.idle_timeout_secs,
        start_on_demand: input.start_on_demand,
        created_at: now,
        updated_at: now,
    };

    let instance_row = InstanceRow {
        id: instance_id_str,
        name: input.name,
        transport: "unix".to_string(),
        url: None,
        socket_path: Some(input.socket_path),
        api_key: None,
        tls_ca_path: None,
        created_at: now_str,
    };
    let instance_summary = InstanceSummary::from_row(instance_row)?;

    Ok((daemon_summary, instance_summary))
}

/// Tear down a managed daemon and its paired instance row in a single tx.
///
/// This helper is the runtime source of truth because the schema-level CASCADE
/// on instances.managed_by_id is declared-only (PRAGMA foreign_keys is not
/// enabled).
pub async fn delete_with_paired_instance(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let id_str = id.to_string();
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM instances WHERE managed_by_id = ?1")
        .bind(&id_str)
        .execute(&mut *tx)
        .await?;

    let res = sqlx::query("DELETE FROM managed_daemons WHERE id = ?1")
        .bind(&id_str)
        .execute(&mut *tx)
        .await?;

    if res.rows_affected() == 0 {
        // Drop tx without committing => implicit rollback of the instances
        // delete above (which would have been a no-op anyway since the
        // managed_daemons row never existed).
        return Err(AppError::NotFound);
    }

    tx.commit().await?;
    Ok(())
}
