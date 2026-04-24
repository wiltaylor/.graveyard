//! Serde-friendly snapshot of an aciddog instance's team + AI core
//! configuration. Returned by the supervisor `TeamInfo` RPC to the
//! manager web portal's Team tab.
//!
//! We expose only fields relevant for the overview — `system_prompt`
//! bodies and other long-form blobs stay off the wire until a caller
//! asks for them explicitly.

use serde::{Deserialize, Serialize};

use crate::config::{AciddogConfig, AiCore, AiCoreKind, Team, UsageProvider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTeamInfo {
    pub team: Team,
    pub ai_cores: Vec<WireAiCore>,
    pub fleet: WireFleetCounts,
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
    /// Stable camelCase variant tag — matches what a config file
    /// writes as `type = :claudeCode`.
    pub kind_label: String,
    /// Variant-specific model id, when one is pinned on the core.
    /// `None` for CLI variants that let the binary pick a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Human-readable credential source: `$ENV_VAR` for API cores,
    /// the credential name for the ChatGPT variant, `None` for CLI
    /// cores that manage their own auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    /// Per-core default tool allowlist, when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools: Option<Vec<String>>,
    /// Whether this core has a system prompt configured. We don't
    /// emit the body itself to keep the wire snapshot small — the
    /// portal can surface "has a system prompt" without the content.
    pub has_system_prompt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_provider: Option<UsageProvider>,
    /// `true` when this core is the team's `default_ai_core`.
    pub is_team_default: bool,
    /// Ids of declared agents resolving to this core (either via an
    /// explicit `ai_core` attribute or by inheriting the team
    /// default). Matches `cores_view::agents_using_core`.
    pub agents_using: Vec<String>,
}

impl WireTeamInfo {
    pub fn project(config: &AciddogConfig) -> Self {
        let ai_cores = config
            .ai_cores
            .iter()
            .map(|core| WireAiCore::project(core, config))
            .collect();
        Self {
            team: config.team.clone(),
            ai_cores,
            fleet: WireFleetCounts {
                agents: config.agents.len(),
                workers: config.workers.len(),
                automatons: config.automatons.len(),
                channels: config.channels.len(),
                workflows: config.workflows.len(),
            },
        }
    }
}

impl WireAiCore {
    fn project(core: &AiCore, config: &AciddogConfig) -> Self {
        Self {
            id: core.id.clone(),
            kind_label: kind_label(&core.kind).to_string(),
            model: core.runtime_model().map(str::to_string),
            credential: credential_label(&core.kind),
            default_tools: core.default_tools().map(<[String]>::to_vec),
            has_system_prompt: core_has_system_prompt(&core.kind),
            usage_provider: core.effective_usage_provider(),
            is_team_default: config.team.default_ai_core.as_deref() == Some(core.id.as_str()),
            agents_using: crate::tui::agents_using_core(config, &core.id)
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

fn kind_label(kind: &AiCoreKind) -> &'static str {
    match kind {
        AiCoreKind::ClaudeCode { .. } => "claudeCode",
        AiCoreKind::AnthropicApi { .. } => "anthropicApi",
        AiCoreKind::OpenAiApi { .. } => "openAiApi",
        AiCoreKind::OpenAiChatgpt { .. } => "openAiChatgpt",
        AiCoreKind::CopilotCli { .. } => "copilotCli",
        AiCoreKind::GeminiCli { .. } => "geminiCli",
        AiCoreKind::Jules { .. } => "jules",
    }
}

fn credential_label(kind: &AiCoreKind) -> Option<String> {
    match kind {
        AiCoreKind::AnthropicApi { api_key_env, .. }
        | AiCoreKind::OpenAiApi { api_key_env, .. }
        | AiCoreKind::Jules { api_key_env, .. } => Some(format!("${api_key_env}")),
        AiCoreKind::OpenAiChatgpt { credential, .. } => Some(credential.clone()),
        AiCoreKind::ClaudeCode { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. } => None,
    }
}

fn core_has_system_prompt(kind: &AiCoreKind) -> bool {
    match kind {
        AiCoreKind::ClaudeCode { system_prompt, .. }
        | AiCoreKind::AnthropicApi { system_prompt, .. }
        | AiCoreKind::OpenAiApi { system_prompt, .. }
        | AiCoreKind::OpenAiChatgpt { system_prompt, .. } => system_prompt.is_some(),
        AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. }
        | AiCoreKind::Jules { .. } => false,
    }
}
