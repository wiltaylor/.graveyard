//! `/compact` and `/reset` implementation.
//!
//! Four strategies are supported (declared per-agent via a `compaction`
//! block in `aciddog.wcl`); `/reset` is strategy-independent and always
//! runs `Drop`. See `CompactionStrategy` in `src/config.rs` for the
//! authoritative semantics.

use std::sync::Arc;

use futures::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::agent_ipc;
use crate::ai::{AICore, AIEvent, AciddogCliCtx, PromptParts};
use crate::config::{AiCore, CompactionStrategy};

use super::app::{
    build_core, build_memory_block, build_team_block, context_window_for_core, core_display_label,
    core_usage_mode, run_core, uses_tag_ipc, AgentChat, AgentStatus, App,
};
use super::blocks::ChatBlock;

/// Work scheduled to happen after the agent's current turn finishes.
///
/// `promptAndDrop` queues a final prompt through `user_tx` and then
/// sets this so `handle_ai_event` can catch the `TurnResult` and run
/// the teardown once the reply has been rendered. `summarise` takes a
/// similar shape but carries a `seed` string to enqueue as the first
/// prompt on the rebuilt session.
#[derive(Debug, Clone)]
pub enum PendingCompaction {
    /// Tear down and rebuild once the current turn completes. No seed
    /// prompt is delivered on the new session.
    DropAfterTurn { note: String },
    /// Tear down, rebuild, then enqueue `seed` as the first user
    /// message of the new session (used by `summarise` after the
    /// throwaway summary core has returned).
    DropAndSeed { note: String, seed: String },
}

/// Result bus from background summarise tasks back into the main
/// event loop. Wired up the same way as `ai_tx` — event_loop drains
/// it and dispatches to `handle_compaction_event`.
#[derive(Debug)]
pub enum CompactionEvent {
    /// Summary produced successfully. `session_id` is the agent's
    /// session at the time the summary was kicked off; used to
    /// suppress stale results if the user has already re-run /compact
    /// or the agent was otherwise rebuilt.
    SummaryReady {
        idx: usize,
        session_id: Uuid,
        note: String,
        summary: String,
    },
    /// Summary failed. We still drop + rebuild (user clearly wanted
    /// the session reset) but skip the seed step and show the error.
    SummaryFailed {
        idx: usize,
        session_id: Uuid,
        error: String,
    },
}

/// One entry in the transcript JSON blob handed to the summariser
/// core. Keeps the payload schema stable across core variants.
#[derive(Debug, Serialize)]
struct TranscriptEntry {
    role: &'static str,
    text: String,
}

impl App {
    /// Entry point for `/compact` (`force_drop = false`) and `/reset`
    /// (`force_drop = true`). All validation happens here; the real
    /// work is delegated to strategy-specific helpers.
    pub(super) fn begin_compaction(&mut self, idx: usize, force_drop: bool) {
        let Some(chat) = self.chats.get(idx) else {
            warn!(idx, "compaction requested on unknown chat");
            return;
        };
        if chat.hibernated {
            self.warn(idx, "agent is hibernated — nothing to compact");
            return;
        }
        if chat.worker_template.is_some() {
            // Workers are ephemeral by design. Resetting them would
            // lose the task context their parent spawned them with,
            // since aciddog doesn't store that task text anywhere
            // retrievable. Forbid for now.
            self.warn(
                idx,
                "compaction is not supported on workers — complete or message them instead",
            );
            return;
        }

        // Resolve the strategy. `/reset` pins to Drop; `/compact` uses
        // the agent's configured policy and falls back to Drop when
        // unconfigured.
        let agent_id = chat.display_name.clone();
        let strategy = if force_drop {
            CompactionStrategy::Drop
        } else {
            self.config
                .compaction_for(&agent_id)
                .map(|c| c.strategy.clone())
                .unwrap_or(CompactionStrategy::Drop)
        };

        let label = if force_drop { "reset" } else { strategy.name() };
        info!(idx, %agent_id, strategy = strategy.name(), force_drop, "begin compaction");

        match strategy {
            CompactionStrategy::Harness => self.compact_harness(idx, label),
            CompactionStrategy::Drop => self.compact_drop(idx, label),
            CompactionStrategy::PromptAndDrop { prompt } => {
                self.compact_prompt_and_drop(idx, label, prompt)
            }
            CompactionStrategy::Summarise { prompt, ai_core } => {
                self.compact_summarise(idx, label, prompt, ai_core)
            }
        }
    }

