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
pub use store::{Hit, MemoryStore};

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

    /// Embed `body` and persist it in `bank`. Returns the new entry id.
    /// Callers must have already verified that the requesting actor is
    /// allowed to touch `bank` — the registry does not enforce that.
    pub fn store(&self, bank: &MemoryBank, body: &str) -> Result<uuid::Uuid, MemoryError> {
        let store = self.open_bank(bank)?;
        store.store(body)
    }

    /// Embed `query` and return the top `limit` hits in `bank`. Same
    /// access-control note as [`MemoryRegistry::store`].
    pub fn search(
        &self,
        bank: &MemoryBank,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Hit>, MemoryError> {
        let store = self.open_bank(bank)?;
        store.search(query, limit)
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
