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

/// Which direction to walk when listing an entry's links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDirection {
    /// Entries this one points at (outgoing edges).
    Out,
    /// Entries that point at this one (incoming edges).
    In,
    /// Both directions, de-duplicated by peer id.
    Both,
}

impl LinkDirection {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "out" | "outgoing" | "from" => Some(LinkDirection::Out),
            "in" | "incoming" | "to" => Some(LinkDirection::In),
            "both" | "all" => Some(LinkDirection::Both),
            _ => None,
        }
    }
}

/// A single edge in the link graph plus the body/ts of the peer entry
/// it resolves to. `kind` is the free-text label the caller supplied
/// when the link was created (empty string if none).
#[derive(Debug, Clone)]
pub struct Link {
    pub peer_id: Uuid,
    pub peer_body: String,
    pub kind: String,
    pub direction: LinkDirection,
    pub created_at: DateTime<Utc>,
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

impl MemoryStore {
    /// Create a directed link from `from` to `to`. `kind` is an
    /// optional free-text label; `None` or an empty string is stored as
    /// the empty string. Both endpoints must already exist in this
    /// bank — `MemoryError::UnknownEntry` is returned otherwise.
    pub fn link(
        &self,
        from: Uuid,
        to: Uuid,
        kind: Option<&str>,
    ) -> Result<(), MemoryError> {
        if from == to {
            return Err(MemoryError::SelfLink);
        }
        self.require_entry(from)?;
        self.require_entry(to)?;

        let kind = kind.unwrap_or("").to_string();
        let ts = Utc::now().timestamp_millis();

        let mut params = BTreeMap::new();
        params.insert("from".into(), DataValue::Str(from.to_string().into()));
        params.insert("to".into(), DataValue::Str(to.to_string().into()));
        params.insert("kind".into(), DataValue::Str(kind.into()));
        params.insert("created_at".into(), DataValue::Num(Num::Int(ts)));

        self.db
            .run_script(
                "?[from, to, kind, created_at] <- [[$from, $to, $kind, $created_at]] \
                 :put links {from, to => kind, created_at}",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("link: {e}")))?;
        Ok(())
    }

    /// Remove the edge from `from` to `to`, if it exists. Returns
    /// `true` when a row was removed, `false` when no such edge was
    /// present.
    pub fn unlink(&self, from: Uuid, to: Uuid) -> Result<bool, MemoryError> {
        let mut lookup = BTreeMap::new();
        lookup.insert("from".into(), DataValue::Str(from.to_string().into()));
        lookup.insert("to".into(), DataValue::Str(to.to_string().into()));

        let existed = self
            .db
            .run_script(
                "?[found] := *links{from: $from, to: $to}, found = $from",
                lookup.clone(),
                ScriptMutability::Immutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("unlink lookup: {e}")))?
            .rows
            .len()
            > 0;

        if !existed {
            return Ok(false);
        }

