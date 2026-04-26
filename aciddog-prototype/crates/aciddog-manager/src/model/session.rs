use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use sqlx::{FromRow, SqlitePool};

use crate::error::AppResult;
use crate::model::user::UserRow;

pub const SESSION_DURATION_DAYS: i64 = 14;

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub token: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_seen: String,
}

pub fn new_token() -> String {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub async fn create(pool: &SqlitePool, user_id: &str) -> AppResult<SessionRow> {
    let token = new_token();
    let now = Utc::now();
    let expires = now + Duration::days(SESSION_DURATION_DAYS);
    let now_str = now.to_rfc3339();
    let expires_str = expires.to_rfc3339();

    sqlx::query(
        "INSERT INTO sessions (token, user_id, created_at, expires_at, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?3)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(&now_str)
    .bind(&expires_str)
    .execute(pool)
    .await?;

    Ok(SessionRow {
        token,
        user_id: user_id.to_string(),
        created_at: now_str.clone(),
        expires_at: expires_str,
        last_seen: now_str,
    })
}

/// Fetch a session + its user, filtering out expired rows. Bumps
/// `last_seen` on a hit. Returns `None` for missing or expired tokens.
pub async fn lookup(pool: &SqlitePool, token: &str) -> AppResult<Option<(UserRow, SessionRow)>> {
    let now: DateTime<Utc> = Utc::now();
    let now_str = now.to_rfc3339();

    #[derive(FromRow)]
    struct Joined {
        // user columns
        u_id: String,
        u_username: String,
        u_password_hash: String,
        u_created_at: String,
        u_password_updated_at: String,
        u_last_login: Option<String>,
        // session columns
        s_token: String,
        s_user_id: String,
        s_created_at: String,
        s_expires_at: String,
        s_last_seen: String,
    }

    let row: Option<Joined> = sqlx::query_as(
        "SELECT u.id AS u_id, u.username AS u_username, u.password_hash AS u_password_hash,
                u.created_at AS u_created_at, u.password_updated_at AS u_password_updated_at,
                u.last_login AS u_last_login,
                s.token AS s_token, s.user_id AS s_user_id, s.created_at AS s_created_at,
                s.expires_at AS s_expires_at, s.last_seen AS s_last_seen
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = ?1 AND s.expires_at > ?2",
    )
    .bind(token)
    .bind(&now_str)
    .fetch_optional(pool)
    .await?;

    let Some(j) = row else { return Ok(None) };

    let _ = sqlx::query("UPDATE sessions SET last_seen = ?1 WHERE token = ?2")
        .bind(&now_str)
        .bind(token)
        .execute(pool)
        .await;

    let user = UserRow {
        id: j.u_id,
        username: j.u_username,
        password_hash: j.u_password_hash,
        created_at: j.u_created_at,
        password_updated_at: j.u_password_updated_at,
        last_login: j.u_last_login,
    };
    let session = SessionRow {
        token: j.s_token,
        user_id: j.s_user_id,
        created_at: j.s_created_at,
        expires_at: j.s_expires_at,
        last_seen: j.s_last_seen,
    };
    Ok(Some((user, session)))
}

pub async fn delete(pool: &SqlitePool, token: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn purge_expired(pool: &SqlitePool) -> AppResult<u64> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?1")
        .bind(&now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
