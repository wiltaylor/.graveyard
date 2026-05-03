//! Google Jules remote-worker core. Unlike the local cores, the AI run
//! happens off-box on jules.google; this core is a thin client that:
//!
//! 1. Creates a Jules session (POST `/v1alpha/sessions`) on the first
//!    turn, using `properties.repo` / `properties.branch` for source
//!    context.
//! 2. Polls `/v1alpha/sessions/{name}/activities` + `/sessions/{name}`
//!    on a jittered interval while Jules runs the task.
//! 3. Translates each new activity into an `AIEvent` so the worker's
//!    chat pane stays in sync (agent text, progress, bash output,
//!    change sets, plan steps).
//! 4. On state transitions that require external input, sends a mail
//!    to the parent via `socket_tx` and ends the current turn's stream
//!    so the worker goes `Ready`. Parent reply drives the next turn:
//!    body trimmed + lowercased == `"approve"` → POST `:approvePlan`;
//!    anything else → POST `:sendMessage`.
//! 5. On terminal states (`COMPLETED`/`FAILED`) emits
//!    `RequestCmd::WorkerComplete` via `socket_tx` — reusing the
//!    existing hibernation path — and sets `remote_terminal = true`
//!    via a sentinel send so `handle_send_message` rejects further
//!    attempts to resume the dead Jules session.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::agent_ipc::preamble::{tag_preamble, tag_worker_preamble};
use crate::agent_ipc::{AgentSocketEvent, Request, RequestCmd, Response};
use crate::ai::core::{AICore, AIError};
use crate::ai::event::{AIEvent, CodexFailureKind};

const JULES_BASE_URL: &str = "https://jules.googleapis.com/v1alpha";
const DEFAULT_POLL_SECONDS: u64 = 10;
const MIN_POLL_SECONDS: u64 = 3;
const MAX_POLL_SECONDS: u64 = 120;
const MAX_BACKOFF_SECONDS: u64 = 60;
/// Hard ceiling on pagination traversal per poll. Jules' activity
/// list caps at pageSize=100; a single session producing more than
/// ~10k activities between polls is pathological — stop and pick
/// the rest up on the next cycle rather than hang the poll forever.
const MAX_PAGES_PER_POLL: u32 = 100;

/// Per-session Jules state. A subset of the documented enum — we
/// normalise unknown strings to `Unknown` so a new Jules state doesn't
/// crash the core; it just keeps polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JulesState {
    Unspecified,
    Queued,
    Planning,
    AwaitingPlanApproval,
    AwaitingUserFeedback,
    InProgress,
    Paused,
    Failed,
    Completed,
    Unknown,
}

impl JulesState {
    fn parse(s: &str) -> Self {
        match s {
            "STATE_UNSPECIFIED" => Self::Unspecified,
            "QUEUED" => Self::Queued,
            "PLANNING" => Self::Planning,
            "AWAITING_PLAN_APPROVAL" => Self::AwaitingPlanApproval,
            "AWAITING_USER_FEEDBACK" => Self::AwaitingUserFeedback,
            "IN_PROGRESS" => Self::InProgress,
            "PAUSED" => Self::Paused,
            "FAILED" => Self::Failed,
            "COMPLETED" => Self::Completed,
            _ => Self::Unknown,
        }
    }

    fn is_terminal_for_turn(self) -> bool {
        matches!(
            self,
            Self::AwaitingPlanApproval
                | Self::AwaitingUserFeedback
                | Self::Paused
                | Self::Failed
                | Self::Completed
        )
    }

    fn stop_reason(self) -> &'static str {
        match self {
            Self::AwaitingPlanApproval => "awaiting_plan_approval",
            Self::AwaitingUserFeedback => "awaiting_user_feedback",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Completed => "completed",
            _ => "in_progress",
        }
    }
}

/// Properties passed in by the caller at spawn time. Parsed from the
/// IPC `WorkerCreate.properties` JSON.
#[derive(Debug, Clone)]
pub struct JulesProperties {
    pub repo: String,
    pub branch: Option<String>,
    /// Per-spawn override for `automationMode`. `Some(true)` forces
    /// `AUTO_CREATE_PR` (Jules auto-opens the PR when changes are
    /// ready, no "should I open the PR?" round-trip); `Some(false)`
    /// forces `AUTOMATION_MODE_UNSPECIFIED`; `None` falls back to the
    /// `ai_core` block's `automationMode`.
    pub auto_create_pr: Option<bool>,
}

impl JulesProperties {
    pub fn parse(value: &Value, default_branch: Option<&str>) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "`properties` must be a JSON object".to_string())?;
        let repo = obj
            .get("repo")
            .and_then(Value::as_str)
            .ok_or_else(|| "`properties.repo` is required (expected \"owner/repo\")".to_string())?
            .trim()
            .to_string();
        if repo.is_empty() || !repo.contains('/') {
            return Err(format!(
                "`properties.repo` must be \"owner/repo\" (got `{repo}`)"
            ));
        }
        let branch = obj
            .get("branch")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| default_branch.map(str::to_string));
        let auto_create_pr = obj.get("autoCreatePr").and_then(Value::as_bool);
        Ok(Self {
            repo,
            branch,
            auto_create_pr,
        })
    }

    fn source_name(&self) -> String {
        // Jules source resource name convention, confirmed by live
        // `GET /sources` responses:
        //   sources/github-{owner}/{repo}
        // The `github-` prefix is bolted onto the owner segment; the
        // owner/repo separator stays a literal `/`. (The docs show
        // `sources/github-{owner}-{repo}` but that's misleading —
        // real connected sources come back with a `/` between owner
        // and repo, and the `-` form 404s.)
        format!("sources/github-{}", self.repo)
    }
}