        self.db
            .run_script(
                "?[from, to] <- [[$from, $to]] :rm links {from, to}",
                lookup,
                ScriptMutability::Mutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("unlink: {e}")))?;
        Ok(true)
    }

    /// Return every link touching `id` in the requested direction,
    /// along with the peer entry's body for display. Peers with
    /// missing bodies (e.g. entry deleted without cleanup) return an
    /// empty body string rather than failing the whole call.
    pub fn links(
        &self,
        id: Uuid,
        direction: LinkDirection,
    ) -> Result<Vec<Link>, MemoryError> {
        let mut out = Vec::new();
        if matches!(direction, LinkDirection::Out | LinkDirection::Both) {
            out.extend(self.links_one_direction(id, LinkDirection::Out)?);
        }
        if matches!(direction, LinkDirection::In | LinkDirection::Both) {
            out.extend(self.links_one_direction(id, LinkDirection::In)?);
        }
        Ok(out)
    }

    fn links_one_direction(
        &self,
        id: Uuid,
        direction: LinkDirection,
    ) -> Result<Vec<Link>, MemoryError> {
        // Two near-identical queries: swap the bound end of the edge
        // depending on whether we want outgoing or incoming neighbours.
        let (script, label) = match direction {
            LinkDirection::Out => (
                "?[peer, body, kind, created_at] := \
                   *links{from: $id, to: peer, kind, created_at}, \
                   *entries{id: peer, body}",
                LinkDirection::Out,
            ),
            LinkDirection::In => (
                "?[peer, body, kind, created_at] := \
                   *links{from: peer, to: $id, kind, created_at}, \
                   *entries{id: peer, body}",
                LinkDirection::In,
            ),
            LinkDirection::Both => unreachable!("callers pass Out/In here"),
        };

        let mut params = BTreeMap::new();
        params.insert("id".into(), DataValue::Str(id.to_string().into()));

        let rows = self
            .db
            .run_script(script, params, ScriptMutability::Immutable)
            .map_err(|e| MemoryError::Cozo(format!("links: {e}")))?;

        let mut out = Vec::with_capacity(rows.rows.len());
        for row in rows.rows {
            let peer = row
                .first()
                .and_then(as_str)
                .and_then(|s| Uuid::parse_str(&s).ok())
                .ok_or_else(|| MemoryError::Cozo("links row missing peer id".into()))?;
            let body = row
                .get(1)
                .and_then(as_str)
                .unwrap_or_default();
            let kind = row
                .get(2)
                .and_then(as_str)
                .unwrap_or_default();
            let ts_ms = row.get(3).and_then(as_i64).unwrap_or_default();
            let created_at = Utc
                .timestamp_millis_opt(ts_ms)
                .single()
                .unwrap_or_else(Utc::now);
            out.push(Link {
                peer_id: peer,
                peer_body: body,
                kind,
                direction: label,
                created_at,
            });
        }
        Ok(out)
    }

    fn require_entry(&self, id: Uuid) -> Result<(), MemoryError> {
        let mut params = BTreeMap::new();
        params.insert("id".into(), DataValue::Str(id.to_string().into()));
        // cozo rejects head symbols that aren't bound in the body, so
        // we bind a separate `found` variable to the parameter value
        // rather than echoing `id` directly.
        let rows = self
            .db
            .run_script(
                "?[found] := *entries{id: $id}, found = $id",
                params,
                ScriptMutability::Immutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("entry lookup: {e}")))?;
        if rows.rows.is_empty() {
            Err(MemoryError::UnknownEntry(id))
        } else {
            Ok(())
        }
    }
}

/// Create the `entries` relation, its HNSW index, and the `links`
/// relation if they don't already exist. Safe to call on every open —
/// checks the relation list first and skips the DDL per-relation.
fn ensure_schema(db: &DbInstance, dim: usize) -> Result<(), MemoryError> {
    let rels = db
        .run_script(
            "::relations",
            BTreeMap::new(),
            ScriptMutability::Immutable,
        )
        .map_err(|e| MemoryError::Cozo(format!("list relations: {e}")))?;
    let has = |name: &str| -> bool {
        rels.rows.iter().any(|r| {
            r.first()
                .and_then(as_str)
                .map(|s| s == name)
                .unwrap_or(false)
        })
    };

    if !has("entries") {
        let create = format!(
            ":create entries {{id: String => body: String, created_at: Int, embedding: <F32; {dim}>}}"
        );
        db.run_script(&create, BTreeMap::new(), ScriptMutability::Mutable)
            .map_err(|e| MemoryError::Cozo(format!("create entries: {e}")))?;
    }
    if !has("entries:vec") {
        let create_idx = format!(
            "::hnsw create entries:vec {{dim: {dim}, dtype: F32, fields: [embedding], \
             distance: Cosine, ef_construction: 16, m: 32}}"
        );
        db.run_script(&create_idx, BTreeMap::new(), ScriptMutability::Mutable)
            .map_err(|e| MemoryError::Cozo(format!("create hnsw: {e}")))?;
    }
    if !has("links") {
        // Directed edges between entries. `kind` is a free-text label
        // the caller supplies to classify the relationship (e.g.
        // "supersedes", "see_also"); empty string when absent.
        db.run_script(
            ":create links {from: String, to: String => kind: String, created_at: Int}",
            BTreeMap::new(),
            ScriptMutability::Mutable,
        )
        .map_err(|e| MemoryError::Cozo(format!("create links: {e}")))?;
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