    /// Called from `handle_ai_event` on `TurnResult` when a chat has a
    /// pending compaction waiting for the current turn to finish.
    pub(super) fn finish_pending_compaction(&mut self, idx: usize) {
        let Some(chat) = self.chats.get_mut(idx) else {
            return;
        };
        let Some(pending) = chat.pending_compaction.take() else {
            return;
        };
        match pending {
            PendingCompaction::DropAfterTurn { note } => {
                if let Err(e) = self.teardown_and_rebuild(idx, None) {
                    self.error(idx, &format!("compaction failed: {e}"));
                    return;
                }
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.reset_to_init(note);
                }
            }
            PendingCompaction::DropAndSeed { note, seed } => {
                if let Err(e) = self.teardown_and_rebuild(idx, Some(seed)) {
                    self.error(idx, &format!("compaction failed: {e}"));
                    return;
                }
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.reset_to_init(note);
                }
            }
        }
    }

    /// Dispatcher for `CompactionEvent`s arriving from the summarise
    /// background task.
    pub(crate) fn handle_compaction_event(&mut self, ev: CompactionEvent) {
        match ev {
            CompactionEvent::SummaryReady {
                idx,
                session_id,
                note,
                summary,
            } => {
                // Stale-result guard: if the chat was rebuilt or the
                // user ran /reset while the summary was in flight, the
                // session_id no longer matches — drop the result.
                let current = self.chats.get(idx).and_then(|c| c.session_id);
                if current != Some(session_id) {
                    warn!(
                        idx,
                        expected = %session_id,
                        current = ?current,
                        "dropping stale summary result"
                    );
                    return;
                }
                // Queue the teardown so it fires on the next
                // TurnResult (i.e. once whatever turn the user
                // started after typing /compact finishes — normally
                // there is no such turn, but be defensive).
                if let Some(chat) = self.chats.get_mut(idx) {
                    if chat.status == AgentStatus::Working {
                        chat.pending_compaction = Some(PendingCompaction::DropAndSeed {
                            note,
                            seed: summary,
                        });
                        return;
                    }
                }
                if let Err(e) = self.teardown_and_rebuild(idx, Some(summary)) {
                    self.error(idx, &format!("compaction failed: {e}"));
                    return;
                }
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.reset_to_init(note);
                }
            }
            CompactionEvent::SummaryFailed {
                idx,
                session_id,
                error,
            } => {
                let current = self.chats.get(idx).and_then(|c| c.session_id);
                if current != Some(session_id) {
                    return;
                }
                // Summary failed, but the user asked for a compaction,
                // so still drop the session rather than pretending
                // nothing happened.
                if let Err(e) = self.teardown_and_rebuild(idx, None) {
                    self.error(idx, &format!("compaction failed after summary error: {e}"));
                    return;
                }
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.reset_to_init(format!(
                        "session compacted (summarise failed: {error}) — dropped without summary"
                    ));
                }
            }
        }
    }

    // --- strategy implementations ----------------------------------

    fn compact_harness(&mut self, idx: usize, label: &str) {
        let Some(chat) = self.chats.get(idx) else {
            return;
        };
        let Some(tx) = chat.user_tx.as_ref().cloned() else {
            self.warn(idx, "agent is offline — cannot forward /compact");
            return;
        };
        let pool_len = self.config.thinking_messages.len();
        if tx.send("/compact".to_string()).is_err() {
            self.warn(idx, "agent inbox closed — /compact not delivered");
            return;
        }
        if let Some(chat) = self.chats.get_mut(idx) {
            // Clear the TUI transcript down to the AgentInfo header.
            // The harness still holds the old session in its own store;
            // if it runs its native compaction the effect is the same
            // as our other strategies, and if it doesn't the user at
            // least sees the explicit marker rather than a stale
            // history.
            chat.reset_to_init(format!(
                "session compacted via `{label}` — /compact forwarded to the harness"
            ));
            chat.begin_working(pool_len);
        }
    }

    fn compact_drop(&mut self, idx: usize, label: &str) {
        if !self.ensure_idle_for_compaction(idx) {
            return;
        }
        if let Err(e) = self.teardown_and_rebuild(idx, None) {
            self.error(idx, &format!("compaction failed: {e}"));
            return;
        }
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.reset_to_init(format!("session compacted via `{label}`"));
        }
    }

    fn compact_prompt_and_drop(&mut self, idx: usize, label: &str, prompt: String) {
        let Some(chat) = self.chats.get_mut(idx) else {
            return;
        };
        if chat.status == AgentStatus::Working {
            self.warn(idx, "agent is busy — retry /compact when idle");
            return;
        }
        let Some(tx) = chat.user_tx.as_ref().cloned() else {
            self.warn(idx, "agent is offline — cannot run promptAndDrop");
            return;
        };
        let pool_len = self.config.thinking_messages.len();
        if tx.send(prompt).is_err() {
            self.warn(idx, "agent inbox closed — promptAndDrop aborted");
            return;
        }
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.begin_working(pool_len);
            chat.pending_compaction = Some(PendingCompaction::DropAfterTurn {
                note: format!("session compacted via `{label}`"),
            });
        }
    }

    fn compact_summarise(
        &mut self,
        idx: usize,
        label: &str,
        prompt: String,
        ai_core_override: Option<String>,
    ) {
        if !self.ensure_idle_for_compaction(idx) {
            return;
        }
        let Some(compaction_tx) = self.compaction_tx.clone() else {
            self.error(idx, "orchestrator has no compaction channel");
            return;
        };

        // Snapshot what we'll feed the summariser before anything else
        // moves — the rebuilt chat will be empty by the time the
        // summary arrives.
        let Some(chat) = self.chats.get(idx) else {
            return;
        };
        let Some(session_id) = chat.session_id else {
            self.error(idx, "agent has no session id");
            return;
        };
        let transcript = snapshot_transcript(chat);
        let transcript_json = match serde_json::to_string_pretty(&transcript) {
            Ok(s) => s,
            Err(e) => {
                self.error(idx, &format!("failed to serialise transcript: {e}"));
                return;
            }
        };

        // Pick the core the summary pass will run through.
        let agent_id = chat.display_name.clone();
        let summary_core = match self.resolve_summary_core(&agent_id, ai_core_override.as_deref()) {
            Ok(c) => c.clone(),
            Err(e) => {
                self.error(idx, &format!("cannot resolve summariser core: {e}"));
                return;
            }
        };

        let note = format!("session compacted via `{label}`");
        let full_prompt = format!("{prompt}\n\n<transcript>\n{transcript_json}\n</transcript>");
        let env = self.env_for_summary();

        let tx = compaction_tx;
        tokio::spawn(async move {
            let summary_session = Uuid::new_v4();
            let mut core = match build_core(
                &summary_core,
                summary_session,
                summariser_prompt_parts(),
                Vec::new(),
                env,
                None,
                false,
                None,
                None,
                None,
            ) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx
                        .send(CompactionEvent::SummaryFailed {
                            idx,
                            session_id,
                            error: format!("summariser core build failed: {e}"),
                        })
                        .await;
                    return;
                }
            };
            let text_result = run_one_shot_turn(&mut *core, &full_prompt).await;
            let ev = match text_result {
                Ok(summary) if !summary.trim().is_empty() => CompactionEvent::SummaryReady {
                    idx,
                    session_id,
                    note,
                    summary,
                },
                Ok(_) => CompactionEvent::SummaryFailed {
                    idx,
                    session_id,
                    error: "summariser returned empty output".to_string(),
                },
                Err(e) => CompactionEvent::SummaryFailed {
                    idx,
                    session_id,
                    error: e,
                },
            };
            let _ = tx.send(ev).await;
        });

        if let Some(chat) = self.chats.get_mut(idx) {
            chat.push_block(ChatBlock::system_info(
                "summarising session — new session will be seeded with the digest",
            ));
        }
    }

    // --- shared helpers --------------------------------------------

    fn ensure_idle_for_compaction(&mut self, idx: usize) -> bool {
        let Some(chat) = self.chats.get(idx) else {
            return false;
        };
        if chat.status == AgentStatus::Working {
            self.warn(idx, "agent is busy — retry /compact when idle");
            return false;
        }
        true
    }

    fn warn(&mut self, idx: usize, msg: &str) {
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.push_block(ChatBlock::system_warn(msg.to_string()));
        }
    }

    fn error(&mut self, idx: usize, msg: &str) {
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.push_block(ChatBlock::system_error(msg.to_string()));
        }
    }

    fn env_for_summary(&self) -> Vec<(String, String)> {
        vec![(
            agent_ipc::ENV_AGENT_SOCKET.to_string(),
            self.socket_path.clone(),
        )]
    }

    fn resolve_summary_core(
        &self,
        agent_id: &str,
        override_id: Option<&str>,
    ) -> Result<&AiCore, String> {
        if let Some(id) = override_id {
            return self
                .config
                .ai_cores
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| format!("ai_core `{id}` not declared"));
        }
        let agent_override = self
            .config
            .agents
            .iter()
            .find(|a| a.id == agent_id)
            .and_then(|a| a.ai_core.as_deref());
        self.config
            .effective_ai_core(agent_override)
            .ok_or_else(|| "no ai_core configured (check team.default_ai_core)".to_string())
    }

    /// Teardown the running agent and spawn a fresh session. Mirrors
    /// the non-worker spawn loop in `App::run` — new session id, new
    /// `user_tx`/`agent_tx` channels, new `run_core` task. `seed`, if
    /// present, is enqueued on the new `user_tx` before returning, so
    /// the rebuilt core immediately starts a turn on it.
    fn teardown_and_rebuild(&mut self, idx: usize, seed: Option<String>) -> Result<(), String> {
        let Some(ai_tx) = self.ai_tx.clone() else {
            return Err("orchestrator has no ai_tx".into());
        };
        let agent = {
            let chat = self
                .chats
                .get(idx)
                .ok_or_else(|| "unknown chat".to_string())?;
            self.config
                .agents
                .iter()
                .find(|a| a.id == chat.display_name)
                .cloned()
                .ok_or_else(|| format!("no agent config for `{}`", chat.display_name))?
        };

        let ai_core = self
            .config
            .effective_ai_core(agent.ai_core.as_deref())
            .ok_or_else(|| format!("agent `{}` has no ai_core", agent.id))?
            .clone();
        let tools = self
            .config
            .effective_tools(agent.tools.as_deref(), Some(&ai_core));
        let tag_mode = uses_tag_ipc(&tools, &ai_core);

        let session_id = Uuid::new_v4();

        // Build the same PromptParts the original spawn produces so
        // the rebuilt core emits the full first-turn preamble.
        let all_ids: Vec<String> = self.config.all_addressable_ids();
        let contactable: Vec<&str> = match &agent.can_message {
            Some(list) => list.iter().map(String::as_str).collect(),
            None => all_ids
                .iter()
                .filter(|id| id.as_str() != agent.id.as_str())
                .map(String::as_str)
                .collect(),
        };
        let creatable: Vec<&str> = agent
            .can_create_workers
            .as_deref()
            .map(|list| list.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let team_block = build_team_block(&self.config, &contactable, &creatable, true);
        let memory_block = build_memory_block(&self.config, &agent.id);
        let memory_banks_for_agent: Vec<String> = agent
            .memory_banks
            .as_deref()
            .map(|list| list.to_vec())
            .unwrap_or_default();
        let skills_for_agent = self.config.skills_for(&agent.id);
        let channels_for_agent: Vec<crate::ai::ChannelPromptCtx> = self
            .config
            .channels_for_agent(&agent.id)
            .iter()
            .map(|c| crate::ai::ChannelPromptCtx {
                id: c.id.clone(),
                description: c.description.clone(),
                other_members: c
                    .members
                    .iter()
                    .filter(|m| m.as_str() != agent.id.as_str())
                    .cloned()
                    .collect(),
            })
            .collect();

        let parts = PromptParts {
            role: agent.role_description.clone(),
            team: Some(team_block),
            memory: memory_block,
            instructions: agent.instructions.clone(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: self.aciddog_binary.clone(),
                contactable: contactable.iter().map(|s| s.to_string()).collect(),
                creatable: creatable.iter().map(|s| s.to_string()).collect(),
                memory_banks: memory_banks_for_agent,
                skills: skills_for_agent,
                channels: channels_for_agent,
                worker: None,
                supervisor: agent.supervisor,
                sysadmin: agent.sysadmin,
            }),
        };

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                self.socket_path.clone(),
            ),
        ];

        let socket_tx = if tag_mode {
            self.socket_events_tx.clone()
        } else {
            None
        };

        let core = build_core(
            &ai_core,
            session_id,
            parts,
            tools.clone(),
            env,
            socket_tx,
            false,
            agent.model.as_deref(),
            None,
            // Compaction only runs on configured agents, never on
            // workers — so per-spawn properties are never applicable.
            None,
        )
        .map_err(|e| format!("build_core failed during compaction rebuild: {e}"))?;
        let assembled_prompt = core.first_turn_prompt();

        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);
        // Rebuilt channel queue so the agent keeps receiving channel
        // broadcasts after `/reset` / `/compact`. Workers never reach
        // this path (compaction is gated on `!hibernated` and non-
        // worker), but we make the queue unconditionally to keep the
        // spawn API uniform.
        let (channel_tx, channel_rx) = mpsc::channel::<String>(256);
        // Fresh cancel channel for the new run_core task. Capacity 1:
        // see the static-agent spawn in `run` for the rationale.
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);

        // If there's a seed message, enqueue it now so it's the first
        // prompt the new core sees.
        if let Some(text) = seed.as_ref() {
            if user_tx.send(text.clone()).is_err() {
                return Err("failed to enqueue seed prompt".into());
            }
        }

        let pool_len = self.config.thinking_messages.len();
        let new_label = core_display_label(&ai_core);
        let new_usage_mode = core_usage_mode(&ai_core.kind);
        let new_context_window = context_window_for_core(
            self.config
                .effective_model(agent.model.as_deref(), Some(&ai_core)),
            Some(&ai_core),
        );

        // Remove the old session id from the routing map *before* we
        // stamp the new one so a stray inbound socket request can't
        // land on the old chat.
        if let Some(chat) = self.chats.get(idx) {
            if let Some(old_id) = chat.session_id {
                self.agent_ids.remove(&old_id);
            }
        }

        // Swap channels & session id. Dropping the old `user_tx` /
        // `agent_tx` causes the old `run_core` task to exit on its
        // next select — safe because `ensure_idle_for_compaction`
        // already confirmed no turn is in flight.
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.user_tx = Some(user_tx);
            chat.agent_tx = Some(agent_tx);
            chat.channel_tx = Some(channel_tx);
            chat.cancel_tx = Some(cancel_tx);
            chat.session_id = Some(session_id);
            chat.status = AgentStatus::Ready;
            chat.context_used = None;
            chat.context_usage_mode = new_usage_mode;
            chat.context_window = new_context_window;
            chat.pending_compaction = None;

            // The first (AgentInfo) block is what `reset_to_init`
            // keeps; refresh its rendered prompt + core label so the
            // expanded view shows the new session's preamble.
            if let Some(ChatBlock::AgentInfo {
                prompt,
                tools: bt,
                core,
                context_window,
                ..
            }) = chat.blocks.first_mut()
            {
                *prompt = assembled_prompt;
                *bt = tools;
                *core = new_label;
                *context_window = new_context_window;
            }

            if seed.is_some() {
                chat.begin_working(pool_len);
            }
        }

        self.agent_ids.insert(session_id, agent.id.clone());

        tokio::spawn(run_core(
            idx, core, user_rx, agent_rx, channel_rx, cancel_rx, ai_tx,
        ));
        Ok(())
    }
}