/// Per-turn prompt built from the tag-only worker preamble. Injected
/// on the first turn only (via `first_turn_prompt`).
fn build_first_turn_preamble(parent_id: &str, task: &str) -> String {
    let mut out = String::new();
    out.push_str(&tag_worker_preamble(parent_id, task));
    // No contactable peers, no creatable workers, no memory banks,
    // no skills, no channels — remote workers run a single-shot
    // conversation with just their parent.
    // Remote workers are scoped to their parent — `user` is not a
    // valid recipient.
    // Remote workers never supervise.
    out.push_str(&tag_preamble(
        &[parent_id],
        &[],
        &[],
        &[],
        &[],
        false,
        false,
        false,
    ));
    out
}

pub struct JulesCore {
    session_uuid: Uuid,
    api_key_env: String,
    properties: JulesProperties,
    poll_interval: Duration,
    require_plan_approval: bool,
    automation_mode: String,
    parent_id: Option<String>,
    http: reqwest::Client,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    /// Jules session resource name ("sessions/..."). `None` until the
    /// first turn successfully creates a session.
    jules_session_name: Arc<Mutex<Option<String>>>,
    /// Latest known Jules state. Drives the approve-vs-reply branching
    /// on turn N+1.
    last_state: Arc<Mutex<JulesState>>,
    /// Activity ids already forwarded to the TUI. Jules' `/activities`
    /// endpoint uses AIP-158 pagination — `nextPageToken` walks the
    /// *existing* list, not "what's new since last poll". Using it as
    /// a cursor would re-emit every activity on every poll and flood
    /// the event channel, starving UI key events. We instead dedupe
    /// by the activity id (stable across polls) so each activity is
    /// emitted exactly once regardless of where pagination lands.
    seen_activity_ids: Arc<Mutex<HashSet<String>>>,
    /// Most recent plan text rendered from a `planGenerated` activity.
    /// Stashed so the mail we send to the parent on
    /// `AWAITING_PLAN_APPROVAL` can include the plan inline — otherwise
    /// the parent would have to switch to the worker's pane to read it
    /// before deciding whether to approve.
    latest_plan: Arc<Mutex<Option<String>>>,
    /// Most recent `agentMessaged` body. When Jules transitions to
    /// `AWAITING_USER_FEEDBACK` / `COMPLETED` / `FAILED` / `PAUSED`
    /// the last thing Jules said is almost always the question or
    /// summary the parent needs to act on (e.g. "should I open a
    /// PR?"). Without including it in the mail the parent LLM only
    /// sees the generic state transition and often guesses wrong —
    /// in practice, it archives when it should have answered.
    latest_agent_message: Arc<Mutex<Option<String>>>,
    /// `false` until the first turn has been dispatched (either at
    /// spawn or after resurrection). Matches `mark_started` semantics
    /// from AnthropicApiCore.
    started: bool,
    /// Cached first-turn preamble (includes the task). Assembled from
    /// `parent_id` + task on `new`, so `first_turn_prompt()` stays a
    /// cheap accessor and the TUI's AgentInfo block can surface the
    /// preamble verbatim.
    first_turn_prompt_cache: String,
}

impl JulesCore {
    pub fn new(
        session_uuid: Uuid,
        api_key_env: String,
        properties: JulesProperties,
        poll_interval_seconds: Option<u64>,
        require_plan_approval: Option<bool>,
        automation_mode: Option<String>,
        parent_id: Option<String>,
        task: &str,
    ) -> Self {
        let clamped = poll_interval_seconds
            .unwrap_or(DEFAULT_POLL_SECONDS)
            .clamp(MIN_POLL_SECONDS, MAX_POLL_SECONDS);
        let preamble = match parent_id.as_deref() {
            Some(pid) => build_first_turn_preamble(pid, task),
            None => String::new(),
        };
        Self {
            session_uuid,
            api_key_env,
            properties,
            poll_interval: Duration::from_secs(clamped),
            require_plan_approval: require_plan_approval.unwrap_or(true),
            automation_mode: automation_mode
                .unwrap_or_else(|| "AUTOMATION_MODE_UNSPECIFIED".to_string()),
            parent_id,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            socket_tx: None,
            jules_session_name: Arc::new(Mutex::new(None)),
            last_state: Arc::new(Mutex::new(JulesState::Unspecified)),
            seen_activity_ids: Arc::new(Mutex::new(HashSet::new())),
            latest_plan: Arc::new(Mutex::new(None)),
            latest_agent_message: Arc::new(Mutex::new(None)),
            started: false,
            first_turn_prompt_cache: preamble,
        }
    }

    pub fn with_socket_tx(mut self, tx: mpsc::Sender<AgentSocketEvent>) -> Self {
        self.socket_tx = Some(tx);
        self
    }

    /// Mark as already-started so a resurrected Jules worker skips the
    /// create-session path. The orchestrator guards against resurrection
    /// of remote-terminal workers (via `AgentChat::remote_terminal`),
    /// so in practice this path never fires — but keep the API parallel
    /// with AnthropicApiCore.
    pub fn mark_started(mut self) -> Self {
        self.started = true;
        self
    }
}

#[async_trait]
impl AICore for JulesCore {
    async fn turn(&mut self, prompt: &str) -> Result<BoxStream<'static, AIEvent>, AIError> {
        let (tx, rx) = mpsc::channel::<AIEvent>(64);

        let api_key = std::env::var(&self.api_key_env)
            .ok()
            .filter(|k| !k.is_empty());

        // Snapshot everything the polling task needs — the async block
        // owns its state, we stay `&mut self`.
        let session_uuid = self.session_uuid;
        let api_key_env = self.api_key_env.clone();
        let http = self.http.clone();
        let properties = self.properties.clone();
        let require_plan_approval = self.require_plan_approval;
        let automation_mode = self.automation_mode.clone();
        let socket_tx = self.socket_tx.clone();
        let parent_id = self.parent_id.clone();
        let poll_interval = self.poll_interval;
        let session_name_cell = self.jules_session_name.clone();
        let last_state_cell = self.last_state.clone();
        let seen_ids_cell = self.seen_activity_ids.clone();
        let latest_plan_cell = self.latest_plan.clone();
        let latest_msg_cell = self.latest_agent_message.clone();

