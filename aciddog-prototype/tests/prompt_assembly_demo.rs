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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo/aciddog.wcl");
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
    let planner = cfg.agents.iter().find(|a| a.id == "planner").expect("planner");

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
        instructions: planner.instructions.clone(),
        aciddog_cli: Some(AciddogCliCtx {
            binary: "/usr/local/bin/aciddog".into(),
            contactable: contactable.iter().map(|s| s.to_string()).collect(),
            creatable: creatable.iter().map(|s| s.to_string()).collect(),
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
    assert!(p.contains("team coordinator"), "role_description missing: {p}");
    assert!(p.contains("<team>"), "missing team: {p}");
    assert!(p.contains("- coder — implementer"), "team listing missing: {p}");
    assert!(p.contains("- jokester — comedian"), "creatable listing missing: {p}");
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
        !p.contains("/usr/local/bin/aciddog send"),
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
        instructions: coder.instructions.clone(),
        aciddog_cli: Some(AciddogCliCtx {
            binary: "/usr/local/bin/aciddog".into(),
            contactable: contactable.iter().map(|s| s.to_string()).collect(),
            creatable: creatable.iter().map(|s| s.to_string()).collect(),
            worker: None,
        }),
    };
    let core = ClaudeCodeCore::new(Uuid::new_v4(), parts, tools, None);
    let p = core.first_turn_prompt();
    eprintln!("───── CODER PROMPT ─────\n{p}\n────────────────────────");

    assert!(p.contains("- `Bash` —"), "missing Bash built-in line: {p}");
    assert!(
        p.contains("/usr/local/bin/aciddog send --to"),
        "expected CLI preamble inside <tools>: {p}"
    );
    assert!(
        !p.contains("<aciddog-call>"),
        "should not include tag preamble: {p}"
    );
    assert!(
        p.contains("- researcher — investigator"),
        "creatable researcher listing missing: {p}"
    );
}
