use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: String,
    pub password_updated_at: String,
    pub last_login: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
    pub last_login: Option<DateTime<Utc>>,
}

impl UserSummary {
    pub fn from_row(row: &UserRow) -> AppResult<Self> {
        let id = Uuid::parse_str(&row.id)
            .map_err(|_| AppError::Validation(format!("invalid uuid stored: {}", row.id)))?;
        let last_login = match &row.last_login {
            Some(s) => Some(
                s.parse::<DateTime<Utc>>()
                    .map_err(|_| AppError::Validation(format!("invalid last_login stored: {s}")))?,
            ),
            None => None,
        };
        Ok(Self {
            id,
            username: row.username.clone(),
            last_login,
        })
    }
}

pub fn hash_password(plain: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Validation(format!("hashing password: {e}")))
}

pub fn verify_password(hash: &str, plain: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}

pub async fn count(pool: &SqlitePool) -> AppResult<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

pub async fn find_by_username(pool: &SqlitePool, username: &str) -> AppResult<Option<UserRow>> {
    let row: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, created_at, password_updated_at, last_login
         FROM users WHERE username = ?1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> AppResult<Option<UserRow>> {
    let row: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, created_at, password_updated_at, last_login
         FROM users WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list_summaries(pool: &SqlitePool) -> AppResult<Vec<UserSummary>> {
    let rows: Vec<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, created_at, password_updated_at, last_login
         FROM users ORDER BY username",
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(UserSummary::from_row).collect()
}

pub async fn insert(
    pool: &SqlitePool,
    username: &str,
    plain_password: &str,
) -> AppResult<UserSummary> {
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::Validation("username is required".into()));
    }
    if plain_password.is_empty() {
        return Err(AppError::Validation("password is required".into()));
    }
    let id = Uuid::new_v4();
    let id_str = id.to_string();
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let hash = hash_password(plain_password)?;

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, password_updated_at, last_login)
         VALUES (?1, ?2, ?3, ?4, ?4, NULL)",
    )
    .bind(&id_str)
    .bind(username)
    .bind(&hash)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Validation(format!("user `{username}` already exists"))
        }
        _ => AppError::Db(e),
    })?;

    Ok(UserSummary {
        id,
        username: username.to_string(),
        last_login: None,
    })
}

pub async fn update_password(
    pool: &SqlitePool,
    username: &str,
    plain_password: &str,
) -> AppResult<()> {
    if plain_password.is_empty() {
        return Err(AppError::Validation("password is required".into()));
    }
    let hash = hash_password(plain_password)?;
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE users SET password_hash = ?1, password_updated_at = ?2 WHERE username = ?3",
    )
    .bind(&hash)
    .bind(&now)
    .bind(username)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn update_password_by_id(
    pool: &SqlitePool,
    id: &str,
    plain_password: &str,
) -> AppResult<()> {
    if plain_password.is_empty() {
        return Err(AppError::Validation("password is required".into()));
    }
    let hash = hash_password(plain_password)?;
    let now = Utc::now().to_rfc3339();
    let result =
        sqlx::query("UPDATE users SET password_hash = ?1, password_updated_at = ?2 WHERE id = ?3")
            .bind(&hash)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn delete_by_id(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn mark_logged_in(pool: &SqlitePool, user_id: &str) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET last_login = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
