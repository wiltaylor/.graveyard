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

/// One stored memory entry, its title + tags, and its distance from
/// a search query. `distance` is [`f32::NAN`] for non-semantic searches
/// (pure title/tag filtering) since there's no embedding to compare
/// against.
#[derive(Debug, Clone)]
pub struct Hit {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
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

/// A single edge in the link graph plus the body/title/tags of the
/// peer entry it resolves to. `kind` is the free-text label the caller
/// supplied when the link was created (empty string if none).
#[derive(Debug, Clone)]
pub struct Link {
    pub peer_id: Uuid,
    pub peer_title: String,
    pub peer_body: String,
    pub peer_tags: Vec<String>,
    pub kind: String,
    pub direction: LinkDirection,
    pub created_at: DateTime<Utc>,
}

/// Filter applied on top of a search or link traversal. Entries must
/// carry all of `include_tags` and none of `exclude_tags`. Empty
/// vectors disable each side of the filter.
#[derive(Debug, Clone, Default)]
pub struct TagFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl TagFilter {
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}

/// Everything `search` needs to know. Any subset may be empty: an empty
/// `SearchOptions` returns all entries (oldest first) up to `limit`.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Natural-language query text. When `Some`, the search runs through
    /// the HNSW index; when `None`, results fall back to a plain
    /// relational scan ordered by `created_at` descending.
    pub query: Option<String>,
    /// Case-insensitive substring the title must contain. Applied after
    /// the main query (and, for semantic search, after HNSW ranking).
    pub title_contains: Option<String>,
    pub tags: TagFilter,
    /// Maximum number of hits to return. Clamped to at least 1.
    pub limit: usize,
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

    /// Embed and persist `body`. `title` is an optional short label
    /// searchable via `SearchOptions::title_contains`; `tags` is a
    /// free-text label set used for include/exclude filtering. Tags
    /// are normalised (lower-cased and trimmed) and de-duplicated
    /// before storage.
    pub fn store(
        &self,
        body: &str,
        title: Option<&str>,
        tags: &[String],
    ) -> Result<Uuid, MemoryError> {
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
        let title = title.map(str::trim).unwrap_or("").to_string();
        let tags = normalise_tags(tags);

        let mut params = BTreeMap::new();
        params.insert("id".into(), DataValue::Str(id.to_string().into()));
        params.insert("title".into(), DataValue::Str(title.into()));
        params.insert("body".into(), DataValue::Str(trimmed.to_string().into()));
        params.insert("created_at".into(), DataValue::Num(Num::Int(ts)));
        params.insert("embedding".into(), list_of_floats(&embedding));
        self.db
            .run_script(
                "?[id, title, body, created_at, embedding] <- \
                   [[$id, $title, $body, $created_at, $embedding]] \
                 :put entries {id => title, body, created_at, embedding}",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("store entries: {e}")))?;

        // entry_tags: one row per tag. Skip silently when empty.
        for tag in &tags {
            let mut tag_params = BTreeMap::new();
            tag_params.insert("entry".into(), DataValue::Str(id.to_string().into()));
            tag_params.insert("tag".into(), DataValue::Str(tag.clone().into()));
            tag_params.insert("created_at".into(), DataValue::Num(Num::Int(ts)));
            self.db
                .run_script(
                    "?[entry, tag, created_at] <- [[$entry, $tag, $created_at]] \
                     :put entry_tags {entry, tag => created_at}",
                    tag_params,
                    ScriptMutability::Mutable,
                )
                .map_err(|e| MemoryError::Cozo(format!("store entry_tags: {e}")))?;
        }

