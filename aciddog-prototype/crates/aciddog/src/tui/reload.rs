//! `/reload` implementation.
//!
//! Re-reads the WCL config from `App::config_path` and reconciles the
//! running fleet against the new declaration: agents (with per-agent
//! `on_reload` policy), channels, workflows, automatons + webhooks.
//! Workers are template-driven and don't need reconciliation — new
//! instances pick up the new config naturally on the next spawn.
//!
//! Cron is intentionally NOT reloaded here. Cron jobs are not declared
//! in WCL today; they live entirely in `.aciddog/cron.json` and are
//! mutated through the cron CLI / TUI view. Reloading the WCL has no
//! cron-relevant content to reconcile.
//!
//! Failure modes:
//! - Parse error → `App::config` is left untouched; the caller pushes
//!   a system_error block. The running fleet keeps serving the old
//!   config.
//! - Webservice automaton restart races a fresh listener bind against
//!   the old listener's socket release. The old driver only frees its
//!   port/socket after observing its `cancel_tx`; the new driver may
//!   briefly fail to bind. v1 accepts this; if it bites, run `/reload`
//!   again.

use std::sync::Arc;

use crate::config::{self, AciddogConfig, Agent, Automaton, ReloadAction, Webhook};

use super::app::{AgentStatus, App};
use super::blocks::ChatBlock;
use super::boot::{add_agent_chat, add_automaton_chat, spawn_automaton_core};

impl App {
    /// Re-read the WCL config and reconcile the running fleet.
    ///
    /// Returns a one-line summary on success (counts of added /
    /// removed / changed entities across each subsystem), or a parse
    /// error string on failure. On parse failure `self.config` is left
    /// unchanged.
    pub fn reload(&mut self) -> Result<String, String> {
        let path = self.config_path.clone();
        if path.as_os_str().is_empty() {
            return Err("config path not set; reload is unavailable in this run".to_string());
        }
        let new_config = config::load_config(&path).map_err(|e| e.to_string())?;

        let old = self.config.clone();

        // --- Build diff plans before we mutate anything --------------

        let agent_plan = AgentReloadPlan::compute(&old, &new_config);
        let automaton_plan = AutomatonReloadPlan::compute(&old, &new_config);
        let channel_plan = ChannelReloadPlan::compute(&old, &new_config);

        // --- Atomic config swap --------------------------------------
        //
        // This is the load-bearing step. After this point every read
        // off `self.config` (including the `teardown_and_rebuild` path
        // in `compaction.rs`) sees the new picture. That's exactly
        // what makes `ReloadAction::Ignore` cost-free: the running
        // session keeps its old prompt in memory, but the next
        // `/compact` or `/reset` rebuilds against the fresh config.

        self.config = new_config;
        if let Some(engine) = self.workflow_engine.as_mut() {
            engine.config = Arc::new(self.config.clone());
        }

        // --- Apply plans in dependency order -------------------------
        //
        // Agents → channels → workflows → automatons. Channels depend
        // on agent identities (member lists); automatons may host
        // webhooks that reference channels and agents.

        let agent_outcome = self.apply_agent_plan(&agent_plan);
        let channel_outcome = self.apply_channel_plan(&channel_plan);
        let workflow_outcome = self.apply_workflow_clamp();
        let automaton_outcome = self.apply_automaton_plan(&automaton_plan);

        // Defensive cron wakeup — cron isn't reloaded but a long
        // reload window may have starved its scheduler arm; this
        // re-evaluates the next-due-at on the next event-loop tick.
        self.cron_invalidate.notify_one();

        Ok(format!(
            "reload: agents +{}/-{}/~{} ({}), channels +{}/-{}/~{}, workflows {}, automatons +{}/-{}/~{}",
            agent_outcome.added,
            agent_outcome.removed,
            agent_outcome.changed,
            agent_outcome.action_breakdown(),
            channel_outcome.added,
            channel_outcome.removed_warned,
            channel_outcome.members_updated,
            workflow_outcome,
            automaton_outcome.added,
            automaton_outcome.removed,
            automaton_outcome.restarted,
        ))
    }

