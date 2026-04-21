//! Memory banks: declarative vector-search stores shared across a
//! team's agents and workers. The registry is an open-shop service —
//! `store` and `search` take a [`MemoryBank`] directly and do no
//! access-control check, so callers (the IPC handler) must gate on
//! [`AciddogConfig::can_access_bank`] before dispatching here. This
//! keeps the registry decoupled from the full config.
//!
//! Physical layout: one cozo database per bank at
//! `<root>/<bank_id>/db.sqlite`, with the fastembed model cache at
//! `<root>/_models/`. `<root>` is typically `.aciddog/memory/`.

pub mod embedder;
pub mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::config::MemoryBank;

pub use embedder::{EmbedderPool, DEFAULT_EMBED_MODEL};
pub use store::{Hit, Link, LinkDirection, MemoryStore, SearchOptions, TagFilter};

/// Standard sub-path for the model cache, sibling to per-bank dbs.
const MODEL_CACHE_SUBDIR: &str = "_models";

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("cozo error: {0}")]
    Cozo(String),

    #[error("embed error: {0}")]
    Embed(String),

    #[error("unknown embed model `{0}`")]
    UnknownModel(String),

    #[error("body is empty")]
    EmptyBody,

    #[error("embedding dim mismatch: expected {expected}, got {actual}")]
    DimMismatch { expected: usize, actual: usize },

    #[error("unknown memory entry `{0}`")]
    UnknownEntry(uuid::Uuid),

    #[error("cannot link an entry to itself")]
    SelfLink,
}

/// Top-level handle wired onto the App. Holds the fleet root path, the
/// shared embedder pool, and a lazy cache of per-bank stores keyed by
/// bank id.
pub struct MemoryRegistry {
    root: PathBuf,
    embedder: Arc<EmbedderPool>,
    stores: Mutex<HashMap<String, Arc<MemoryStore>>>,
}

impl MemoryRegistry {
    /// Build a registry rooted at `root` (e.g. `.aciddog/memory/`).
    /// Does not open any bank dbs — banks are lazily opened on first
    /// store/search.
    pub fn new(root: PathBuf) -> Self {
        let cache_dir = root.join(MODEL_CACHE_SUBDIR);
        Self {
            root,
            embedder: Arc::new(EmbedderPool::new(cache_dir)),
            stores: Mutex::new(HashMap::new()),
        }
    }

    /// Embed `body` and persist it in `bank` with the given `title`
    /// (optional) and `tags` (possibly empty). Returns the new entry
    /// id. Callers must have already verified that the requesting
    /// actor is allowed to touch `bank` — the registry does not enforce
    /// that.
    pub fn store(
        &self,
        bank: &MemoryBank,
        body: &str,
        title: Option<&str>,
        tags: &[String],
    ) -> Result<uuid::Uuid, MemoryError> {
        let store = self.open_bank(bank)?;
        store.store(body, title, tags)
    }

    /// Run a search with the combined filters in `opts`. Same
    /// access-control note as [`MemoryRegistry::store`].
    pub fn search(
        &self,
        bank: &MemoryBank,
        opts: SearchOptions,
    ) -> Result<Vec<Hit>, MemoryError> {
        let store = self.open_bank(bank)?;
        store.search(opts)
    }

    /// Create a directed link between two entries in `bank`.
    pub fn link(
        &self,
        bank: &MemoryBank,
        from: uuid::Uuid,
        to: uuid::Uuid,
        kind: Option<&str>,
    ) -> Result<(), MemoryError> {
        let store = self.open_bank(bank)?;
        store.link(from, to, kind)
    }

    /// Remove a directed link between two entries in `bank`. Returns
    /// `true` when the edge existed.
    pub fn unlink(
        &self,
        bank: &MemoryBank,
        from: uuid::Uuid,
        to: uuid::Uuid,
    ) -> Result<bool, MemoryError> {
        let store = self.open_bank(bank)?;
        store.unlink(from, to)
    }

    /// List every link touching `id` in the requested direction.
    /// `tag_filter` restricts results to peers matching the include /
    /// exclude tag sets.
    pub fn links(
        &self,
        bank: &MemoryBank,
        id: uuid::Uuid,
        direction: LinkDirection,
        tag_filter: &TagFilter,
    ) -> Result<Vec<Link>, MemoryError> {
        let store = self.open_bank(bank)?;
        store.links(id, direction, tag_filter)
    }

    /// Pre-open every bank in `banks` so the first real call doesn't
    /// stall on directory creation or model download. Errors short-
    /// circuit on the first failure; earlier successes remain cached.
    pub fn warm(&self, banks: &[MemoryBank]) -> Result<(), MemoryError> {
        for bank in banks {
            self.open_bank(bank)?;
        }
        Ok(())
    }

    fn open_bank(&self, bank: &MemoryBank) -> Result<Arc<MemoryStore>, MemoryError> {
        {
            let guard = self.stores.lock().expect("memory store map poisoned");
            if let Some(store) = guard.get(&bank.id) {
                return Ok(Arc::clone(store));
            }
        }

        let store = MemoryStore::open(bank, &self.root, Arc::clone(&self.embedder))?;
        let arc = Arc::new(store);

        let mut guard = self.stores.lock().expect("memory store map poisoned");
        Ok(Arc::clone(
            guard
                .entry(bank.id.clone())
                .or_insert_with(|| arc.clone()),
        ))
    }
}
