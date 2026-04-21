//! End-to-end tests for the memory subsystem: open a bank against a
//! temp directory, store a few entries, search, verify ranking. These
//! hit the real fastembed pipeline — the first run downloads ~30MB of
//! ONNX weights into the tempdir — so they are marked `#[ignore]` and
//! are only exercised with `cargo test -- --ignored`.

use std::sync::Arc;

use aciddog::config::MemoryBank;
use aciddog::memory::{EmbedderPool, LinkDirection, MemoryStore, SearchOptions, TagFilter};

fn bank_default() -> MemoryBank {
    MemoryBank {
        id: "test_bank".into(),
        description: "integration test bank".into(),
        embed_model: None,
    }
}

fn open_tmp() -> (tempfile::TempDir, MemoryStore) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let bank = bank_default();
    let store = MemoryStore::open(&bank, &root, embedder).expect("open");
    (tmp, store)
}

fn opts_query(q: &str, limit: usize) -> SearchOptions {
    SearchOptions {
        query: Some(q.to_string()),
        limit,
        ..SearchOptions::default()
    }
}

#[test]
#[ignore]
fn store_and_search_roundtrip() {
    let (_tmp, store) = open_tmp();

    let rust_id = store
        .store(
            "We settled on tokio 1.x as the async runtime for this project.",
            None,
            &[],
        )
        .expect("store tokio");
    let _db_id = store
        .store(
            "The billing module uses Stripe for payment processing.",
            None,
            &[],
        )
        .expect("store billing");
    let _docs_id = store
        .store(
            "All public documentation lives under /docs in the repo.",
            None,
            &[],
        )
        .expect("store docs");

    let hits = store
        .search(opts_query("what async runtime do we use?", 3))
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
            .store(
                "The CI pipeline is defined in .github/workflows/ci.yml.",
                None,
                &[],
            )
            .expect("store");
    }

    // Second open against the same root: the sqlite file and HNSW index
    // are reused, so the previously stored entry should be searchable.
    let embedder = Arc::new(EmbedderPool::new(root.join("_models")));
    let store = MemoryStore::open(&bank, &root, embedder).expect("open 2");
    let hits = store
        .search(opts_query("where is the CI config?", 1))
        .expect("search");
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

    let err = store
        .store("   ", None, &[])
        .expect_err("empty body should fail");
    assert!(err.to_string().contains("empty"), "got {err}");
}

#[test]
#[ignore]
fn title_and_tags_roundtrip_and_are_returned_on_hit() {
    let (_tmp, store) = open_tmp();

    let id = store
        .store(
            "Run migrations with `just db-migrate` before deploying.",
            Some("db migrations"),
            &["deploy".into(), "DB".into(), "db".into()], // dedup + case
        )
        .expect("store");

    let hits = store
        .search(opts_query("how do I run migrations", 3))
        .expect("search");
    let hit = hits.iter().find(|h| h.id == id).expect("hit present");
    assert_eq!(hit.title, "db migrations");
    // Tags are normalised (lower-cased, dedup'd, sorted).
    assert_eq!(hit.tags, vec!["db".to_string(), "deploy".to_string()]);
}

#[test]
#[ignore]
fn search_tag_filters_include_and_exclude() {
    let (_tmp, store) = open_tmp();

    let urgent = store
        .store(
            "Fix the OAuth refresh bug in auth/refresh.rs.",
            Some("oauth bug"),
            &["urgent".into(), "auth".into()],
        )
        .expect("urgent");
    let done = store
        .store(
            "OAuth scope expansion shipped last sprint — no open work here.",
            Some("oauth scope done"),
            &["auth".into(), "done".into()],
        )
        .expect("done");
    let _unrelated = store
        .store(
            "Our CSS uses Tailwind utility classes across the board.",
            Some("styling"),
            &["frontend".into()],
        )
        .expect("unrelated");

    // Include `auth`, no exclude: should surface both auth entries.
    let opts = SearchOptions {
        query: Some("oauth".into()),
        tags: TagFilter {
            include: vec!["auth".into()],
            exclude: vec![],
        },
        limit: 10,
        ..SearchOptions::default()
    };
    let hits = store.search(opts).expect("auth");
    let ids: Vec<_> = hits.iter().map(|h| h.id).collect();
    assert!(ids.contains(&urgent) && ids.contains(&done), "{ids:?}");

    // Now exclude `done`: should drop the shipped entry.
    let opts = SearchOptions {
        query: Some("oauth".into()),
        tags: TagFilter {
            include: vec!["auth".into()],
            exclude: vec!["done".into()],
        },
        limit: 10,
        ..SearchOptions::default()
    };
    let hits = store.search(opts).expect("auth !done");
    let ids: Vec<_> = hits.iter().map(|h| h.id).collect();
    assert!(ids.contains(&urgent), "urgent missing: {ids:?}");
    assert!(!ids.contains(&done), "done leaked through: {ids:?}");
}

