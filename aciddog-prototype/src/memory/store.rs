//! Per-bank cozo database: `entries` relation + HNSW index on the
//! embedding column, plus the few CRUD helpers the IPC handler needs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use cozo::{DataValue, DbInstance, Num, ScriptMutability};
use uuid::Uuid;

use super::embedder::EmbedderPool;
use super::MemoryError;
use crate::config::MemoryBank;

/// One stored memory entry and its distance from a search query.
#[derive(Debug, Clone)]
pub struct Hit {
    pub id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub distance: f32,
}

/// cozo handle for one bank. Opens the sqlite backend at
/// `<root>/<bank_id>/db.sqlite` on `open`, creating the relation and
/// HNSW index on first use. Cheap to clone the Arc-wrapped embedder
/// pool; the db itself is held by value.
pub struct MemoryStore {
    bank_id: String,
    embed_model: Option<String>,
    dim: usize,
    db: DbInstance,
    embedder: Arc<EmbedderPool>,
}

impl MemoryStore {
    /// Open (or create) the bank db. `root` is the fleet-level memory
    /// directory — typically `.aciddog/memory/`.
    pub fn open(
        bank: &MemoryBank,
        root: &Path,
        embedder: Arc<EmbedderPool>,
    ) -> Result<Self, MemoryError> {
        let bank_dir = root.join(&bank.id);
        std::fs::create_dir_all(&bank_dir).map_err(|e| MemoryError::Io {
            path: bank_dir.display().to_string(),
            source: e,
        })?;

        let db_path: PathBuf = bank_dir.join("db.sqlite");
        let db = DbInstance::new("sqlite", &db_path, "")
            .map_err(|e| MemoryError::Cozo(format!("opening {}: {e}", db_path.display())))?;

        let dim = embedder.dim(bank.embed_model.as_deref())?;
        ensure_schema(&db, dim)?;

        Ok(Self {
            bank_id: bank.id.clone(),
            embed_model: bank.embed_model.clone(),
            dim,
            db,
            embedder,
        })
    }

    pub fn bank_id(&self) -> &str {
        &self.bank_id
    }

    /// Embed and persist `body`. Returns the generated entry id so the
    /// caller can echo it back to the agent.
    pub fn store(&self, body: &str) -> Result<Uuid, MemoryError> {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return Err(MemoryError::EmptyBody);
        }

        let embedding = self.embedder.embed(self.embed_model.as_deref(), trimmed)?;
        if embedding.len() != self.dim {
            return Err(MemoryError::DimMismatch {
                expected: self.dim,
                actual: embedding.len(),
            });
        }

        let id = Uuid::new_v4();
        let ts = Utc::now().timestamp_millis();

        let mut params = BTreeMap::new();
        params.insert("id".into(), DataValue::Str(id.to_string().into()));
        params.insert("body".into(), DataValue::Str(trimmed.to_string().into()));
        params.insert("created_at".into(), DataValue::Num(Num::Int(ts)));
        params.insert("embedding".into(), list_of_floats(&embedding));

        self.db
            .run_script(
                "?[id, body, created_at, embedding] <- [[$id, $body, $created_at, $embedding]] \
                 :put entries {id => body, created_at, embedding}",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("store: {e}")))?;

        Ok(id)
    }

    /// Embed `query` and return the top `limit` hits ranked by vector
    /// distance. `limit` is clamped to at least 1.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>, MemoryError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Err(MemoryError::EmptyBody);
        }
        let k = limit.max(1);

        let embedding = self.embedder.embed(self.embed_model.as_deref(), trimmed)?;
        if embedding.len() != self.dim {
            return Err(MemoryError::DimMismatch {
                expected: self.dim,
                actual: embedding.len(),
            });
        }

        let mut params = BTreeMap::new();
        params.insert("q".into(), list_of_floats(&embedding));
        params.insert("k".into(), DataValue::Num(Num::Int(k as i64)));

        let rows = self
            .db
            .run_script(
                "?[dist, id, body, created_at] := \
                   q = vec($q), \
                   ~entries:vec{id, body, created_at | query: q, k: $k, \
                                ef: 32, bind_distance: dist}",
                params,
                ScriptMutability::Immutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("search: {e}")))?;

        let mut hits = Vec::with_capacity(rows.rows.len());
        for row in rows.rows {
            let dist = row
                .first()
                .and_then(as_f32)
                .ok_or_else(|| MemoryError::Cozo("search row missing distance".into()))?;
            let id = row
                .get(1)
                .and_then(as_str)
                .and_then(|s| Uuid::parse_str(&s).ok())
                .ok_or_else(|| MemoryError::Cozo("search row missing/invalid id".into()))?;
            let body = row
                .get(2)
                .and_then(as_str)
                .ok_or_else(|| MemoryError::Cozo("search row missing body".into()))?;
            let created_ms = row
                .get(3)
                .and_then(as_i64)
                .unwrap_or_default();
            let created_at = Utc
                .timestamp_millis_opt(created_ms)
                .single()
                .unwrap_or_else(Utc::now);
            hits.push(Hit {
                id,
                body,
                created_at,
                distance: dist,
            });
        }
        Ok(hits)
    }
}

/// Create the `entries` relation and its HNSW index if they don't
/// already exist. Safe to call on every open — checks the relation list
/// first and skips the DDL when both are present.
fn ensure_schema(db: &DbInstance, dim: usize) -> Result<(), MemoryError> {
    let rels = db
        .run_script(
            "::relations",
            BTreeMap::new(),
            ScriptMutability::Immutable,
        )
        .map_err(|e| MemoryError::Cozo(format!("list relations: {e}")))?;
    let has_entries = rels.rows.iter().any(|r| {
        r.first()
            .and_then(as_str)
            .map(|s| s == "entries")
            .unwrap_or(false)
    });
    let has_index = rels.rows.iter().any(|r| {
        r.first()
            .and_then(as_str)
            .map(|s| s == "entries:vec")
            .unwrap_or(false)
    });

    if !has_entries {
        let create = format!(
            ":create entries {{id: String => body: String, created_at: Int, embedding: <F32; {dim}>}}"
        );
        db.run_script(&create, BTreeMap::new(), ScriptMutability::Mutable)
            .map_err(|e| MemoryError::Cozo(format!("create entries: {e}")))?;
    }
    if !has_index {
        let create_idx = format!(
            "::hnsw create entries:vec {{dim: {dim}, dtype: F32, fields: [embedding], \
             distance: Cosine, ef_construction: 16, m: 32}}"
        );
        db.run_script(&create_idx, BTreeMap::new(), ScriptMutability::Mutable)
            .map_err(|e| MemoryError::Cozo(format!("create hnsw: {e}")))?;
    }
    Ok(())
}

fn list_of_floats(v: &[f32]) -> DataValue {
    DataValue::List(
        v.iter()
            .map(|x| DataValue::Num(Num::Float(*x as f64)))
            .collect(),
    )
}

fn as_str(v: &DataValue) -> Option<String> {
    match v {
        DataValue::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

fn as_f32(v: &DataValue) -> Option<f32> {
    match v {
        DataValue::Num(Num::Float(f)) => Some(*f as f32),
        DataValue::Num(Num::Int(i)) => Some(*i as f32),
        _ => None,
    }
}

fn as_i64(v: &DataValue) -> Option<i64> {
    match v {
        DataValue::Num(Num::Int(i)) => Some(*i),
        DataValue::Num(Num::Float(f)) => Some(*f as i64),
        _ => None,
    }
}