    fn apply_agent_plan(&mut self, plan: &AgentReloadPlan) -> AgentOutcome {
        let mut outcome = AgentOutcome::default();

        // 1. Removed agents — hibernate the chat slot, drop IPC
        //    handles, purge from `agent_ids`. We don't physically
        //    `Vec::remove` because that would renumber every chat
        //    behind it (workers reference parent_idx, in-flight tasks
        //    hold idx). Hibernation hides the pane from the agent
        //    list while keeping the transcript intact.
        for removed_id in &plan.removed_ids {
            if let Some(idx) = self.find_agent_idx_by_id(removed_id) {
                self.hibernate_chat_slot(idx, &format!("agent `{removed_id}` removed by /reload"));
                outcome.removed += 1;
            }
        }

        // 2. Changed agents — dispatch by per-agent `on_reload`. Note
        //    `self.config` already reflects the NEW config, so
        //    `begin_compaction` / `teardown_and_rebuild` will pick up
        //    the new prompt automatically.
        for changed in &plan.changed {
            if let Some(idx) = self.find_agent_idx_by_id(&changed.id) {
                match changed.action {
                    ReloadAction::Ignore => {
                        outcome.action_ignored += 1;
                        if let Some(chat) = self.chats.get_mut(idx) {
                            chat.push_block(ChatBlock::system_info(
                                "agent config changed — running session unaffected (on_reload = ignore). Run /compact or /reset to apply.",
                            ));
                        }
                    }
                    ReloadAction::Compact => {
                        outcome.action_compact += 1;
                        self.begin_compaction(idx, false);
                    }
                    ReloadAction::Reset => {
                        outcome.action_reset += 1;
                        self.begin_compaction(idx, true);
                    }
                }
                outcome.changed += 1;
            }
        }

        // 3. Added agents — append a new chat slot at the tail and
        //    spawn its core. Tail-append (rather than slotting at
        //    config-declaration position) preserves the in-memory
        //    indices held by every running task; canonical ordering
        //    is restored on the next full restart.
        for added_id in &plan.added_ids {
            // Look up the freshly-parsed agent struct from the new
            // (already-swapped) config.
            let agent = match self.config.agents.iter().find(|a| &a.id == added_id) {
                Some(a) => a.clone(),
                None => continue,
            };
            match add_agent_chat(self, &agent) {
                Ok(_) => outcome.added += 1,
                Err(e) => {
                    self.push_top_warning(format!("failed to spawn agent `{}`: {e}", agent.id));
                }
            }
        }

        outcome
    }

    fn apply_channel_plan(&mut self, plan: &ChannelReloadPlan) -> ChannelOutcome {
        let mut outcome = ChannelOutcome::default();

        // 1. Removed channels — flip to ephemeral with a warning, but
        //    keep the pane open and the disk log alive. The user can
        //    `/leave` to dismiss it.
        for removed_id in &plan.removed_ids {
            if let Some(state) = self.channels.iter_mut().find(|c| &c.id == removed_id) {
                if !state.is_ephemeral {
                    state.is_ephemeral = true;
                    state.blocks.push(ChatBlock::system_warn(
                        "channel removed from WCL — pane will close on /leave",
                    ));
                    outcome.removed_warned += 1;
                }
            }
        }

        // 2. Existing declared channels — overwrite `members` with
        //    the new declared set. Runtime joins/kicks (`/add`,
        //    `/kick`) lose: the WCL is the source of truth for
        //    declared channels.
        for (id, members) in &plan.member_updates {
            if let Some(state) = self.channels.iter_mut().find(|c| &c.id == id) {
                if !state.is_ephemeral && state.members != *members {
                    state.members = members.clone();
                    outcome.members_updated += 1;
                }
            }
        }

        // 3. Added channels — append a fresh `ChannelState` with the
        //    welcome banner seeded, then open its append-only JSONL
        //    log. Tail history is NOT replayed on reload — only at
        //    boot — to avoid showing a wall of stale messages from
        //    other sessions.
        for ch in &plan.added {
            let mut blocks: Vec<ChatBlock> = Vec::new();
            if let Some(w) = ch.welcome.as_ref().filter(|s| !s.trim().is_empty()) {
                blocks.push(ChatBlock::channel_welcome(w.clone()));
            }
            let mut state = super::app::ChannelState {
                id: ch.id.clone(),
                description: ch.description.clone(),
                welcome: ch.welcome.clone(),
                members: ch.members.clone(),
                blocks,
                sticky_bottom: true,
                ..Default::default()
            };
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            if let Err(e) = super::channel_log::migrate_legacy(&cwd, &ch.id) {
                tracing::warn!(channel = %ch.id, error = %e, "failed to migrate legacy channel log on reload");
            }
            let (started, hash) = match super::channel_log::latest_session(&cwd, &ch.id) {
                Some((_, h)) => (chrono::Utc::now(), h),
                None => {
                    let now = chrono::Utc::now();
                    (now, super::channel_log::session_hash(&ch.id, now))
                }
            };
            match super::channel_log::ChannelLog::open_session(&cwd, &ch.id, &hash) {
                Ok(log) => {
                    state.session_started = started;
                    state.session_hash = hash;
                    state.log = Some(log);
                }
                Err(e) => {
                    tracing::warn!(channel = %ch.id, error = %e, "failed to open channel log on reload");
                }
            }
            self.channels.push(state);
            outcome.added += 1;
        }

        outcome
    }

