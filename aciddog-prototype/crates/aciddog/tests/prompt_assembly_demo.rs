//! End-to-end check that the demo WCL config produces sensibly-structured
//! first-turn prompts — exercises the new `PromptParts` assembly inside
//! `ClaudeCodeCore`. Each agent is a different scenario:
//!
//! - `planner` has no `Bash` tool ⇒ `<tools>` should embed the tag-protocol
//!   preamble (with `<aciddog-call>` examples).
//! - `coder` inherits the default tool set including `Bash` ⇒ `<tools>`
//!   should embed the CLI preamble (with `aciddog send --to ...`).
//!
//! The orchestrator owns team-block assembly, so we mirror the bits of
//! `app.rs` we need (the helper here is intentionally a small copy — keeping
//! the test self-contained avoids exposing the orchestrator helper as
//! `pub`).

use std::path::PathBuf;

use aciddog::ai::{AciddogCliCtx, ClaudeCodeCore, PromptParts};
use aciddog::config::{load_config, AciddogConfig};
use uuid::Uuid;

fn demo_config() -> AciddogConfig {
    // CARGO_MANIFEST_DIR is crates/aciddog/; the demo/ dir lives at the repo root.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate manifest dir has at least two ancestors")
        .join("demo/aciddog.wcl");
    load_config(&path).expect("demo config parses")
}

fn build_team_block(cfg: &AciddogConfig, contactable: &[&str], creatable: &[&str]) -> String {
    let mut out = String::new();
    out.push_str(&format!("Team: {}\n", cfg.team.description));
    if !contactable.is_empty() {
        out.push_str("\nAgents you can message:\n");
        for id in contactable {
            let role = cfg
                .agents
                .iter()
                .find(|a| a.id == *id)
                .map(|a| a.role.as_str())
                .unwrap_or("agent");
            out.push_str(&format!("- {id} — {role}\n"));
        }
    }
    if !creatable.is_empty() {
        out.push_str("\nWorker templates you can spawn:\n");
        for id in creatable {
            let role = cfg
                .workers
                .iter()
                .find(|w| w.id == *id)
                .map(|w| w.role.as_str())
                .unwrap_or("worker");
            out.push_str(&format!("- {id} — {role}\n"));
        }
    }
    out
}

#[test]
fn planner_prompt_has_role_team_tag_tools_and_instructions() {
    let cfg = demo_config();
    let planner = cfg
        .agents
        .iter()
        .find(|a| a.id == "planner")
        .expect("planner");

    // Planner can message every other agent and spawn jokester. It has
    // no Bash tool, so the <tools> block must use the tag preamble.
    let contactable: Vec<&str> = cfg
        .agents
        .iter()
        .filter(|a| a.id != "planner")
        .map(|a| a.id.as_str())
        .collect();
    let creatable: Vec<&str> = vec!["jokester"];

    let team = build_team_block(&cfg, &contactable, &creatable);
    let parts = PromptParts {
        role: planner.role_description.clone(),
        team: Some(team),
        memory: None,
        instructions: planner.instructions.clone(),
        aciddog_cli: Some(AciddogCliCtx {
            binary: "/usr/local/bin/aciddog".into(),
            contactable: contactable.iter().map(|s| s.to_string()).collect(),
            creatable: creatable.iter().map(|s| s.to_string()).collect(),
            memory_banks: vec![],
            skills: vec![],
            channels: vec![],
            supervisor: false,
            sysadmin: false,
            worker: None,
        }),
    };
    let core = ClaudeCodeCore::new(
        Uuid::new_v4(),
        parts,
        planner.tools.clone().unwrap_or_default(),
        None,
    );
    let p = core.first_turn_prompt();
    eprintln!("───── PLANNER PROMPT ─────\n{p}\n──────────────────────────");

    assert!(p.contains("<role>"), "missing role: {p}");
    assert!(
        p.contains("team coordinator"),
        "role_description missing: {p}"
    );
    assert!(p.contains("<team>"), "missing team: {p}");
    assert!(
        p.contains("- coder — implementer"),
        "team listing missing: {p}"
    );
    assert!(
        p.contains("- jokester — comedian"),
        "creatable listing missing: {p}"
    );
    // planner has no memory banks in this test setup (we pass `memory: None`);
    // see the dedicated memory-block test below for the positive case.
    assert!(!p.contains("<memory>"), "unexpected memory block: {p}");
    assert!(p.contains("<tools>"), "missing tools: {p}");
    assert!(p.contains("- `Read` —"), "missing built-in Read line: {p}");
    assert!(
        !p.contains("- `Bash` —"),
        "planner has no Bash; should not be listed: {p}"
    );
    assert!(
        p.contains("<aciddog-call>"),
        "planner has no Bash → tag preamble expected: {p}"
    );
    assert!(
        p.contains("| send_message |"),
        "tag-mode send_message row missing: {p}"
    );
    assert!(
        !p.contains("/usr/local/bin/aciddog test"),
        "CLI preamble leaked into tag-mode tools: {p}"
    );
    assert!(p.contains("<instructions>"), "missing instructions: {p}");
}