        // Decide create-vs-continue up front (not under await in the
        // mutex — lock for as little as possible).
        let first_turn = !self.started;
        self.started = true;
        let prompt_full = if first_turn && !self.first_turn_prompt_cache.is_empty() {
            if prompt.is_empty() {
                self.first_turn_prompt_cache.clone()
            } else {
                format!("{}\n\n{}", self.first_turn_prompt_cache, prompt)
            }
        } else {
            prompt.to_string()
        };

        tokio::spawn(async move {
            let Some(api_key) = api_key else {
                let msg = format!("jules: env var `{api_key_env}` is not set or empty");
                let _ = tx
                    .send(AIEvent::Error {
                        kind: CodexFailureKind::Unknown,
                        message: msg.clone(),
                        raw: None,
                    })
                    .await;
                report_setup_failure(
                    &tx,
                    socket_tx.as_ref(),
                    session_uuid,
                    &format!("Jules worker failed to start: {msg}"),
                    "env_missing",
                )
                .await;
                return;
            };

            // Emit SystemInit so the TUI's AgentInfo block renders
            // the tool list (empty here — Jules runs its own tools
            // remotely) and model (unknown).
            let _ = tx
                .send(AIEvent::SystemInit {
                    session_id: session_uuid,
                    tools: Vec::new(),
                    model: Some("jules".to_string()),
                    raw: Value::Null,
                })
                .await;

            // Dispatch the turn: create session / approve plan / send
            // message. Any failure emits an Error + terminal
            // TurnResult and returns.
            let current_name = session_name_cell.lock().expect("poisoned").clone();
            let current_state = *last_state_cell.lock().expect("poisoned");

            let name = match current_name {
                None => {
                    // First turn: create a new Jules session.
                    match create_session(
                        &http,
                        &api_key,
                        &prompt_full,
                        &properties,
                        require_plan_approval,
                        &automation_mode,
                    )
                    .await
                    {
                        Ok(n) => {
                            *session_name_cell.lock().expect("poisoned") = Some(n.clone());
                            // Register the backend session id with
                            // the orchestrator so `worker archive`
                            // can DELETE the real remote session at
                            // teardown rather than waiting on Jules'
                            // own idle TTL.
                            if let Some(socket) = socket_tx.as_ref() {
                                let _ = send_ipc(
                                    socket,
                                    session_uuid,
                                    RequestCmd::RemoteSessionBound { name: n.clone() },
                                )
                                .await;
                            }
                            n
                        }
                        Err(e) => {
                            let msg = format!("jules: session create failed: {e}");
                            let _ = tx
                                .send(AIEvent::Error {
                                    kind: CodexFailureKind::Unknown,
                                    message: msg.clone(),
                                    raw: None,
                                })
                                .await;
                            report_setup_failure(
                                &tx,
                                socket_tx.as_ref(),
                                session_uuid,
                                &format!("Jules worker failed to start: {msg}"),
                                "create_failed",
                            )
                            .await;
                            return;
                        }
                    }
                }
                Some(n) => {
                    // Subsequent turn: translate the parent's reply
                    // into approve vs send_message.
                    let trimmed = prompt_full.trim().to_ascii_lowercase();
                    let is_approve =
                        trimmed == "approve" && current_state == JulesState::AwaitingPlanApproval;
                    let result = if is_approve {
                        approve_plan(&http, &api_key, &n).await
                    } else {
                        send_message(&http, &api_key, &n, &prompt_full).await
                    };
                    if let Err(e) = result {
                        let msg = format!("jules: turn dispatch failed: {e}");
                        let _ = tx
                            .send(AIEvent::Error {
                                kind: CodexFailureKind::Unknown,
                                message: msg.clone(),
                                raw: None,
                            })
                            .await;
                        // Dispatch failures on an already-running
                        // session don't terminate the remote session,
                        // but the parent still needs to know the
                        // approve/reply they asked for didn't go
                        // through. A simple mail (not WorkerComplete)
                        // keeps the worker alive so retry is possible.
                        if let (Some(socket), Some(parent)) =
                            (socket_tx.as_ref(), parent_id.as_deref())
                        {
                            let _ = send_ipc(
                                socket,
                                session_uuid,
                                RequestCmd::SendMessage {
                                    to: parent.to_string(),
                                    body: format!(
                                        "Remote worker couldn't forward your reply: {msg}. \
                                         Send the reply again to retry."
                                    ),
                                },
                            )
                            .await;
                        }
                        end_turn(&tx, "dispatch_failed").await;
                        return;
                    }
                    n
                }
            };

            // Poll until Jules hits a turn-terminal state. Only honour
            // cancellation implicitly via the channel closing — if the
            // consumer drops our stream the send will fail and we
            // return.
            let mut backoff = 0u64;
            loop {
                let jittered = jitter_ms(poll_interval.as_secs() * 1000, backoff);
                tokio::time::sleep(Duration::from_millis(jittered)).await;

                // 1. Drain any new activities. We walk through all
                // pages on each poll (capped at MAX_PAGES_PER_POLL to
                // stop runaway) and dedupe against the ids we've
                // already emitted — `nextPageToken` is a list-walker,
                // not a "since last poll" cursor, so without dedup we
                // would re-emit every activity on every poll and
                // starve the main UI loop with event traffic.
                let mut page_token: Option<String> = None;
                let mut pages_this_poll = 0;
                let mut emitted_this_poll = 0;
                let mut poll_failed = false;
                loop {
                    if pages_this_poll >= MAX_PAGES_PER_POLL {
                        break;
                    }
                    let activities_result =
                        list_activities(&http, &api_key, &name, page_token.as_deref()).await;
                    match activities_result {
                        Ok(ListActivitiesResult {
                            activities,
                            next_page_token,
                        }) => {
                            backoff = 0;
                            for a in activities {
                                let id = a
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .or_else(|| a.get("name").and_then(Value::as_str))
                                    .map(str::to_string);
                                let Some(id) = id else {
                                    // An activity without an id is
                                    // unexpected; skip rather than
                                    // emit it forever on every poll.
                                    continue;
                                };
                                {
                                    let mut seen = seen_ids_cell.lock().expect("poisoned");
                                    if !seen.insert(id) {
                                        continue;
                                    }
                                }
                                emit_activity(&tx, &a, &latest_plan_cell, &latest_msg_cell).await;
                                emitted_this_poll += 1;
                            }
                            match next_page_token {
                                Some(t) => {
                                    page_token = Some(t);
                                    pages_this_poll += 1;
                                }
                                None => break,
                            }
                        }
                        Err(PollError::RateLimited) | Err(PollError::ServerError) => {
                            backoff = next_backoff(backoff);
                            poll_failed = true;
                            break;
                        }
                        Err(PollError::Other(msg)) => {
                            let _ = tx
                                .send(AIEvent::Error {
                                    kind: CodexFailureKind::Unknown,
                                    message: format!("jules: activities poll error: {msg}"),
                                    raw: None,
                                })
                                .await;
                            end_turn(&tx, "poll_error").await;
                            return;
                        }
                    }
                }
                let _ = emitted_this_poll;
                if poll_failed {
                    continue;
                }

                // 2. Check session state.
                let state = match get_session_state(&http, &api_key, &name).await {
                    Ok(s) => s,
                    Err(PollError::RateLimited) | Err(PollError::ServerError) => {
                        backoff = next_backoff(backoff);
                        continue;
                    }
                    Err(PollError::Other(msg)) => {
                        let _ = tx
                            .send(AIEvent::Error {
                                kind: CodexFailureKind::Unknown,
                                message: format!("jules: session fetch error: {msg}"),
                                raw: None,
                            })
                            .await;
                        end_turn(&tx, "state_error").await;
                        return;
                    }
                };
                *last_state_cell.lock().expect("poisoned") = state;

                if state.is_terminal_for_turn() {
                    let plan_snapshot = latest_plan_cell.lock().expect("poisoned").clone();
                    let msg_snapshot = latest_msg_cell.lock().expect("poisoned").clone();
                    finish_turn(
                        &tx,
                        socket_tx.as_ref(),
                        session_uuid,
                        parent_id.as_deref(),
                        state,
                        plan_snapshot.as_deref(),
                        msg_snapshot.as_deref(),
                    )
                    .await;
                    return;
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn session_id(&self) -> Option<Uuid> {
        Some(self.session_uuid)
    }

    fn first_turn_prompt(&self) -> String {
        self.first_turn_prompt_cache.clone()
    }

    // Jules has no local shell — agents interact via tag IPC only.
    // Leaving `shell_tool_name` at the trait default of `None` is
    // what we want.
}

// ── Polling helpers ────────────────────────────────────────────────

#[derive(Debug)]
enum PollError {
    RateLimited,
    ServerError,
    Other(String),
}

#[derive(Debug, Clone, Deserialize)]
struct CreateSessionResponse {
    name: String,
}

async fn create_session(
    http: &reqwest::Client,
    api_key: &str,
    prompt: &str,
    properties: &JulesProperties,
    require_plan_approval: bool,
    automation_mode: &str,
) -> Result<String, String> {
    // Prefer a live lookup against `GET /sources` — the account's
    // actual resource names are authoritative (Jules has been known
    // to return different formats than the docs suggest). Fall back
    // to the documented `sources/github-{owner}/{repo}` string only
    // when the lookup fails or returns no entries, so the common path
    // is self-healing but we still produce a useful error when the
    // repo simply isn't connected.
    let resolved = resolve_source_name_via_list(http, api_key, properties).await;
    let source_name = match resolved {
        Ok(Some(n)) => n,
        _ => properties.source_name(),
    };
    let mut source_context = json!({
        "source": source_name,
    });
    if let Some(branch) = properties.branch.as_deref().filter(|s| !s.is_empty()) {
        source_context["githubRepoContext"] = json!({
            "startingBranch": branch,
        });
    }
    let body = json!({
        "prompt": prompt,
        "sourceContext": source_context,
        "requirePlanApproval": require_plan_approval,
        "automationMode": automation_mode,
    });
    let url = format!("{JULES_BASE_URL}/sessions");
    let resp = http
        .post(&url)
        .header("x-goog-api-key", api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(512).collect();
        // 404 on create nearly always means "the repo you asked for
        // isn't connected to this Jules account yet". Pull the list
        // of connected sources and surface them so the caller can
        // spot a typo or realise they need to visit jules.google/new.
        if status.as_u16() == 404 {
            let hint = match list_sources_summary(http, api_key).await {
                Ok(list) if list.is_empty() => {
                    " (no repos are currently connected to this Jules account — \
                     connect one at jules.google.com/new first)"
                        .to_string()
                }
                Ok(list) => format!(
                    " (asked for `{source_name}`; connected sources on this \
                     account are: {}). If your repo isn't listed, connect it \
                     at jules.google.com/new first.",
                    list.join(", ")
                ),
                Err(e) => format!(
                    " (asked for `{source_name}`; also failed to list \
                     available sources: {e})"
                ),
            };
            return Err(format!("HTTP {status}: {snippet}{hint}"));
        }
        return Err(format!("HTTP {status}: {snippet}"));
    }
    let parsed: CreateSessionResponse = resp
        .json()
        .await
        .map_err(|e| format!("decode response failed: {e}"))?;
    Ok(parsed.name)
}

/// Resolve the caller's `owner/repo` input to a real Jules source
/// `name` by listing the account's connected sources and matching on
/// (a) the documented string form, (b) the fallback all-dashes form,
/// or (c) the `githubRepo.{owner,repo}` structured fields. Returns
/// `Ok(None)` if the list was reachable but contained no match; the
/// caller then falls back to the documented format so Jules' own 404
/// surfaces the list of connected sources. `Err` for network-level
/// failures so the caller can decide whether to retry.
async fn resolve_source_name_via_list(
    http: &reqwest::Client,
    api_key: &str,
    properties: &JulesProperties,
) -> Result<Option<String>, String> {
    let (owner, repo) = match properties.repo.split_once('/') {
        Some((o, r)) => (o, r),
        // Malformed input — let the session-create path surface the
        // validation error rather than silently accepting.
        None => return Ok(None),
    };
    let documented = properties.source_name();
    let all_dashes = format!("sources/github-{}-{}", owner, repo);

    let url = format!("{JULES_BASE_URL}/sources?pageSize=100");
    let resp = http
        .get(&url)
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("decode failed: {e}"))?;
    let sources = match v.get("sources").and_then(Value::as_array) {
        Some(arr) => arr,
        None => return Ok(None),
    };
    for s in sources {
        let name = s.get("name").and_then(Value::as_str).unwrap_or("");
        if name == documented || name == all_dashes {
            return Ok(Some(name.to_string()));
        }
        let gh = s.get("githubRepo");
        let gh_owner = gh.and_then(|o| o.get("owner")).and_then(Value::as_str);
        let gh_repo = gh.and_then(|o| o.get("repo")).and_then(Value::as_str);
        if gh_owner == Some(owner) && gh_repo == Some(repo) {
            return Ok(Some(name.to_string()));
        }
    }
    Ok(None)
}

/// List connected Jules sources and return their `name` fields so the
/// caller can show a concrete list in error messages. Only used on
/// session-create failures — not called on every poll.
async fn list_sources_summary(
    http: &reqwest::Client,
    api_key: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{JULES_BASE_URL}/sources?pageSize=100");
    let resp = http
        .get(&url)
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("decode failed: {e}"))?;
    let names = v
        .get("sources")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(names)
}

async fn send_message(
    http: &reqwest::Client,
    api_key: &str,
    name: &str,
    prompt: &str,
) -> Result<(), String> {
    let url = format!("{JULES_BASE_URL}/{name}:sendMessage");
    let resp = http
        .post(&url)
        .header("x-goog-api-key", api_key)
        .header("content-type", "application/json")
        .json(&json!({ "prompt": prompt }))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(256).collect();
        return Err(format!("HTTP {status}: {snippet}"));
    }
    Ok(())
}

async fn approve_plan(http: &reqwest::Client, api_key: &str, name: &str) -> Result<(), String> {
    let url = format!("{JULES_BASE_URL}/{name}:approvePlan");
    let resp = http
        .post(&url)
        .header("x-goog-api-key", api_key)
        .header("content-type", "application/json")
        .json(&json!({}))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(256).collect();
        return Err(format!("HTTP {status}: {snippet}"));
    }
    Ok(())
}

