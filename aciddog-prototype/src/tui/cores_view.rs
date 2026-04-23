//! State for the in-TUI Cores screen.
//!
//! The Cores screen lists every configured `ai_core` together with a
//! usage percentage when the core has a resolvable
//! [`crate::config::UsageProvider`]. All state here is UI-local; the
//! config is the source of truth for the core list, and per-core usage
//! snapshots are keyed by core id so background fetches can deliver
//! results without worrying about list order.

use std::collections::HashMap;

use crate::config::{AciddogConfig, AiCore, AiCoreKind, UsageProvider};
use crate::usage::UsageSnapshot;

/// Return the ids of agents that resolve to `core_id`, either by
/// declaring it explicitly via their `ai_core` attribute or by
/// inheriting the team's `default_ai_core`. Order matches declaration
/// order in `aciddog.wcl`.
pub fn agents_using_core<'a>(config: &'a AciddogConfig, core_id: &str) -> Vec<&'a str> {
    let team_default_is_this = config.team.default_ai_core.as_deref() == Some(core_id);
    config
        .agents
        .iter()
        .filter(|a| match a.ai_core.as_deref() {
            Some(id) => id == core_id,
            None => team_default_is_this,
        })
        .map(|a| a.id.as_str())
        .collect()
}

/// True iff `core_id` is the team-wide default core. Surfaced in the
/// detail panel so it's clear even when no agent explicitly references
/// it.
pub fn is_team_default(config: &AciddogConfig, core_id: &str) -> bool {
    config.team.default_ai_core.as_deref() == Some(core_id)
}

/// Event delivered to the TUI when a background usage fetch
/// completes. One event per core; the app writes the result into
/// [`CoresScreen::usage`] and clears `fetching` once every pending
/// reply has arrived.
#[derive(Debug)]
pub struct UsageEvent {
    pub core_id: String,
    pub snapshot: UsageSnapshot,
    /// Generation counter from the dispatching [`CoresScreen`]. Stale
    /// replies (the user clicked `r` again) are ignored.
    pub generation: u64,
}

/// A single flattened row shown in the Cores view. Derived from the
/// config every time the view is opened so rename/reorder/removal in
/// `aciddog.wcl` is reflected without a restart.
#[derive(Debug, Clone)]
pub struct CoreRow {
    pub id: String,
    pub kind_label: &'static str,
    /// Human-readable credential identifier for the variant — env-var
    /// name prefixed with `$` for API cores, credential name for the
    /// ChatGPT variant, `None` for CLI-harness cores that manage their
    /// own auth outside the config.
    pub credential: Option<String>,
    pub usage_provider: Option<UsageProvider>,
}

#[derive(Debug, Default)]
pub struct CoresScreen {
    pub cursor: usize,
    pub rows: Vec<CoreRow>,
    /// Usage snapshot per core id. Missing keys render as a dash
    /// (no provider) or "…" (fetch in flight) depending on
    /// `fetching`.
    pub usage: HashMap<String, UsageSnapshot>,
    /// Number of in-flight fetches. Renders as "…" when > 0.
    pub pending: usize,
    /// Monotonic id of the current fetch batch. Incremented on each
    /// refresh so late replies from a cancelled batch are discarded.
    pub generation: u64,
}

impl CoresScreen {
    pub fn rebuild(&mut self, cores: &[AiCore]) {
        self.rows = cores.iter().map(CoreRow::from_core).collect();
        self.reclamp();
    }

    pub fn reclamp(&mut self) {
        if self.rows.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len() - 1;
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.cursor = 0;
            return;
        }
        let len = self.rows.len() as isize;
        let next = (self.cursor as isize + delta).clamp(0, len - 1);
        self.cursor = next as usize;
    }
}

impl CoreRow {
    fn from_core(core: &AiCore) -> Self {
        Self {
            id: core.id.clone(),
            kind_label: kind_label(&core.kind),
            credential: credential_label(&core.kind),
            usage_provider: core.effective_usage_provider(),
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
    }
}

fn credential_label(kind: &AiCoreKind) -> Option<String> {
    match kind {
        AiCoreKind::AnthropicApi { api_key_env, .. }
        | AiCoreKind::OpenAiApi { api_key_env, .. } => Some(format!("${}", api_key_env)),
        AiCoreKind::OpenAiChatgpt { credential, .. } => Some(credential.clone()),
        AiCoreKind::ClaudeCode { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. } => None,
    }
}

/// Render a `UsageSnapshot` (or its absence) into a one-line cell for
/// the table.
pub fn format_usage_cell(
    provider: Option<UsageProvider>,
    snapshot: Option<&UsageSnapshot>,
    pending: bool,
) -> String {
    let Some(_) = provider else {
        return "—".to_string();
    };
    match snapshot {
        None if pending => "…".to_string(),
        None => "(idle)".to_string(),
        Some(s) if s.error.is_some() => format!("err: {}", s.error.as_deref().unwrap_or("?")),
        Some(s) if s.display_override.is_some() => s
            .display_override
            .as_deref()
            .unwrap_or("—")
            .to_string(),
        Some(s) => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(p) = s.five_hour_pct {
                if let Some(reset) = s.reset_display.as_deref() {
                    parts.push(format!("5h:{}% ({})", p, reset));
                } else {
                    parts.push(format!("5h:{}%", p));
                }
            }
            if let Some(p) = s.week_pct {
                parts.push(format!("wk:{}%", p));
            }
            if parts.is_empty() {
                "—".to_string()
            } else {
                parts.join(" · ")
            }
        }
    }
}