        Ok(id)
    }

    /// Run a search against this bank with the combined set of filters.
    /// Returns entries ordered by (for semantic search) vector distance
    /// ascending, or (for non-semantic) `created_at` descending. Tag
    /// filters are applied at query time; the title substring filter
    /// and the final `limit` truncation happen in Rust so the
    /// datalog stays small.
    pub fn search(&self, opts: SearchOptions) -> Result<Vec<Hit>, MemoryError> {
        let limit = opts.limit.max(1);
        // Over-fetch so title filtering in Rust still yields enough hits
        // to fill `limit`. Small banks pay nothing; huge banks cap at
        // 4× + 20 so we never unbounded-scan.
        let fetch_cap = limit.saturating_mul(4).saturating_add(20);

        let candidates = if let Some(query) = opts
            .query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.semantic_candidates(query, fetch_cap, &opts.tags)?
        } else {
            self.relational_candidates(fetch_cap, &opts.tags)?
        };

        let needle = opts
            .title_contains
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase);

        let mut out = Vec::with_capacity(limit.min(candidates.len()));
        for (id, dist, title, body, created_at) in candidates {
            if let Some(needle) = &needle {
                if !title.to_ascii_lowercase().contains(needle) {
                    continue;
                }
            }
            let tags = self.load_tags(id)?;
            out.push(Hit {
                id,
                title,
                body,
                tags,
                created_at,
                distance: dist,
            });
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// HNSW candidate fetch: up to `fetch_cap` nearest hits with tag
    /// filters already applied. Returns `(id, distance, title, body,
    /// created_at)` tuples in cozo row order (distance ascending).
    fn semantic_candidates(
        &self,
        query: &str,
        fetch_cap: usize,
        tags: &TagFilter,
    ) -> Result<Vec<(Uuid, f32, String, String, DateTime<Utc>)>, MemoryError> {
        let embedding = self.embedder.embed(self.embed_model.as_deref(), query)?;
        if embedding.len() != self.dim {
            return Err(MemoryError::DimMismatch {
                expected: self.dim,
                actual: embedding.len(),
            });
        }

        let mut params = BTreeMap::new();
        params.insert("q".into(), list_of_floats(&embedding));
        params.insert("k".into(), DataValue::Num(Num::Int(fetch_cap as i64)));
        let tag_clauses = build_tag_clauses(&mut params, tags, "id");

        let script = format!(
            "?[dist, id, title, body, created_at] := \
               q = vec($q), \
               ~entries:vec{{id, title, body, created_at | query: q, k: $k, \
                             ef: 32, bind_distance: dist}}{tag_clauses}"
        );

        let rows = self
            .db
            .run_script(&script, params, ScriptMutability::Immutable)
            .map_err(|e| MemoryError::Cozo(format!("search: {e}")))?;

        let mut out = Vec::with_capacity(rows.rows.len());
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
            let title = row
                .get(2)
                .and_then(as_str)
                .unwrap_or_default();
            let body = row
                .get(3)
                .and_then(as_str)
                .ok_or_else(|| MemoryError::Cozo("search row missing body".into()))?;
            let created_at = row
                .get(4)
                .and_then(as_i64)
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
                .unwrap_or_else(Utc::now);
            out.push((id, dist, title, body, created_at));
        }
        Ok(out)
    }

    /// Non-semantic candidate fetch: every entry (honouring tag filters),
    /// newest first, capped at `fetch_cap`. Distance is [`f32::NAN`].
    fn relational_candidates(
        &self,
        fetch_cap: usize,
        tags: &TagFilter,
    ) -> Result<Vec<(Uuid, f32, String, String, DateTime<Utc>)>, MemoryError> {
        let mut params = BTreeMap::new();
        let tag_clauses = build_tag_clauses(&mut params, tags, "id");

        let script = format!(
            "?[id, title, body, created_at] := \
               *entries{{id, title, body, created_at}}{tag_clauses} \
             :order -created_at \
             :limit {fetch_cap}"
        );

        let rows = self
            .db
            .run_script(&script, params, ScriptMutability::Immutable)
            .map_err(|e| MemoryError::Cozo(format!("list: {e}")))?;

        let mut out = Vec::with_capacity(rows.rows.len());
        for row in rows.rows {
            let id = row
                .first()
                .and_then(as_str)
                .and_then(|s| Uuid::parse_str(&s).ok())
                .ok_or_else(|| MemoryError::Cozo("list row missing/invalid id".into()))?;
            let title = row
                .get(1)
                .and_then(as_str)
                .unwrap_or_default();
            let body = row
                .get(2)
                .and_then(as_str)
                .ok_or_else(|| MemoryError::Cozo("list row missing body".into()))?;
            let created_at = row
                .get(3)
                .and_then(as_i64)
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
                .unwrap_or_else(Utc::now);
            out.push((id, f32::NAN, title, body, created_at));
        }
        Ok(out)
    }

    fn load_tags(&self, id: Uuid) -> Result<Vec<String>, MemoryError> {
        let mut params = BTreeMap::new();
        params.insert("id".into(), DataValue::Str(id.to_string().into()));
        let rows = self
            .db
            .run_script(
                "?[tag] := *entry_tags{entry: $id, tag}",
                params,
                ScriptMutability::Immutable,
            )
            .map_err(|e| MemoryError::Cozo(format!("load tags: {e}")))?;
        let mut tags: Vec<String> = rows
            .rows
            .into_iter()
            .filter_map(|row| row.into_iter().next().as_ref().and_then(as_str))
            .collect();
        tags.sort();
        Ok(tags)
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

    /// Return every link touching `id` in the requested direction.
    /// `tag_filter` restricts results to peers carrying all of
    /// `include` and none of `exclude`; an empty filter is a no-op.
    /// Peers with missing bodies (e.g. entry deleted without cleanup)
    /// surface as empty strings rather than failing the whole call.
    pub fn links(
        &self,
        id: Uuid,
        direction: LinkDirection,
        tag_filter: &TagFilter,
    ) -> Result<Vec<Link>, MemoryError> {
        let mut out = Vec::new();
        if matches!(direction, LinkDirection::Out | LinkDirection::Both) {
            out.extend(self.links_one_direction(id, LinkDirection::Out, tag_filter)?);
        }
        if matches!(direction, LinkDirection::In | LinkDirection::Both) {
            out.extend(self.links_one_direction(id, LinkDirection::In, tag_filter)?);
        }
        Ok(out)
    }

    fn links_one_direction(
        &self,
        id: Uuid,
        direction: LinkDirection,
        tag_filter: &TagFilter,
    ) -> Result<Vec<Link>, MemoryError> {
        let mut params = BTreeMap::new();
        params.insert("id".into(), DataValue::Str(id.to_string().into()));
        let tag_clauses = build_tag_clauses(&mut params, tag_filter, "peer");

        // Two near-identical queries: swap the bound end of the edge
        // depending on whether we want outgoing or incoming neighbours.
        let base = match direction {
            LinkDirection::Out => {
                "*links{from: $id, to: peer, kind, created_at}, \
                 *entries{id: peer, title, body}"
            }
            LinkDirection::In => {
                "*links{from: peer, to: $id, kind, created_at}, \
                 *entries{id: peer, title, body}"
            }
            LinkDirection::Both => unreachable!("callers pass Out/In here"),
        };
        let script = format!("?[peer, title, body, kind, created_at] := {base}{tag_clauses}");

        let rows = self
            .db
            .run_script(&script, params, ScriptMutability::Immutable)
            .map_err(|e| MemoryError::Cozo(format!("links: {e}")))?;

        let mut out = Vec::with_capacity(rows.rows.len());
        for row in rows.rows {
            let peer = row
                .first()
                .and_then(as_str)
                .and_then(|s| Uuid::parse_str(&s).ok())
                .ok_or_else(|| MemoryError::Cozo("links row missing peer id".into()))?;
            let title = row
                .get(1)
                .and_then(as_str)
                .unwrap_or_default();
            let body = row
                .get(2)
                .and_then(as_str)
                .unwrap_or_default();
            let kind = row
                .get(3)
                .and_then(as_str)
                .unwrap_or_default();
            let ts_ms = row.get(4).and_then(as_i64).unwrap_or_default();
            let created_at = Utc
                .timestamp_millis_opt(ts_ms)
                .single()
                .unwrap_or_else(Utc::now);
            let peer_tags = self.load_tags(peer)?;
            out.push(Link {
                peer_id: peer,
                peer_title: title,
                peer_body: body,
                peer_tags,
                kind,
                direction,
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
            ":create entries {{id: String => title: String, body: String, \
             created_at: Int, embedding: <F32; {dim}>}}"
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
    if !has("entry_tags") {
        // One row per (entry, tag) pair. Composite-key design lets us
        // intersect (`include`) and negate (`exclude`) tags directly in
        // cozo datalog without smuggling lists through the query layer.
        db.run_script(
            ":create entry_tags {entry: String, tag: String => created_at: Int}",
            BTreeMap::new(),
            ScriptMutability::Mutable,
        )
        .map_err(|e| MemoryError::Cozo(format!("create entry_tags: {e}")))?;
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

/// Canonicalise caller-supplied tags: trim, lower-case, drop empties
/// and duplicates. All tag matching (store, include, exclude) routes
/// through this so queries can rely on normalised comparisons.
pub(crate) fn normalise_tag(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn normalise_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tags
        .iter()
        .map(|t| normalise_tag(t))
        .filter(|t| !t.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Build cozo datalog body fragments (including the leading `, ` each
/// fragment needs when appended after an existing body clause) that
/// enforce `filter` against `entry_tags` rows keyed by `entry_var`.
/// Include tags become intersection clauses; exclude tags become
/// negated clauses. Parameter names are synthesised (`$inc_tag_0`, …)
/// and written into `params`.
fn build_tag_clauses(
    params: &mut BTreeMap<String, DataValue>,
    filter: &TagFilter,
    entry_var: &str,
) -> String {
    let mut out = String::new();
    let include = normalise_tags(&filter.include);
    let exclude = normalise_tags(&filter.exclude);
    for (i, tag) in include.iter().enumerate() {
        let key = format!("inc_tag_{i}");
        params.insert(key.clone(), DataValue::Str(tag.clone().into()));
        out.push_str(&format!(
            ", *entry_tags{{entry: {entry_var}, tag: ${key}}}"
        ));
    }
    for (i, tag) in exclude.iter().enumerate() {
        let key = format!("exc_tag_{i}");
        params.insert(key.clone(), DataValue::Str(tag.clone().into()));
        out.push_str(&format!(
            ", not *entry_tags{{entry: {entry_var}, tag: ${key}}}"
        ));
    }
    out
}