    fn apply_workflow_clamp(&mut self) -> &'static str {
        let total = self.config.workflows.len();
        if total == 0 {
            self.selected_workflow = 0;
        } else if self.selected_workflow >= total {
            self.selected_workflow = total - 1;
        }
        "config refreshed (running instances unaffected)"
    }

    fn apply_automaton_plan(&mut self, plan: &AutomatonReloadPlan) -> AutomatonOutcome {
        let mut outcome = AutomatonOutcome::default();

        // 1. Removed automatons — cancel the driver task and
        //    hibernate the pane. Listener cleanup happens inside the
        //    driver as it observes its own cancel signal.
        for removed_id in &plan.removed_ids {
            if let Some(idx) = self.find_automaton_idx_by_id(removed_id) {
                self.hibernate_chat_slot(
                    idx,
                    &format!("automaton `{removed_id}` removed by /reload"),
                );
                outcome.removed += 1;
            }
        }

        // 2. Restarted automatons — driver-level: send cancel, then
        //    immediately spawn a replacement via the same chat slot.
        //    Webservice listeners may briefly race their port/socket
        //    release; document the limitation and let the driver log
        //    bind errors if they fire.
        for restart in &plan.restarted {
            let Some(idx) = self.find_automaton_idx_by_id(&restart.id) else {
                continue;
            };
            // Fire-and-forget cancel — driver picks it up and exits
            // its event loop, releasing any held listener.
            if let Some(chat) = self.chats.get_mut(idx) {
                if let Some(tx) = chat.cancel_tx.take() {
                    let _ = tx.try_send(());
                }
                // Drop old IPC handles; the driver's own clones drop
                // when its task exits.
                chat.user_tx = None;
                chat.agent_tx = None;
                chat.channel_tx = None;
                if let Some(old_session) = chat.session_id.take() {
                    self.agent_ids.remove(&old_session);
                }
                chat.push_block(ChatBlock::system_warn(
                    "automaton config changed — restarting driver (listener may briefly fail to bind)",
                ));
            }
            // Look up the freshly parsed automaton from the swapped
            // config and respawn into the same chat slot.
            let automaton = match self.config.automatons.iter().find(|a| a.id == restart.id) {
                Some(a) => a.clone(),
                None => continue,
            };
            spawn_automaton_core(self, idx, &automaton);
            outcome.restarted += 1;
        }

        // 3. Added automatons — append a new chat slot at the tail.
        for added_id in &plan.added_ids {
            let automaton = match self.config.automatons.iter().find(|a| &a.id == added_id) {
                Some(a) => a.clone(),
                None => continue,
            };
            add_automaton_chat(self, &automaton);
            outcome.added += 1;
        }

        outcome
    }

    fn find_agent_idx_by_id(&self, id: &str) -> Option<usize> {
        self.chats.iter().position(|c| {
            !c.hibernated && matches!(c.kind, super::app::ChatKind::Agent) && c.display_name == id
        })
    }

    fn find_automaton_idx_by_id(&self, id: &str) -> Option<usize> {
        self.chats.iter().position(|c| {
            !c.hibernated
                && matches!(c.kind, super::app::ChatKind::Automaton)
                && c.display_name == id
        })
    }

    fn hibernate_chat_slot(&mut self, idx: usize, note: &str) {
        if let Some(chat) = self.chats.get_mut(idx) {
            if let Some(tx) = chat.cancel_tx.take() {
                let _ = tx.try_send(());
            }
            chat.user_tx = None;
            chat.agent_tx = None;
            chat.channel_tx = None;
            if let Some(old_session) = chat.session_id.take() {
                self.agent_ids.remove(&old_session);
            }
            chat.status = AgentStatus::Hibernated;
            chat.hibernated = true;
            chat.end_working();
            chat.push_block(ChatBlock::system_warn(note.to_string()));
        }
    }

    fn push_top_warning(&mut self, msg: String) {
        if let Some(chat) = self.chats.first_mut() {
            chat.push_block(ChatBlock::system_warn(msg));
        }
    }
}

