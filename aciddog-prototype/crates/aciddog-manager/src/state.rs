use sqlx::SqlitePool;

use crate::auth::AuthConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub auth: AuthConfig,
}

impl AppState {
    pub fn new(pool: SqlitePool, auth: AuthConfig) -> Self {
        Self { pool, auth }
    }
}