fn summariser_prompt_parts() -> PromptParts {
    // Throwaway core used for one turn only. We keep the preamble
    // minimal so the summariser's output isn't polluted by
    // `<role>` / `<team>` / `<tools>` boilerplate; the actual
    // summary directive lives in the user prompt.
    PromptParts {
        role: None,
        team: None,
        memory: None,
        instructions: String::new(),
        aciddog_cli: None,
    }
}

async fn run_one_shot_turn(core: &mut dyn AICore, prompt: &str) -> Result<String, String> {
    let mut stream = core.turn(prompt).await.map_err(|e| e.to_string())?;
    let buf = Arc::new(std::sync::Mutex::new(String::new()));
    while let Some(ev) = stream.next().await {
        match ev {
            AIEvent::AssistantText { text, .. } => {
                let mut g = buf.lock().expect("one-shot buf poisoned");
                g.push_str(&text);
            }
            AIEvent::Error { message, .. } => return Err(message),
            AIEvent::TurnResult { .. } => break,
            _ => {}
        }
    }
    let out = buf.lock().expect("one-shot buf poisoned").clone();
    Ok(out)
}

/// Walk the chat's blocks and produce an ordered transcript suitable
/// for JSON serialisation. Keeps user text and AI markdown; drops
/// tool noise, system banners, and aciddog-internal blocks, since
/// they add bulk without summariser value.
fn snapshot_transcript(chat: &AgentChat) -> Vec<TranscriptEntry> {
    let mut out = Vec::new();
    for block in &chat.blocks {
        match block {
            ChatBlock::UserText { text, .. } => out.push(TranscriptEntry {
                role: "user",
                text: text.clone(),
            }),
            ChatBlock::AiMarkdown { raw, .. } => out.push(TranscriptEntry {
                role: "assistant",
                text: raw.clone(),
            }),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::blocks::ChatBlock;

    fn chat_with_blocks(blocks: Vec<ChatBlock>) -> AgentChat {
        AgentChat {
            blocks,
            ..Default::default()
        }
    }

    #[test]
    fn transcript_keeps_user_and_ai_only_in_order() {
        let chat = chat_with_blocks(vec![
            ChatBlock::user("first user"),
            ChatBlock::ai_markdown("first reply"),
            ChatBlock::system_info("toolbar noise"),
            ChatBlock::user("second user"),
            ChatBlock::ai_markdown("second reply"),
        ]);
        let t = snapshot_transcript(&chat);
        assert_eq!(t.len(), 4);
        assert_eq!(t[0].role, "user");
        assert_eq!(t[0].text, "first user");
        assert_eq!(t[1].role, "assistant");
        assert_eq!(t[1].text, "first reply");
        assert_eq!(t[2].role, "user");
        assert_eq!(t[3].role, "assistant");
    }
}