#[test]
fn coder_prompt_uses_cli_preamble_inside_tools() {
    let cfg = demo_config();
    let coder = cfg.agents.iter().find(|a| a.id == "coder").expect("coder");

    let contactable: Vec<&str> = cfg
        .agents
        .iter()
        .filter(|a| a.id != "coder")
        .map(|a| a.id.as_str())
        .collect();
    let creatable: Vec<&str> = vec!["researcher"];

    let core = cfg.effective_ai_core(coder.ai_core.as_deref());
    let tools = cfg.effective_tools(coder.tools.as_deref(), core);
    assert!(
        tools.iter().any(|t| t == "Bash"),
        "coder should inherit Bash from default_tools: {tools:?}"
    );

    let parts = PromptParts {
        role: coder.role_description.clone(),
        team: Some(build_team_block(&cfg, &contactable, &creatable)),
        memory: None,
        instructions: coder.instructions.clone(),
        aciddog_cli: Some(AciddogCliCtx {
            binary: "/usr/local/bin/aciddog".into(),
            contactable: contactable.iter().map(|s| s.to_string()).collect(),
            creatable: creatable.iter().map(|s| s.to_string()).collect(),
            memory_banks: vec![],
            skills: vec![],
            channels: vec![],
            supervisor: false,
            sysadmin: false,
            worker: None,
        }),
    };
    let core = ClaudeCodeCore::new(Uuid::new_v4(), parts, tools, None);
    let p = core.first_turn_prompt();
    eprintln!("───── CODER PROMPT ─────\n{p}\n────────────────────────");

    assert!(p.contains("- `Bash` —"), "missing Bash built-in line: {p}");
    assert!(
        p.contains("/usr/local/bin/aciddog test"),
        "expected CLI preamble header (binary path) inside <tools>: {p}"
    );
    assert!(p.contains("| send |"), "CLI-mode send row missing: {p}");
    assert!(
        !p.contains("<aciddog-call>"),
        "should not include tag preamble: {p}"
    );
    assert!(
        p.contains("- researcher — investigator"),
        "creatable researcher listing missing: {p}"
    );
}

/// The `<memory>` block lists each bank the actor can access, with its
/// description. A separate CLI doc inside `<tools>` teaches the model
/// how to call `memory search` / `memory store`.
#[test]
fn planner_prompt_includes_memory_block_and_cli_docs_for_allowed_banks() {
    let cfg = demo_config();
    let planner = cfg
        .agents
        .iter()
        .find(|a| a.id == "planner")
        .expect("planner");

    // Build a memory block mirroring what the orchestrator does.
    let banks = cfg.memory_banks_for("planner");
    assert!(
        !banks.is_empty(),
        "demo should grant planner at least one bank"
    );
    let mut memory_body = String::new();
    memory_body.push_str("You have access to the following memory banks.\n\n");
    for bank in &banks {
        memory_body.push_str(&format!("- {} — {}\n", bank.id, bank.description));
    }

    let bank_ids: Vec<String> = banks.iter().map(|b| b.id.clone()).collect();
    let contactable: Vec<&str> = cfg
        .agents
        .iter()
        .filter(|a| a.id != "planner")
        .map(|a| a.id.as_str())
        .collect();
    let creatable: Vec<&str> = vec!["jokester"];

    let parts = PromptParts {
        role: planner.role_description.clone(),
        team: Some(build_team_block(&cfg, &contactable, &creatable)),
        memory: Some(memory_body),
        instructions: planner.instructions.clone(),
        aciddog_cli: Some(AciddogCliCtx {
            binary: "/usr/local/bin/aciddog".into(),
            contactable: contactable.iter().map(|s| s.to_string()).collect(),
            creatable: creatable.iter().map(|s| s.to_string()).collect(),
            memory_banks: bank_ids.clone(),
            skills: vec![],
            channels: vec![],
            supervisor: false,
            sysadmin: false,
            worker: None,
        }),
    };
    let core = ClaudeCodeCore::new(
        Uuid::new_v4(),
        parts,
        planner.tools.clone().unwrap_or_default(),
        None,
    );
    let p = core.first_turn_prompt();

    assert!(p.contains("<memory>"), "missing memory tag: {p}");
    assert!(
        p.contains("- project_context —"),
        "bank listing missing: {p}"
    );
    // Planner has no Bash → tag preamble; look for tag-flavored docs.
    assert!(
        p.contains("| memory_search |"),
        "memory_search tag row missing: {p}"
    );
    assert!(
        p.contains("| memory_store |"),
        "memory_store tag row missing: {p}"
    );
    // Memory tag should appear before the tools tag in reading order.
    let mem_pos = p.find("<memory>").unwrap();
    let tools_pos = p.find("<tools>").unwrap();
    assert!(mem_pos < tools_pos, "memory should precede tools: {p}");
}