// --- Plans ----------------------------------------------------------

#[derive(Debug, Default)]
struct AgentReloadPlan {
    removed_ids: Vec<String>,
    added_ids: Vec<String>,
    changed: Vec<ChangedAgent>,
}

#[derive(Debug)]
struct ChangedAgent {
    id: String,
    action: ReloadAction,
}

impl AgentReloadPlan {
    fn compute(old: &AciddogConfig, new: &AciddogConfig) -> Self {
        let mut plan = AgentReloadPlan::default();
        for old_a in &old.agents {
            if !new.agents.iter().any(|n| n.id == old_a.id) {
                plan.removed_ids.push(old_a.id.clone());
            }
        }
        for new_a in &new.agents {
            match old.agents.iter().find(|o| o.id == new_a.id) {
                None => plan.added_ids.push(new_a.id.clone()),
                Some(old_a) => {
                    if agent_effectively_changed(old, new, old_a, new_a) {
                        plan.changed.push(ChangedAgent {
                            id: new_a.id.clone(),
                            action: new_a.on_reload,
                        });
                    }
                }
            }
        }
        plan
    }
}

#[derive(Debug, Default)]
struct AutomatonReloadPlan {
    removed_ids: Vec<String>,
    added_ids: Vec<String>,
    restarted: Vec<RestartedAutomaton>,
}

#[derive(Debug)]
struct RestartedAutomaton {
    id: String,
}

impl AutomatonReloadPlan {
    fn compute(old: &AciddogConfig, new: &AciddogConfig) -> Self {
        let mut plan = AutomatonReloadPlan::default();
        for old_a in &old.automatons {
            if !new.automatons.iter().any(|n| n.id == old_a.id) {
                plan.removed_ids.push(old_a.id.clone());
            }
        }
        for new_a in &new.automatons {
            match old.automatons.iter().find(|o| o.id == new_a.id) {
                None => plan.added_ids.push(new_a.id.clone()),
                Some(old_a) => {
                    if automaton_or_webhooks_changed(old, new, old_a, new_a) {
                        plan.restarted.push(RestartedAutomaton {
                            id: new_a.id.clone(),
                        });
                    }
                }
            }
        }
        plan
    }
}

#[derive(Debug, Default)]
struct ChannelReloadPlan {
    removed_ids: Vec<String>,
    added: Vec<crate::config::Channel>,
    member_updates: Vec<(String, Vec<String>)>,
}

impl ChannelReloadPlan {
    fn compute(old: &AciddogConfig, new: &AciddogConfig) -> Self {
        let mut plan = ChannelReloadPlan::default();
        for old_c in &old.channels {
            if !new.channels.iter().any(|n| n.id == old_c.id) {
                plan.removed_ids.push(old_c.id.clone());
            }
        }
        for new_c in &new.channels {
            match old.channels.iter().find(|o| o.id == new_c.id) {
                None => plan.added.push(new_c.clone()),
                Some(old_c) => {
                    if old_c.members != new_c.members {
                        plan.member_updates
                            .push((new_c.id.clone(), new_c.members.clone()));
                    }
                }
            }
        }
        plan
    }
}