#[test]
#[ignore]
fn search_without_query_lists_entries_filtered_by_tags() {
    let (_tmp, store) = open_tmp();

    let a = store
        .store("entry A", None, &["x".into()]).expect("a");
    let _b = store
        .store("entry B", None, &["y".into()]).expect("b");

    // No --query, include-tag x: only A.
    let opts = SearchOptions {
        tags: TagFilter {
            include: vec!["x".into()],
            exclude: vec![],
        },
        limit: 10,
        ..SearchOptions::default()
    };
    let hits = store.search(opts).expect("relational");
    let ids: Vec<_> = hits.iter().map(|h| h.id).collect();
    assert_eq!(ids, vec![a]);
}

#[test]
#[ignore]
fn search_title_contains_filters_after_query() {
    let (_tmp, store) = open_tmp();

    let target = store
        .store(
            "The billing module uses Stripe.",
            Some("billing integration notes"),
            &[],
        )
        .expect("billing");
    let _other = store
        .store(
            "We use PostgreSQL as the primary database.",
            Some("database stack"),
            &[],
        )
        .expect("db");

    let opts = SearchOptions {
        query: Some("billing".into()),
        title_contains: Some("BILLING".into()),
        limit: 5,
        ..SearchOptions::default()
    };
    let hits = store.search(opts).expect("filtered");
    let ids: Vec<_> = hits.iter().map(|h| h.id).collect();
    assert_eq!(ids, vec![target], "{hits:?}");
}

#[test]
#[ignore]
fn links_roundtrip_between_two_entries() {
    let (_tmp, store) = open_tmp();

    let old_decision = store
        .store(
            "We used to require two approvals on every PR.",
            None,
            &[],
        )
        .expect("store old");
    let new_decision = store
        .store(
            "Only one approval is now required on PRs under 50 lines.",
            None,
            &[],
        )
        .expect("store new");

    // new_decision supersedes old_decision.
    store
        .link(new_decision, old_decision, Some("supersedes"))
        .expect("link");

    let empty = TagFilter::default();
    let out = store
        .links(new_decision, LinkDirection::Out, &empty)
        .expect("out");
    assert_eq!(out.len(), 1, "expected one outgoing link, got {out:?}");
    assert_eq!(out[0].peer_id, old_decision);
    assert_eq!(out[0].kind, "supersedes");

    let inc = store
        .links(old_decision, LinkDirection::In, &empty)
        .expect("in");
    assert_eq!(inc.len(), 1, "expected one incoming link, got {inc:?}");
    assert_eq!(inc[0].peer_id, new_decision);

    let both = store
        .links(new_decision, LinkDirection::Both, &empty)
        .expect("both");
    assert_eq!(both.len(), 1);

    assert!(store.unlink(new_decision, old_decision).unwrap(), "removed");
    assert!(!store.unlink(new_decision, old_decision).unwrap(), "no-op");
    let out = store
        .links(new_decision, LinkDirection::Out, &empty)
        .expect("out2");
    assert!(out.is_empty(), "expected no links after unlink");
}

#[test]
#[ignore]
fn links_respect_tag_filter_on_peer() {
    let (_tmp, store) = open_tmp();

    let hub = store.store("hub entry", None, &[]).expect("hub");
    let tagged = store
        .store("tagged peer", None, &["interesting".into()])
        .expect("tagged");
    let boring = store
        .store("boring peer", None, &["boring".into()])
        .expect("boring");

    store.link(hub, tagged, None).expect("link tagged");
    store.link(hub, boring, None).expect("link boring");

    // Include `interesting`: only `tagged` peer surfaces.
    let filter = TagFilter {
        include: vec!["interesting".into()],
        exclude: vec![],
    };
    let hits = store
        .links(hub, LinkDirection::Out, &filter)
        .expect("filtered");
    let peers: Vec<_> = hits.iter().map(|l| l.peer_id).collect();
    assert_eq!(peers, vec![tagged]);

    // Exclude `boring`: also just `tagged` surfaces.
    let filter = TagFilter {
        include: vec![],
        exclude: vec!["boring".into()],
    };
    let hits = store
        .links(hub, LinkDirection::Out, &filter)
        .expect("exclude");
    let peers: Vec<_> = hits.iter().map(|l| l.peer_id).collect();
    assert_eq!(peers, vec![tagged]);
}

#[test]
#[ignore]
fn link_self_and_unknown_entry_are_rejected() {
    let (_tmp, store) = open_tmp();

    let real = store.store("a real entry", None, &[]).expect("store");

    let self_err = store.link(real, real, None).expect_err("self-link");
    assert!(self_err.to_string().to_lowercase().contains("itself"));

    let ghost = uuid::Uuid::new_v4();
    let missing_err = store.link(real, ghost, None).expect_err("missing to");
    assert!(
        missing_err.to_string().to_lowercase().contains("unknown"),
        "got {missing_err}"
    );
}
