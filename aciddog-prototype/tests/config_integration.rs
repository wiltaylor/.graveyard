use std::path::PathBuf;

use aciddog::config::load_config;
use aciddog::scaffold::ensure_demo;
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn loads_scaffold_template_end_to_end() {
    let path = repo_root()
        .join("scaffold")
        .join("demo")
        .join("aciddog.wcl");
    let cfg = load_config(&path).expect("scaffold aciddog.wcl should parse");

    assert_eq!(cfg.team.id, "default");
    assert!(cfg.agents.iter().any(|a| a.id == "planner"));
    assert!(cfg.agents.iter().any(|a| a.id == "coder"));
    for agent in &cfg.agents {
        assert!(!agent.role.is_empty());
        assert!(!agent.instructions.is_empty());
        assert!(!agent.model.is_empty());
    }

    // Workers declared in the scaffold should parse and be referenced
    // correctly by each other and by the coder agent.
    assert!(cfg.workers.iter().any(|w| w.id == "researcher"));
    assert!(cfg.workers.iter().any(|w| w.id == "fetcher"));
    assert!(cfg.can_create("coder", "researcher"));
    assert!(cfg.can_create("researcher", "fetcher"));
    assert!(!cfg.can_create("planner", "researcher"));
}

#[test]
fn ensure_demo_bootstraps_from_scaffold_template() {
    let tmp = TempDir::new().unwrap();
    let demo = tmp.path().join("demo");
    let template = repo_root().join("scaffold").join("demo");

    let created = ensure_demo(&demo, &template).unwrap();
    assert!(created);
    assert!(demo.join("aciddog.wcl").is_file());

    // Second call is a no-op.
    let created_again = ensure_demo(&demo, &template).unwrap();
    assert!(!created_again);

    // The copied file is a valid aciddog config.
    let cfg = load_config(&demo.join("aciddog.wcl")).unwrap();
    assert_eq!(cfg.team.id, "default");
}
