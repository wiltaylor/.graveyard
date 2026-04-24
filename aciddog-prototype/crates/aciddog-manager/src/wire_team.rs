//! Deserialize/serialize mirror of
//! `aciddog::wire_team::{WireTeamInfo, WireAiCore, WireFleetCounts}`.
//!
//! Same duplication rationale as the other `wire_*.rs` modules —
//! canonical shape is in `crates/aciddog/src/wire_team.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTeam {
    pub id: String,
    pub description: String,
    pub replace_thinking_messages: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ai_core: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireUsageProvider {
    Zai,
    ChatgptOauth,
    Openrouter,
    GithubCopilot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireFleetCounts {
    pub agents: usize,
    pub workers: usize,
    pub automatons: usize,
    pub channels: usize,
    pub workflows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAiCore {
    pub id: String,
    pub kind_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools: Option<Vec<String>>,
    pub has_system_prompt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_provider: Option<WireUsageProvider>,
    pub is_team_default: bool,
    #[serde(default)]
    pub agents_using: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTeamInfo {
    pub team: WireTeam,
    #[serde(default)]
    pub ai_cores: Vec<WireAiCore>,
    pub fleet: WireFleetCounts,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_info_round_trips() {
        let wire = r#"{
            "team": {
                "id":"demo",
                "description":"x",
                "replace_thinking_messages": false,
                "default_ai_core":"c1"
            },
            "ai_cores": [
                {
                    "id":"c1",
                    "kind_label":"claudeCode",
                    "has_system_prompt": true,
                    "is_team_default": true,
                    "agents_using": ["planner","coder"]
                }
            ],
            "fleet": {"agents":2,"workers":0,"automatons":1,"channels":0,"workflows":0}
        }"#;
        let info: WireTeamInfo = serde_json::from_str(wire).unwrap();
        assert_eq!(info.team.id, "demo");
        assert_eq!(info.ai_cores.len(), 1);
        assert_eq!(info.ai_cores[0].agents_using, vec!["planner", "coder"]);
        assert_eq!(info.fleet.agents, 2);
    }
}
