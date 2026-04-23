//! State for the in-TUI Cores screen.
//!
//! The Cores screen lists every configured `ai_core` together with a
//! usage percentage when the core has a resolvable
//! [`crate::config::UsageProvider`]. All state here is UI-local; the
//! config is the source of truth for the core list, and per-core usage
//! snapshots are keyed by core id so background fetches can deliver
//! results without worrying about list order.

use std::collections::HashMap;

use crate::ai::event::RateLimitKind;
use crate::config::{AciddogConfig, AiCore, AiCoreKind, UsageProvider};
use crate::usage::{merge_rate_limit_into, UsageSnapshot};

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

    /// Fold a `rate_limit_event` into the usage entry for `core_id`.
    /// Each kind writes only its own window slot, so the two events
    /// (five-hour, weekly) that arrive on different turns don't clobber
    /// each other.
    pub fn merge_rate_limit(
        &mut self,
        core_id: &str,
        kind: RateLimitKind,
        resets_at_unix: Option<i64>,
    ) {
        let prev = self.usage.remove(core_id);
        let merged = merge_rate_limit_into(prev, kind, resets_at_unix);
        self.usage.insert(core_id.to_string(), merged);
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
        AiCoreKind::Jules { .. } => "jules",
    }
}

fn credential_label(kind: &AiCoreKind) -> Option<String> {
    match kind {
        AiCoreKind::AnthropicApi { api_key_env, .. }
        | AiCoreKind::OpenAiApi { api_key_env, .. }
        | AiCoreKind::Jules { api_key_env, .. } => Some(format!("${}", api_key_env)),
        AiCoreKind::OpenAiChatgpt { credential, .. } => Some(credential.clone()),
        AiCoreKind::ClaudeCode { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. } => None,
    }
}

/// Render a `UsageSnapshot` (or its absence) into a one-line cell for
/// the table. A populated snapshot (error or display_override) wins
/// over an absent provider, so `claudeCode` rows — which have no
/// HTTP provider — still render their stream-derived reset info.
pub fn format_usage_cell(
    provider: Option<UsageProvider>,
    snapshot: Option<&UsageSnapshot>,
    pending: bool,
) -> String {
    if let Some(s) = snapshot {
        if let Some(err) = s.error.as_deref() {
            return format!("err: {err}");
        }
        if let Some(ov) = s.display_override.as_deref() {
            return ov.to_string();
        }
    }
    if provider.is_none() {
        return "—".to_string();
    }
    match snapshot {
        None if pending => "…".to_string(),
        None => "(idle)".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_rate_limit_populates_five_hour_slot() {
        let mut screen = CoresScreen::default();
        screen.merge_rate_limit("cc", RateLimitKind::FiveHour, Some(i64::MAX));
        let snap = screen.usage.get("cc").expect("entry");
        assert_eq!(snap.claude_five_hour_resets_at, Some(i64::MAX));
        assert!(snap.claude_weekly_resets_at.is_none());
        assert!(snap.display_override.is_some());
    }

    #[test]
    fn merge_rate_limit_weekly_does_not_clobber_five_hour() {
        let mut screen = CoresScreen::default();
        screen.merge_rate_limit("cc", RateLimitKind::FiveHour, Some(10));
        screen.merge_rate_limit("cc", RateLimitKind::Weekly, Some(20));
        let snap = screen.usage.get("cc").expect("entry");
        assert_eq!(snap.claude_five_hour_resets_at, Some(10));
        assert_eq!(snap.claude_weekly_resets_at, Some(20));
    }

    #[test]
    fn format_usage_cell_renders_stream_snapshot_with_no_provider() {
        let snap = UsageSnapshot {
            display_override: Some("5h in 2h 14m".to_string()),
            ..UsageSnapshot::default()
        };
        let cell = format_usage_cell(None, Some(&snap), false);
        assert_eq!(cell, "5h in 2h 14m");
    }

    #[test]
    fn format_usage_cell_still_dashes_for_no_provider_no_snapshot() {
        assert_eq!(format_usage_cell(None, None, false), "—");
    }

    #[test]
    fn format_usage_cell_shows_error_over_provider_percentages() {
        let snap = UsageSnapshot {
            error: Some("http 500".to_string()),
            five_hour_pct: Some(42),
            ..UsageSnapshot::default()
        };
        let cell = format_usage_cell(Some(UsageProvider::Zai), Some(&snap), false);
        assert_eq!(cell, "err: http 500");
    }
}
