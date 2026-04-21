//! End-to-end tests for the memory subsystem: open a bank against a
//! temp directory, store a few entries, search, verify ranking. These
//! hit the real fastembed pipeline — the first run downloads ~30MB of
//! ONNX weights into the tempdir — so they are marked `#[ignore]` and
//! are only exercised with `cargo test -- --ignored`.

use std::sync::Arc;

use aciddog::config::MemoryBank;
use aciddog::memory::{EmbedderPool, MemoryStore};

fn bank_default() -> MemoryBank {
    MemoryBank {
        id: "test_bank".into(),
        description: "integration test bank".into(),
        embed_model: None,
    }
}

#[test]
#[ignore]
fn store_and_search_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let bank = bank_default();
    let store = MemoryStore::open(&bank, &root, embedder).expect("open");

    let rust_id = store
        .store("We settled on tokio 1.x as the async runtime for this project.")
        .expect("store tokio");
    let _db_id = store
        .store("The billing module uses Stripe for payment processing.")
        .expect("store billing");
    let _docs_id = store
        .store("All public documentation lives under /docs in the repo.")
        .expect("store docs");

    let hits = store
        .search("what async runtime do we use?", 3)
        .expect("search");
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(
        hits[0].id, rust_id,
        "tokio entry should rank first, got {:?}",
        hits[0]
    );
}

#[test]
#[ignore]
fn reopen_persists_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let bank = bank_default();

    {
        let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
        let store = MemoryStore::open(&bank, &root, embedder).expect("open 1");
        store
            .store("The CI pipeline is defined in .github/workflows/ci.yml.")
            .expect("store");
    }

    // Second open against the same root: the sqlite file and HNSW index
    // are reused, so the previously stored entry should be searchable.
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let store = MemoryStore::open(&bank, &root, embedder).expect("open 2");
    let hits = store.search("where is the CI config?", 1).expect("search");
    assert!(!hits.is_empty(), "entry should persist across reopen");
    assert!(
        hits[0].body.contains("ci.yml"),
        "unexpected top hit: {:?}",
        hits[0]
    );
}

#[test]
fn empty_body_is_rejected_without_embedding() {
    // This test doesn't need a real model — `store("")` should bail
    // before hitting fastembed. A bank file under the tempdir is
    // required so `open` succeeds; that step doesn't download the model
    // either (embedder load is lazy).
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let bank = bank_default();
    let store = MemoryStore::open(&bank, &root, embedder).expect("open");

    let err = store.store("   ").expect_err("empty body should fail");
    assert!(err.to_string().contains("empty"), "got {err}");
}
