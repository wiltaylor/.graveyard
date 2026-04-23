use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Https,
    Unix,
}

impl Transport {
    fn as_str(&self) -> &'static str {
        match self {
            Transport::Https => "https",
            Transport::Unix => "unix",
        }
    }

    fn parse(s: &str) -> AppResult<Self> {
        match s {
            "https" => Ok(Transport::Https),
            "unix" => Ok(Transport::Unix),
            other => Err(AppError::Validation(format!("unknown transport `{other}`"))),
        }
    }
}

/// Full row shape — includes secrets; stays internal.
#[derive(Debug, Clone, FromRow)]
pub struct InstanceRow {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub url: Option<String>,
    pub socket_path: Option<String>,
    pub api_key: Option<String>,
    pub tls_ca_path: Option<String>,
    pub created_at: String,
}

/// Outward-facing view — omits `api_key`.
#[derive(Debug, Clone, Serialize)]
pub struct InstanceSummary {
    pub id: Uuid,
    pub name: String,
    pub transport: Transport,
    pub url: Option<String>,
    pub socket_path: Option<String>,
    pub tls_ca_path: Option<String>,
    pub has_api_key: bool,
    pub created_at: DateTime<Utc>,
}

impl InstanceSummary {
    pub fn from_row(row: InstanceRow) -> AppResult<Self> {
        let id = Uuid::parse_str(&row.id)
            .map_err(|_| AppError::Validation(format!("invalid uuid stored: {}", row.id)))?;
        let transport = Transport::parse(&row.transport)?;
        let created_at = row.created_at.parse::<DateTime<Utc>>().map_err(|_| {
            AppError::Validation(format!("invalid created_at stored: {}", row.created_at))
        })?;
        Ok(Self {
            id,
            name: row.name,
            transport,
            url: row.url,
            socket_path: row.socket_path,
            tls_ca_path: row.tls_ca_path,
            has_api_key: row.api_key.is_some(),
            created_at,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInstance {
    pub name: String,
    pub transport: Transport,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub socket_path: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub tls_ca_path: Option<String>,
}

impl CreateInstance {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation("name is required".into()));
        }
        match self.transport {
            Transport::Https => {
                if self.url.as_deref().map(str::trim).unwrap_or("").is_empty() {
                    return Err(AppError::Validation(
                        "url is required when transport=https".into(),
                    ));
                }
                if self.socket_path.is_some() {
                    return Err(AppError::Validation(
                        "socket_path must be empty when transport=https".into(),
                    ));
                }
            }
            Transport::Unix => {
                if self
                    .socket_path
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(AppError::Validation(
                        "socket_path is required when transport=unix".into(),
                    ));
                }
                if self.url.is_some() {
                    return Err(AppError::Validation(
                        "url must be empty when transport=unix".into(),
                    ));
                }
                if self.api_key.is_some() {
                    return Err(AppError::Validation(
                        "api_key must be empty when transport=unix (filesystem perms are the trust boundary)".into(),
                    ));
                }
                if self.tls_ca_path.is_some() {
                    return Err(AppError::Validation(
                        "tls_ca_path must be empty when transport=unix".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

pub async fn insert(pool: &SqlitePool, input: CreateInstance) -> AppResult<InstanceSummary> {
    input.validate()?;
    let id = Uuid::new_v4();
    let created_at = Utc::now();
    let created_at_str = created_at.to_rfc3339();
    let transport_str = input.transport.as_str();
    let id_str = id.to_string();

    sqlx::query(
        "INSERT INTO instances (id, name, transport, url, socket_path, api_key, tls_ca_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id_str)
    .bind(&input.name)
    .bind(transport_str)
    .bind(&input.url)
    .bind(&input.socket_path)
    .bind(&input.api_key)
    .bind(&input.tls_ca_path)
    .bind(&created_at_str)
    .execute(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Validation(format!("an instance named `{}` already exists", input.name))
        }
        _ => AppError::Db(e),
    })?;

    Ok(InstanceSummary {
        id,
        name: input.name,
        transport: input.transport,
        url: input.url,
        socket_path: input.socket_path,
        tls_ca_path: input.tls_ca_path,
        has_api_key: input.api_key.is_some(),
        created_at,
    })
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<InstanceSummary>> {
    let rows: Vec<InstanceRow> = sqlx::query_as(
        "SELECT id, name, transport, url, socket_path, api_key, tls_ca_path, created_at
         FROM instances ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(InstanceSummary::from_row).collect()
}

pub async fn fetch(pool: &SqlitePool, id: Uuid) -> AppResult<InstanceRow> {
    let row: Option<InstanceRow> = sqlx::query_as(
        "SELECT id, name, transport, url, socket_path, api_key, tls_ca_path, created_at
         FROM instances WHERE id = ?1",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    row.ok_or(AppError::NotFound)
}

pub async fn delete(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM instances WHERE id = ?1")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}