#[derive(Debug, Default)]
struct AgentOutcome {
    added: usize,
    removed: usize,
    changed: usize,
    action_ignored: usize,
    action_compact: usize,
    action_reset: usize,
}

impl AgentOutcome {
    fn action_breakdown(&self) -> String {
        format!(
            "ignore={} compact={} reset={}",
            self.action_ignored, self.action_compact, self.action_reset
        )
    }
}

#[derive(Debug, Default)]
struct ChannelOutcome {
    added: usize,
    removed_warned: usize,
    members_updated: usize,
}

#[derive(Debug, Default)]
struct AutomatonOutcome {
    added: usize,
    removed: usize,
    restarted: usize,
}

// --- Deep-compare helpers -------------------------------------------

// NOTE on the leader-flag: `Agent` and `Automaton` both derive
// `PartialEq` over their full struct (including the new `leader`
// field), so flipping `leader = true/false` in the WCL and running
// `/reload` lands in the `old_a != new_a` branch below. The TUI's
// `App::visible_order` helper recomputes leader-bucket partitioning
// on every render tick from the latest `App::config`, so the agent
// list re-sorts on the next redraw with no extra plumbing required.

/// Did the agent's *effective* runtime config change between two
/// loads? The agent struct itself is one input, but the agent's
/// behaviour also depends on the resolved ai_core, compaction,
/// memory_banks, skills, and channel membership — any of which can
/// change without the `agent` block being touched. Walking the
/// transitive deps means a `:reset` action correctly fires when, say,
/// the agent's referenced ai_core block has its model bumped.
fn agent_effectively_changed(
    old: &AciddogConfig,
    new: &AciddogConfig,
    old_a: &Agent,
    new_a: &Agent,
) -> bool {
    if old_a != new_a {
        return true;
    }
    if old.effective_ai_core(old_a.ai_core.as_deref())
        != new.effective_ai_core(new_a.ai_core.as_deref())
    {
        return true;
    }
    let old_core = old.effective_ai_core(old_a.ai_core.as_deref());
    let new_core = new.effective_ai_core(new_a.ai_core.as_deref());
    if old.effective_tools(old_a.tools.as_deref(), old_core)
        != new.effective_tools(new_a.tools.as_deref(), new_core)
    {
        return true;
    }
    if old.effective_model(old_a.model.as_deref(), old_core)
        != new.effective_model(new_a.model.as_deref(), new_core)
    {
        return true;
    }
    if old.compaction_for(&old_a.id) != new.compaction_for(&new_a.id) {
        return true;
    }
    let old_banks: Vec<_> = old
        .memory_banks_for(&old_a.id)
        .into_iter()
        .cloned()
        .collect();
    let new_banks: Vec<_> = new
        .memory_banks_for(&new_a.id)
        .into_iter()
        .cloned()
        .collect();
    if old_banks != new_banks {
        return true;
    }
    if old.skills_for(&old_a.id) != new.skills_for(&new_a.id) {
        return true;
    }
    let old_channels: Vec<_> = old
        .channels_for_agent(&old_a.id)
        .into_iter()
        .cloned()
        .collect();
    let new_channels: Vec<_> = new
        .channels_for_agent(&new_a.id)
        .into_iter()
        .cloned()
        .collect();
    if old_channels != new_channels {
        return true;
    }
    false
}

