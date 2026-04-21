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

#[test]
#[ignore]
fn links_roundtrip_between_two_entries() {
    use aciddog::memory::LinkDirection;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let bank = bank_default();
    let store = MemoryStore::open(&bank, &root, embedder).expect("open");

    let old_decision = store
        .store("We used to require two approvals on every PR.")
        .expect("store old");
    let new_decision = store
        .store("Only one approval is now required on PRs under 50 lines.")
        .expect("store new");

    // new_decision supersedes old_decision.
    store
        .link(new_decision, old_decision, Some("supersedes"))
        .expect("link");

    // Outgoing from new: should surface old as the peer with kind=supersedes.
    let out = store.links(new_decision, LinkDirection::Out).expect("out");
    assert_eq!(out.len(), 1, "expected one outgoing link, got {out:?}");
    assert_eq!(out[0].peer_id, old_decision);
    assert_eq!(out[0].kind, "supersedes");

    // Incoming to old: should surface new as the peer.
    let inc = store.links(old_decision, LinkDirection::In).expect("in");
    assert_eq!(inc.len(), 1, "expected one incoming link, got {inc:?}");
    assert_eq!(inc[0].peer_id, new_decision);

    // Both on new covers out-only here.
    let both = store.links(new_decision, LinkDirection::Both).expect("both");
    assert_eq!(both.len(), 1);

    // Unlink and confirm idempotency on the second call.
    assert!(store.unlink(new_decision, old_decision).unwrap(), "removed");
    assert!(!store.unlink(new_decision, old_decision).unwrap(), "no-op");
    let out = store.links(new_decision, LinkDirection::Out).expect("out2");
    assert!(out.is_empty(), "expected no links after unlink");
}

#[test]
#[ignore]
fn link_self_and_unknown_entry_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let bank = bank_default();
    let store = MemoryStore::open(&bank, &root, embedder).expect("open");

    let real = store.store("a real entry").expect("store");

    let self_err = store.link(real, real, None).expect_err("self-link");
    assert!(self_err.to_string().to_lowercase().contains("itself"));

    let ghost = uuid::Uuid::new_v4();
    let missing_err = store.link(real, ghost, None).expect_err("missing to");
    assert!(
        missing_err.to_string().to_lowercase().contains("unknown"),
        "got {missing_err}"
    );
}