struct ListActivitiesResult {
    activities: Vec<Value>,
    next_page_token: Option<String>,
}

async fn list_activities(
    http: &reqwest::Client,
    api_key: &str,
    name: &str,
    page_token: Option<&str>,
) -> Result<ListActivitiesResult, PollError> {
    let mut url = format!("{JULES_BASE_URL}/{name}/activities?pageSize=100");
    if let Some(pt) = page_token {
        url.push_str(&format!("&pageToken={}", urlencoding_encode(pt)));
    }
    let resp = http
        .get(&url)
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| PollError::Other(format!("request failed: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 429 {
        return Err(PollError::RateLimited);
    }
    if status.is_server_error() {
        return Err(PollError::ServerError);
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(256).collect();
        return Err(PollError::Other(format!("HTTP {status}: {snippet}")));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| PollError::Other(format!("decode failed: {e}")))?;
    let activities = v
        .get("activities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let next_page_token = v
        .get("nextPageToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(ListActivitiesResult {
        activities,
        next_page_token,
    })
}

async fn get_session_state(
    http: &reqwest::Client,
    api_key: &str,
    name: &str,
) -> Result<JulesState, PollError> {
    let url = format!("{JULES_BASE_URL}/{name}");
    let resp = http
        .get(&url)
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| PollError::Other(format!("request failed: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 429 {
        return Err(PollError::RateLimited);
    }
    if status.is_server_error() {
        return Err(PollError::ServerError);
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(256).collect();
        return Err(PollError::Other(format!("HTTP {status}: {snippet}")));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| PollError::Other(format!("decode failed: {e}")))?;
    let state_str = v
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("STATE_UNSPECIFIED");
    Ok(JulesState::parse(state_str))
}

/// DELETE the Jules session. Fire-and-forget — callers ignore errors
/// beyond logging because the local worker is already being torn
/// down. Public so `handle_worker_archive` can trigger it synchronously.
pub async fn delete_session(
    http: &reqwest::Client,
    api_key: &str,
    name: &str,
) -> Result<(), String> {
    let url = format!("{JULES_BASE_URL}/{name}");
    let resp = http
        .delete(&url)
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() && status.as_u16() != 404 {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(256).collect();
        return Err(format!("HTTP {status}: {snippet}"));
    }
    Ok(())
}

/// Best-effort URL encoder for page tokens. Jules returns opaque
/// base64-ish tokens; reqwest doesn't expose a public encode-component
/// helper, so we keep this minimal.
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ── Activity → AIEvent translation ─────────────────────────────────

async fn emit_activity(
    tx: &mpsc::Sender<AIEvent>,
    a: &Value,
    latest_plan: &Arc<Mutex<Option<String>>>,
    latest_agent_message: &Arc<Mutex<Option<String>>>,
) {
    let originator = a.get("originator").and_then(Value::as_str).unwrap_or("");

    // `userMessaged` activities come from us — skip to avoid
    // echoing the parent's message back into the worker's pane.
    if originator == "user" {
        return;
    }

    // Agent-text messages (including plan generation & progress)
    // come through as one of the event one-of fields.
    if let Some(msg) = a
        .get("agentMessaged")
        .and_then(|v| v.get("agentMessage"))
        .and_then(Value::as_str)
    {
        let trimmed = msg.trim();
        if !trimmed.is_empty() {
            // Cache so finish_turn can quote this back to the parent
            // alongside the state-transition mail — otherwise the
            // parent sees "awaits user feedback" with no idea what
            // Jules was actually asking.
            *latest_agent_message.lock().expect("poisoned") = Some(trimmed.to_string());
            let _ = tx
                .send(AIEvent::AssistantText {
                    text: msg.to_string(),
                    raw: a.clone(),
                })
                .await;
        }
    }

    if let Some(progress) = a.get("progressUpdated") {
        let title = progress.get("title").and_then(Value::as_str).unwrap_or("");
        let desc = progress
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        let text = if desc.is_empty() {
            format!("▸ {title}")
        } else {
            format!("▸ {title}: {desc}")
        };
        let _ = tx
            .send(AIEvent::AssistantText {
                text,
                raw: a.clone(),
            })
            .await;
    }

    if let Some(plan) = a.get("planGenerated").and_then(|v| v.get("plan")) {
        let mut text = String::from("📋 Plan:\n");
        if let Some(steps) = plan.get("steps").and_then(Value::as_array) {
            for step in steps {
                let idx = step.get("index").and_then(Value::as_u64).unwrap_or(0);
                let title = step.get("title").and_then(Value::as_str).unwrap_or("");
                let desc = step
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if desc.is_empty() {
                    text.push_str(&format!("  {idx}. {title}\n"));
                } else {
                    text.push_str(&format!("  {idx}. {title} — {desc}\n"));
                }
            }
        }
        // Cache so finish_turn's AWAITING_PLAN_APPROVAL mail can
        // include the plan inline — the parent shouldn't have to
        // switch panes to read it before deciding.
        *latest_plan.lock().expect("poisoned") = Some(text.clone());
        let _ = tx
            .send(AIEvent::AssistantText {
                text,
                raw: a.clone(),
            })
            .await;
    }

    if a.get("planApproved").is_some() {
        let _ = tx
            .send(AIEvent::AssistantText {
                text: "✓ plan approved".to_string(),
                raw: a.clone(),
            })
            .await;
    }

    // Artifacts (attached to any activity).
    if let Some(artifacts) = a.get("artifacts").and_then(Value::as_array) {
        for artifact in artifacts {
            emit_artifact(tx, artifact).await;
        }
    }

    // Also surface a few flat artifact fields some Jules replies carry.
    if let Some(change_set) = a.get("changeSet") {
        emit_change_set(tx, change_set).await;
    }
    if let Some(bash) = a.get("bashOutput") {
        emit_bash_output(tx, bash).await;
    }
}

async fn emit_artifact(tx: &mpsc::Sender<AIEvent>, artifact: &Value) {
    if let Some(bash) = artifact.get("bashOutput") {
        emit_bash_output(tx, bash).await;
    }
    if let Some(change_set) = artifact.get("changeSet") {
        emit_change_set(tx, change_set).await;
    }
    if let Some(media) = artifact.get("media") {
        let mime = media.get("mimeType").and_then(Value::as_str).unwrap_or("?");
        let _ = tx
            .send(AIEvent::AssistantText {
                text: format!("[media: {mime}]"),
                raw: artifact.clone(),
            })
            .await;
    }
}

async fn emit_bash_output(tx: &mpsc::Sender<AIEvent>, bash: &Value) {
    let command = bash.get("command").and_then(Value::as_str).unwrap_or("");
    let output = bash.get("output").and_then(Value::as_str).unwrap_or("");
    let exit_code = bash.get("exitCode").and_then(Value::as_i64).unwrap_or(0);
    let id = format!("jules-bash-{}", Uuid::new_v4());
    let _ = tx
        .send(AIEvent::ToolUse {
            id: id.clone(),
            name: "jules_bash".to_string(),
            input: json!({ "command": command }),
            raw: bash.clone(),
        })
        .await;
    let _ = tx
        .send(AIEvent::ToolResult {
            tool_use_id: id,
            content: Value::String(output.to_string()),
            is_error: exit_code != 0,
            raw: json!({
                "command": command,
                "output": output,
                "exitCode": exit_code,
            }),
        })
        .await;
}

async fn emit_change_set(tx: &mpsc::Sender<AIEvent>, change_set: &Value) {
    let patch = change_set.get("gitPatch");
    let diff = patch
        .and_then(|p| p.get("unidiffPatch"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let commit_msg = patch
        .and_then(|p| p.get("suggestedCommitMessage"))
        .and_then(Value::as_str)
        .unwrap_or("(no commit message)");
    let file_count = count_patch_files(&diff);
    let summary = if file_count == 0 {
        format!("change set: {commit_msg}")
    } else {
        format!("change set: {commit_msg} ({file_count} files)")
    };
    // The AI event stream carries summary + diff; the TUI maps this
    // to a `ChatBlock::RemotePatch` via the handle_ai_event branch.
    let _ = tx
        .send(AIEvent::ToolUse {
            id: format!("jules-changeset-{}", Uuid::new_v4()),
            name: "jules_change_set".to_string(),
            input: json!({
                "summary": summary,
                "diff": diff,
            }),
            raw: change_set.clone(),
        })
        .await;
}

fn count_patch_files(diff: &str) -> usize {
    diff.lines()
        .filter(|l| l.starts_with("diff --git "))
        .count()
}

/// Send a message via `socket_tx` as if the Jules worker had emitted
/// it itself — drives the "mail parent" and "worker complete" paths
/// through the same orchestrator code local workers use.
async fn send_ipc(
    socket_tx: &mpsc::Sender<AgentSocketEvent>,
    agent_id: Uuid,
    cmd: RequestCmd,
) -> Response {
    let (reply_tx, reply_rx) = oneshot::channel();
    let event = AgentSocketEvent {
        request: Request { agent_id, cmd },
        reply: reply_tx,
    };
    if socket_tx.send(event).await.is_err() {
        return Response::err("orchestrator channel closed");
    }
    reply_rx
        .await
        .unwrap_or_else(|_| Response::err("orchestrator dropped reply"))
}

async fn finish_turn(
    tx: &mpsc::Sender<AIEvent>,
    socket_tx: Option<&mpsc::Sender<AgentSocketEvent>>,
    agent_id: Uuid,
    parent_id: Option<&str>,
    state: JulesState,
    latest_plan: Option<&str>,
    latest_agent_message: Option<&str>,
) {
    let stop_reason = state.stop_reason();

    // Send the state-transition mail / worker complete through the
    // orchestrator. If we have no parent id (shouldn't happen for
    // workers), just end the stream.
    if let (Some(socket), Some(parent)) = (socket_tx, parent_id) {
        // Quote the most recent thing Jules said as a block every
        // state mail can embed — transitions like
        // AWAITING_USER_FEEDBACK / COMPLETED are only actionable for
        // the parent if the parent can see what Jules actually
        // wrote.
        let last_from_jules = match latest_agent_message {
            Some(m) if !m.is_empty() => {
                format!("\n\n--- Last message from the remote worker ---\n{m}\n--- end ---")
            }
            _ => String::new(),
        };
        match state {
            JulesState::AwaitingPlanApproval => {
                let plan_block = latest_plan
                    .map(|p| format!("\n\n{p}"))
                    .unwrap_or_else(|| " (see the worker's pane for details)".to_string());
                let body = format!(
                    "Remote worker has produced a plan and is awaiting your \
                     approval.{plan_block}{last_from_jules}\n\n\
                     Reply with `send` body exactly `approve` to accept the \
                     plan as-is and execute it, or `send` a body with \
                     feedback to revise it. DO NOT archive — the session \
                     is still live and needs your decision."
                );
                let _ = send_ipc(
                    socket,
                    agent_id,
                    RequestCmd::SendMessage {
                        to: parent.to_string(),
                        body,
                    },
                )
                .await;
            }
            JulesState::AwaitingUserFeedback => {
                // This state almost always means Jules asked a
                // specific question (often "should I open a PR?"
                // mid-task). The parent MUST answer it rather than
                // archive — the session is not done.
                let body = format!(
                    "Remote worker is asking you a question and won't \
                     proceed until you answer.{last_from_jules}\n\n\
                     Reply via `send` with the answer to the remote \
                     worker's question. DO NOT archive — the session is \
                     paused mid-task and will lose in-progress work if \
                     you tear it down now. Archive only after the task \
                     is fully finished (PR landed / changes accepted)."
                );
                let _ = send_ipc(
                    socket,
                    agent_id,
                    RequestCmd::SendMessage {
                        to: parent.to_string(),
                        body,
                    },
                )
                .await;
            }
            JulesState::Paused => {
                let body = format!(
                    "Remote worker is PAUSED.{last_from_jules}\n\n\
                     Send another message to resume, or `worker archive` \
                     if the task is abandoned."
                );
                let _ = send_ipc(
                    socket,
                    agent_id,
                    RequestCmd::SendMessage {
                        to: parent.to_string(),
                        body,
                    },
                )
                .await;
            }
            JulesState::Completed => {
                // Jules' `COMPLETED` is "idle, awaiting further
                // input" — the session is still live and accepts
                // follow-up `:sendMessage` calls to request changes
                // (e.g. after a PR is opened). Mail the parent so
                // they know the current work wrapped, but leave the
                // worker un-hibernated so `send` keeps routing to
                // this chat. Only `worker archive` actually closes
                // the remote session.
                let body = format!(
                    "Remote worker reports its current attempt is COMPLETE. \
                     Read the last message below carefully — if it \
                     contains a question (e.g. \"should I open a PR?\"), \
                     the task is NOT actually done; reply to answer. Only \
                     `worker archive` when the PR has landed (or the \
                     work is truly abandoned).{last_from_jules}\n\n\
                     Next step: reply via `send` (to request more changes \
                     or answer a pending question) OR `worker archive \
                     --id <worker>` (to close the remote session)."
                );
                let _ = send_ipc(
                    socket,
                    agent_id,
                    RequestCmd::SendMessage {
                        to: parent.to_string(),
                        body,
                    },
                )
                .await;
            }
            JulesState::Failed => {
                // Jules' `FAILED` is recoverable in the same way —
                // the session remains addressable, and sending a
                // follow-up message often lets the user correct
                // course. Mail + leave alive; parent can retry or
                // archive.
                let body = format!(
                    "Remote worker reports the current attempt FAILED.\
                     {last_from_jules}\n\n\
                     Send another message with corrective instructions \
                     to retry, or `worker archive --id <worker>` to close \
                     the remote session if recovery isn't possible."
                );
                let _ = send_ipc(
                    socket,
                    agent_id,
                    RequestCmd::SendMessage {
                        to: parent.to_string(),
                        body,
                    },
                )
                .await;
            }
            _ => {}
        }
    }

    // Emit the terminal TurnResult so the worker's status flips to
    // Ready (or, for WorkerComplete, the orchestrator hibernates it).
    let _ = tx
        .send(AIEvent::TurnResult {
            stop_reason: Some(stop_reason.to_string()),
            total_cost_usd: None,
            usage: None,
            raw: Value::Null,
        })
        .await;
}

/// Route a setup-time failure (missing API key, session-create 404,
/// etc.) back to the parent so the caller agent actually learns the
/// spawn didn't succeed. Emits `WorkerComplete` via `socket_tx` — the
/// orchestrator's `handle_worker_complete` then delivers the body as
/// mail to the parent, hibernates this worker, and (because the core
/// is Jules) flags `remote_terminal = true` so no one tries to
/// resurrect a non-existent remote session. Falls back to just the
/// TurnResult when the worker has no `socket_tx` or `parent_id`.
async fn report_setup_failure(
    tx: &mpsc::Sender<AIEvent>,
    socket_tx: Option<&mpsc::Sender<AgentSocketEvent>>,
    agent_id: Uuid,
    body: &str,
    stop_reason: &str,
) {
    if let Some(socket) = socket_tx {
        let _ = send_ipc(
            socket,
            agent_id,
            RequestCmd::WorkerComplete {
                body: body.to_string(),
            },
        )
        .await;
    }
    end_turn(tx, stop_reason).await;
}

async fn end_turn(tx: &mpsc::Sender<AIEvent>, stop_reason: &str) {
    let _ = tx
        .send(AIEvent::TurnResult {
            stop_reason: Some(stop_reason.to_string()),
            total_cost_usd: None,
            usage: None,
            raw: Value::Null,
        })
        .await;
}

fn next_backoff(current: u64) -> u64 {
    if current == 0 {
        2
    } else {
        (current * 2).min(MAX_BACKOFF_SECONDS)
    }
}

/// Produce a jittered sleep duration in milliseconds around `base_ms`
/// with ±20% spread. Additive `extra_seconds` backoff bumps the base.
/// No RNG dependency — a cheap hash of the current Unix nanos gives
/// us "good enough" per-worker variance.
fn jitter_ms(base_ms: u64, extra_seconds: u64) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let total_base = base_ms.saturating_add(extra_seconds.saturating_mul(1000));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // Pseudo-random ±20%.
    let spread = (total_base / 5).max(1);
    let sample = now % (2 * spread + 1);
    total_base.saturating_sub(spread).saturating_add(sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_parse_matches_known_values() {
        assert_eq!(JulesState::parse("COMPLETED"), JulesState::Completed);
        assert_eq!(JulesState::parse("FAILED"), JulesState::Failed);
        assert_eq!(
            JulesState::parse("AWAITING_PLAN_APPROVAL"),
            JulesState::AwaitingPlanApproval
        );
        assert_eq!(JulesState::parse("BOGUS"), JulesState::Unknown);
    }

    #[test]
    fn terminal_states_end_the_turn() {
        assert!(JulesState::Completed.is_terminal_for_turn());
        assert!(JulesState::Failed.is_terminal_for_turn());
        assert!(JulesState::AwaitingPlanApproval.is_terminal_for_turn());
        assert!(JulesState::AwaitingUserFeedback.is_terminal_for_turn());
        assert!(JulesState::Paused.is_terminal_for_turn());
        assert!(!JulesState::InProgress.is_terminal_for_turn());
        assert!(!JulesState::Planning.is_terminal_for_turn());
    }

    #[test]
    fn jules_properties_parses_repo() {
        let v = serde_json::json!({"repo": "acme/foo", "branch": "main"});
        let p = JulesProperties::parse(&v, None).unwrap();
        assert_eq!(p.repo, "acme/foo");
        assert_eq!(p.branch.as_deref(), Some("main"));
        assert_eq!(p.source_name(), "sources/github-acme/foo");
    }

    #[test]
    fn jules_properties_source_name_preserves_hyphens_in_repo() {
        // Regression: "wiltaylor/Test-Project" used to come out as
        // "sources/github-wiltaylor-Test-Project" (all `-`), which
        // 404s because the real Jules source name keeps `/` between
        // owner and repo.
        let v = serde_json::json!({"repo": "wiltaylor/Test-Project"});
        let p = JulesProperties::parse(&v, None).unwrap();
        assert_eq!(p.source_name(), "sources/github-wiltaylor/Test-Project");
    }

    #[test]
    fn jules_properties_rejects_missing_repo() {
        let v = serde_json::json!({});
        assert!(JulesProperties::parse(&v, None).is_err());
    }

    #[test]
    fn jules_properties_rejects_malformed_repo() {
        let v = serde_json::json!({"repo": "just-a-name"});
        assert!(JulesProperties::parse(&v, None).is_err());
    }

    #[test]
    fn jules_properties_falls_back_to_default_branch() {
        let v = serde_json::json!({"repo": "acme/foo"});
        let p = JulesProperties::parse(&v, Some("develop")).unwrap();
        assert_eq!(p.branch.as_deref(), Some("develop"));
    }

    #[test]
    fn jules_properties_parses_auto_create_pr_override() {
        let v = serde_json::json!({"repo": "acme/foo", "autoCreatePr": true});
        let p = JulesProperties::parse(&v, None).unwrap();
        assert_eq!(p.auto_create_pr, Some(true));

        let v = serde_json::json!({"repo": "acme/foo", "autoCreatePr": false});
        let p = JulesProperties::parse(&v, None).unwrap();
        assert_eq!(p.auto_create_pr, Some(false));

        let v = serde_json::json!({"repo": "acme/foo"});
        let p = JulesProperties::parse(&v, None).unwrap();
        assert_eq!(p.auto_create_pr, None);
    }

    #[test]
    fn count_patch_files_counts_diff_headers() {
        let diff = "diff --git a/x b/x\n@@\n-a\n+b\ndiff --git a/y b/y\n@@\n-c\n+d\n";
        assert_eq!(count_patch_files(diff), 2);
    }

    #[test]
    fn next_backoff_caps_at_maximum() {
        let mut b = 0;
        for _ in 0..12 {
            b = next_backoff(b);
        }
        assert_eq!(b, MAX_BACKOFF_SECONDS);
    }

    #[test]
    fn urlencoding_preserves_safe_chars_and_escapes_slashes() {
        assert_eq!(urlencoding_encode("ABC-_.~"), "ABC-_.~");
        assert_eq!(urlencoding_encode("a/b c"), "a%2Fb%20c");
    }
}