/// An automaton needs a driver restart if its own block changed OR
/// its filtered webhook list changed (webservice automatons bake the
/// route table into the listener at spawn time, so a webhook delta
/// has to bounce the driver).
fn automaton_or_webhooks_changed(
    old: &AciddogConfig,
    new: &AciddogConfig,
    old_a: &Automaton,
    new_a: &Automaton,
) -> bool {
    if old_a != new_a {
        return true;
    }
    let old_hooks: Vec<&Webhook> = old
        .webhooks
        .iter()
        .filter(|w| w.mount == old_a.id)
        .collect();
    let new_hooks: Vec<&Webhook> = new
        .webhooks
        .iter()
        .filter(|w| w.mount == new_a.id)
        .collect();
    if old_hooks.len() != new_hooks.len() {
        return true;
    }
    for (o, n) in old_hooks.iter().zip(new_hooks.iter()) {
        if o != n {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_from(src: &str) -> AciddogConfig {
        crate::config::parse_config(src, std::path::Path::new("<test>")).expect("parse ok")
    }

    const TWO_AGENTS: &str = r#"
        team default { description = "t" }
        agent alpha { role = "r" instructions = "i" }
        agent bravo { role = "r" instructions = "i" }
    "#;

    #[test]
    fn agent_plan_detects_added_and_removed() {
        let old = cfg_from(TWO_AGENTS);
        let new = cfg_from(
            r#"
            team default { description = "t" }
            agent alpha { role = "r" instructions = "i" }
            agent charlie { role = "r" instructions = "i" }
            "#,
        );
        let plan = AgentReloadPlan::compute(&old, &new);
        assert_eq!(plan.removed_ids, vec!["bravo".to_string()]);
        assert_eq!(plan.added_ids, vec!["charlie".to_string()]);
        assert!(plan.changed.is_empty());
    }

    #[test]
    fn agent_plan_carries_per_agent_action() {
        let old = cfg_from(TWO_AGENTS);
        let new = cfg_from(
            r#"
            team default { description = "t" }
            agent alpha {
                role = "r"
                instructions = "now different"
                on_reload = :compact
            }
            agent bravo {
                role = "r"
                instructions = "also different"
                on_reload = :reset
            }
            "#,
        );
        let plan = AgentReloadPlan::compute(&old, &new);
        assert!(plan.removed_ids.is_empty());
        assert!(plan.added_ids.is_empty());
        let by_id: std::collections::HashMap<_, _> = plan
            .changed
            .iter()
            .map(|c| (c.id.as_str(), c.action))
            .collect();
        assert_eq!(by_id.get("alpha"), Some(&ReloadAction::Compact));
        assert_eq!(by_id.get("bravo"), Some(&ReloadAction::Reset));
    }

    #[test]
    fn agent_plan_skips_unchanged_agents() {
        let cfg = cfg_from(TWO_AGENTS);
        let plan = AgentReloadPlan::compute(&cfg, &cfg);
        assert!(plan.removed_ids.is_empty());
        assert!(plan.added_ids.is_empty());
        assert!(plan.changed.is_empty());
    }

    #[test]
    fn agent_change_is_detected_when_referenced_compaction_block_changes() {
        // Agent itself unchanged, but referenced compaction body
        // differs — deep-compare must catch this so a `:compact`
        // on_reload actually runs.
        let old = cfg_from(
            r#"
            team default { description = "t" }
            compaction c1 { strategy = :promptAndDrop  prompt = "old" }
            agent alpha { role = "r" instructions = "i" compaction = "c1" on_reload = :compact }
            "#,
        );
        let new = cfg_from(
            r#"
            team default { description = "t" }
            compaction c1 { strategy = :promptAndDrop  prompt = "new prompt" }
            agent alpha { role = "r" instructions = "i" compaction = "c1" on_reload = :compact }
            "#,
        );
        let plan = AgentReloadPlan::compute(&old, &new);
        assert_eq!(plan.changed.len(), 1);
        assert_eq!(plan.changed[0].id, "alpha");
    }

    #[test]
    fn channel_plan_handles_add_remove_and_member_update() {
        let old = cfg_from(
            r#"
            team default { description = "t" }
            agent alpha { role = "r" instructions = "i" }
            agent bravo { role = "r" instructions = "i" }
            channel general { members = ["alpha", "bravo"] }
            channel deprecated { members = ["alpha"] }
            "#,
        );
        let new = cfg_from(
            r#"
            team default { description = "t" }
            agent alpha { role = "r" instructions = "i" }
            agent bravo { role = "r" instructions = "i" }
            channel general { members = ["alpha"] }
            channel newroom { members = ["bravo"] }
            "#,
        );
        let plan = ChannelReloadPlan::compute(&old, &new);
        assert_eq!(plan.removed_ids, vec!["deprecated".to_string()]);
        assert_eq!(plan.added.len(), 1);
        assert_eq!(plan.added[0].id, "newroom");
        assert_eq!(
            plan.member_updates,
            vec![("general".to_string(), vec!["alpha".to_string()])]
        );
    }
}
