use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::auth::AuthConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub auth: AuthConfig,
    pub data_dir: Arc<PathBuf>,
}

impl AppState {
    pub fn new(pool: SqlitePool, auth: AuthConfig, data_dir: PathBuf) -> Self {
        Self {
            pool,
            auth,
            data_dir: Arc::new(data_dir),
        }
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.data_dir.join("skills")
    }

    pub fn marketplaces_dir(&self) -> PathBuf {
        self.data_dir.join("marketplaces")
    }
}
