use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wcl_lang::{parse, ParseOptions, Value};

/// Reserved recipient id for the human operator's inbox. No configured
/// agent / automaton / worker / channel / webhook may use this id.
pub const USER_INBOX_ID: &str = "user";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Team {
    pub id: String,
    pub description: String,
    /// When `true`, user-declared `thinking_messages` blocks replace the
    /// built-in pool entirely; when `false` (default) they are merged
    /// with the built-ins.
    pub replace_thinking_messages: bool,
    /// Fleet-wide default AI core id applied to agents/workers that
    /// don't declare `ai_core`. Must resolve to a declared `ai_core`
    /// block id.
    pub default_ai_core: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub role: String,
    /// Long-form persona/priorities/style guidance. Emitted inside a
    /// `<role>...</role>` block on the first turn so the model can read
    /// it as primary behavioural direction. `None` ⇒ no `<role>` block.
    pub role_description: Option<String>,
    /// Per-agent model override. `None` falls back to the resolved
    /// ai_core's own `model` (via `AiCore::runtime_model`). Honoured
    /// by every core variant — direct-API variants override the
    /// `model` field in the request body; CLI variants pass it as
    /// `--model <id>` to the underlying binary.
    pub model: Option<String>,
    pub instructions: String,
    /// Allowlist of agent ids this agent may send messages to.
    /// `None` means unrestricted (can message anyone).
    /// `Some(vec![])` means may not message any other agent.
    pub can_message: Option<Vec<String>>,
    /// Allowlist of worker template ids this agent may spawn via
    /// `aciddog worker create`. `None` (the default) disables worker
    /// creation entirely; `Some(list)` is the set of permitted
    /// templates.
    pub can_create_workers: Option<Vec<String>>,
    /// Allowlist of workflow ids this agent may trigger via the
    /// `aciddog workflow run` CLI. `None` (the default) disables
    /// workflow triggering entirely; `Some(list)` is the set of
    /// permitted workflows.
    pub can_run_workflows: Option<Vec<String>>,
    /// Optional per-agent tool allowlist. `None` falls back to the
    /// resolved ai_core's own `default_tools`, then the built-in
    /// `DEFAULT_ALLOWED_TOOLS`.
    pub tools: Option<Vec<String>>,
    /// Optional ai_core id override. `None` falls back to
    /// `Team::default_ai_core`.
    pub ai_core: Option<String>,
    /// Allowlist of memory bank ids this agent may read from and write
    /// to. `None` ⇒ no memory access; `Some(list)` grants read+write on
    /// each listed bank.
    pub memory_banks: Option<Vec<String>>,
    /// Skills pre-announced to this agent in its first-turn prompt. Each
    /// id must match a directory in `.aciddog/skills/<id>/` containing a
    /// `SKILL.md`. Unlisted skills remain discoverable by browsing the
    /// folder. `None` ⇒ no skills section in the prompt.
    pub skills: Option<Vec<String>>,
    /// Optional id of a declared `compaction` block driving `/compact`
    /// for this agent. `None` ⇒ `/compact` falls back to the built-in
    /// `drop` strategy.
    pub compaction: Option<String>,
    /// Grants fleet-management capabilities: observe (`agent list`,
    /// `agent status`, `agent history`, `worker list`), control
    /// (`agent stop`, `agent sleep`, `agent wake`, `agent send-as-user`,
    /// `worker force-archive`), and bypasses this agent's own
    /// `can_message` allowlist when using `send`. Only agents (not
    /// workers or automatons) may be supervisors.
    #[serde(default)]
    pub supervisor: bool,
    /// Grants server-level controls (`server shutdown`, `server exit`)
    /// on top of supervisor. Implies `supervisor = true` after the
    /// config is parsed.
    #[serde(default)]
    pub sysadmin: bool,
    /// Pin to the top of the agent list and decorate with 👑 in the
    /// TUI and web UI. Purely cosmetic — sort + crown only, no
    /// behavioural overload. Multiple leaders are tied by declaration
    /// order.
    #[serde(default)]
    pub leader: bool,
    /// What `/reload` does when this agent's effective config changes.
    /// Default is [`ReloadAction::Ignore`] — the running session keeps
    /// its current prompt/model/tools until the next `/compact` or
    /// `/reset`, at which point the new config takes effect naturally.
    #[serde(default)]
    pub on_reload: ReloadAction,
}

/// A template for an ephemeral worker. Unlike `Agent`, a `Worker` is not
/// spawned at startup — it is instantiated on demand when an agent (or
/// another worker whose template opts in) calls `aciddog worker create`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Worker {
    pub id: String,
    pub role: String,
    /// Long-form persona/priorities/style guidance. Emitted inside a
    /// `<role>...</role>` block on the first turn so the model can read
    /// it as primary behavioural direction. `None` ⇒ no `<role>` block.
    pub role_description: Option<String>,
    /// Per-worker-template model override. Same semantics as
    /// `Agent::model`: `None` falls back to the resolved ai_core's own
    /// `model`; otherwise replaces it for instances spawned from this
    /// template.
    pub model: Option<String>,
    pub instructions: String,
    /// Optional per-template tool allowlist. `None` falls back to the
    /// resolved ai_core's own `default_tools`, then the built-in
    /// `DEFAULT_ALLOWED_TOOLS`.
    pub tools: Option<Vec<String>>,
    /// Allowlist of worker template ids an instance of this template
    /// may itself spawn. Same semantics as `Agent::can_create_workers`.
    pub can_create_workers: Option<Vec<String>>,
    /// Allowlist of workflow ids an instance of this template may
    /// trigger. Same semantics as `Agent::can_run_workflows`.
    pub can_run_workflows: Option<Vec<String>>,
    /// Optional ai_core id override. `None` falls back to
    /// `Team::default_ai_core`.
    pub ai_core: Option<String>,
    /// Allowlist of memory bank ids this worker template may read from
    /// and write to. Same semantics as `Agent::memory_banks`.
    pub memory_banks: Option<Vec<String>>,
    /// Skills pre-announced to instances of this worker template. Same
    /// semantics as `Agent::skills`.
    pub skills: Option<Vec<String>>,
    /// Optional id of a declared `compaction` block driving `/compact`
    /// for instances of this template. Same semantics as
    /// `Agent::compaction`.
    pub compaction: Option<String>,
}

/// A script-backed pseudo-agent. Unlike [`Agent`]/[`Worker`], an
/// automaton is a pseudo-agent driven by a per-event backend rather
/// than by an AI core. Three backends exist:
///
/// - [`AutomatonBackend::Script`] — each delivered event spawns a
///   child process with the event written as a single JSON document
///   on stdin; the process exits when done and its stdout/stderr
///   lines become blocks in the automaton's chat pane. The child
///   inherits the same `ACIDDOG_AGENT_ID` + `ACIDDOG_AGENT_SOCKET`
///   env vars that agents get, so it can call back through the
///   `aciddog` CLI. One child at a time per automaton: while it's
///   running the status is `Working` and new messages queue in the
///   usual priority channels (user > agent > channel).
/// - [`AutomatonBackend::Webservice`] — the automaton is backed by a
///   long-lived HTTP server (HTTPS with an API key, or a Unix socket
///   whose filesystem permissions are the trust boundary). The
///   server exposes a `/v1/rpc` endpoint that accepts the same JSON
///   protocol the internal agent IPC speaks — requests are stamped
///   with the automaton's session id and flow into the same handler
///   pipeline. A `/v1/events` SSE stream carries the mail / channel
///   traffic that a script-backed automaton would see on stdin.
///   `webhook` blocks mount as additional paths on this same
///   listener.
/// - [`AutomatonBackend::Remote`] — client-side counterpart to
///   `Webservice`. The automaton dials a remote aciddog's `/v1/rpc`
///   + `/v1/events` endpoints and proxies them as a local fleet
///   member. Local traffic becomes `send_message(to=remote_target)` RPCs
///   against the remote; inbound SSE is delivered to a configured
///   `deliver_to`. An optional `proxy_socket_path` exposes the full
///   `RequestCmd` surface to local tools.
#[allow(clippy::doc_lazy_continuation)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Automaton {
    pub id: String,
    pub role: String,
    /// Long-form description of the automaton's purpose. Not sent
    /// anywhere — displayed as pane metadata in the TUI.
    pub role_description: Option<String>,
    /// Allowlist of agent/automaton ids this automaton may mail via
    /// `aciddog send --to`. Same semantics as [`Agent::can_message`].
    pub can_message: Option<Vec<String>>,
    /// How inbound events are handled. Driven by the `type` symbol on
    /// the WCL block (`:script` default, `:webservice` for HTTP).
    pub backend: AutomatonBackend,
    /// Same semantics as [`Agent::supervisor`] — grants the automaton
    /// access to supervisor RPCs (`AgentList`, `AgentHistory`,
    /// `AgentSendAsUser`, etc.). Primary use: a `:webservice`
    /// automaton that backs the remote management portal.
    pub supervisor: bool,
    /// Same semantics as [`Agent::sysadmin`] — grants the automaton
    /// access to server-lifecycle RPCs (`ServerShutdown`, `ServerExit`,
    /// `ServerReload`) on top of supervisor. Implies `supervisor =
    /// true` after parse.
    pub sysadmin: bool,
    /// Same semantics as [`Agent::leader`] — pins the automaton to the
    /// top of the agent list and decorates it with 👑 in the TUI / web
    /// UI. Purely cosmetic.
    #[serde(default)]
    pub leader: bool,
}

/// The two delivery backends an automaton can use. See [`Automaton`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AutomatonBackend {
    Script {
        /// Executable to spawn. A bare binary name on PATH or an
        /// absolute path. Not a shell string — extra argv goes in
        /// `args`.
        command: String,
        /// Extra argv entries appended after `command`. Empty vec ⇒
        /// the child is spawned with only `command` as argv[0].
        args: Vec<String>,
        /// Working directory for the child. `None` inherits the
        /// harness cwd.
        cwd: Option<String>,
        /// Extra environment variables passed to the child. Parsed
        /// at config load time from `KEY=VAL` strings; malformed
        /// entries (missing `=`) are rejected via
        /// [`ConfigError::BadEnvEntry`].
        env: Vec<(String, String)>,
        /// Kill the child after this many seconds. `None` ⇒ no
        /// timeout.
        timeout_seconds: Option<u32>,
    },
    Webservice {
        bind: WebserviceBind,
        /// Env var holding the API-key expected on inbound requests.
        /// Required when `bind` is [`WebserviceBind::Https`];
        /// ignored (and rejected at parse time) when `bind` is
        /// [`WebserviceBind::UnixSocket`].
        api_key_env: Option<String>,
    },
    /// Client-side counterpart to [`AutomatonBackend::Webservice`]: this
    /// automaton *dials* a remote aciddog's webservice (HTTPS with an
    /// API key, or a Unix socket on the local filesystem) and forwards
    /// local traffic to a fixed identity on the remote. Local members
    /// that message this automaton get their body sent to
    /// `remote_target` on the remote; inbound SSE events from the
    /// remote are delivered to `deliver_to` locally. An optional
    /// `proxy_socket_path` mounts a local Unix socket that speaks the
    /// same JSON-per-line wire format as the internal agent IPC, so
    /// local tools can set `ACIDDOG_AGENT_SOCKET` to that path and
    /// drive the full `RequestCmd` surface against the remote.
    Remote {
        transport: RemoteTransport,
        /// Id on the remote instance that local messages are addressed
        /// to. Free-form — validated only at runtime when the remote
        /// rejects an unknown recipient.
        remote_target: String,
        /// Local agent/automaton id that receives inbound SSE events.
        /// Validated at config load to reference a declared id.
        deliver_to: String,
        /// Optional Unix socket path — when set, a listener mounts that
        /// socket and tunnels every JSON `RequestCmd` through to the
        /// remote `/v1/rpc`. `None` ⇒ no tunnel, chat-only bridge.
        proxy_socket_path: Option<PathBuf>,
    },
}

/// Transport a webservice-backed automaton listens on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WebserviceBind {
    Https {
        /// `host:port` — parsed by the runtime into a `SocketAddr`.
        address: String,
        tls_cert: PathBuf,
        tls_key: PathBuf,
    },
    UnixSocket {
        path: PathBuf,
    },
}

/// Transport a `:remote`-backed automaton dials to reach the remote
/// aciddog instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteTransport {
    Https {
        /// Base URL of the remote webservice, e.g. `https://host:8443`.
        /// The client appends `/v1/rpc` and `/v1/events`.
        url: String,
        /// Env var holding the Bearer token sent in `Authorization`.
        api_key_env: String,
        /// Optional path to a PEM-encoded CA bundle used to verify the
        /// remote's TLS certificate. `None` ⇒ use the system roots.
        tls_ca: Option<PathBuf>,
    },
    UnixSocket {
        path: PathBuf,
    },
}

/// A webhook endpoint mounted on a webservice-backed automaton.
///
/// Each webhook is a named path on the hosting automaton's HTTP
/// listener. When a request arrives and passes the declared auth
/// check, the handler packages the request (method, path, query,
/// headers, body) into a JSON envelope and delivers it to
/// `deliver_to` as a message sent from the host automaton's
/// identity — so the host's `can_message` ACL is what gates which
/// agents a webhook can reach.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Webhook {
    pub id: String,
    /// Id of a declared automaton whose backend is
    /// [`AutomatonBackend::Webservice`]. Validated at config load.
    pub mount: String,
    /// HTTP path the webhook listens on. Must start with `/`.
    pub path: String,
    /// Agent or automaton id that receives the delivery.
    pub deliver_to: String,
    pub auth: WebhookAuth,
    /// Reject request bodies larger than this many bytes with 413.
    /// Default applied by the runtime (~1 MiB).
    pub max_body_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WebhookAuth {
    None,
    /// `Authorization: Bearer <env>` — the value of the env var
    /// named by `secret_env` must match the token after `Bearer `.
    Bearer {
        secret_env: String,
    },
    /// Fixed-value header. The value of `header` must equal the
    /// value of the env var named by `secret_env` (constant-time
    /// compare).
    Header {
        header: String,
        secret_env: String,
    },
    /// Token in a dot-path within the JSON body. The value found at
    /// `field` (e.g. `data.token`) must equal the value of the env
    /// var named by `secret_env`.
    JsonField {
        field: String,
        secret_env: String,
    },
    /// HMAC signature in a header (e.g. GitHub's
    /// `X-Hub-Signature-256: sha256=<hex>`). The handler computes
    /// HMAC(secret_env, raw_body) under `algo` and requires the
    /// header value (optionally prefixed with `"sha256="` /
    /// `"sha1="`) to match in constant time.
    HmacHeader {
        header: String,
        secret_env: String,
        algo: HmacAlgo,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HmacAlgo {
    Sha1,
    Sha256,
}

/// A virtual chat room shared by a named subset of agents plus the
/// human operator. Messages posted into a channel are broadcast to
/// every member (except the sender) as
/// `<ChannelMessage channel="<id>" from="<sender>">body</ChannelMessage>`
/// at the lowest priority of any inbound message (below user input and
/// direct mailbox messages). Members can be either declared `agent` or
/// `automaton` ids — workers are ephemeral and don't participate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    /// Short human-readable description of the channel's purpose.
    /// Surfaced in the `<channels>` block of each member's first-turn
    /// prompt so the model knows what the room is for. `None` ⇒ no
    /// description line emitted.
    pub description: Option<String>,
    /// Cosmetic banner rendered at the top of the channel pane in the
    /// TUI. Never sent to any agent. `None` ⇒ no welcome block.
    pub welcome: Option<String>,
    /// Agent ids that belong to this channel. Validated at parse time
    /// to reference declared `agent` blocks only (never worker
    /// templates). Order is preserved for deterministic broadcast.
    pub members: Vec<String>,
}

/// A named, persistent memory bank. Declared at the fleet level and
/// referenced by agents/workers via their `memory_banks` allowlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryBank {
    pub id: String,
    /// Human-readable description of what this bank stores. Surfaced to
    /// agents in the `<memory>` block of their first-turn prompt so the
    /// model can decide when each bank is relevant.
    pub description: String,
    /// fastembed model id to use for embeddings in this bank. `None`
    /// resolves to the built-in default (BGE-small-en-v1.5).
    pub embed_model: Option<String>,
}

/// Where to fetch rate-limit/usage data from for a given core. Kept as
/// a top-level property on `AiCore` (rather than inside the variant)
/// because the same variant can target several providers — e.g. an
/// `anthropicApi` core pointed at `api.anthropic.com` wants the
/// Anthropic OAuth usage endpoint, while the same variant pointed at
/// `api.z.ai/api/anthropic` wants the z.ai quota endpoint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageProvider {
    /// z.ai quota endpoint (`/api/monitor/usage/quota/limit`). Uses the
    /// core's own `api_key_env` as the bearer token.
    Zai,
    /// OpenAI ChatGPT subscription usage via the upstream Codex
    /// `/wham/usage` endpoint on chatgpt.com.
    ChatgptOauth,
    /// OpenRouter credits endpoint (`/api/v1/credits`). Uses the
    /// `openAiApi` core's own `api_key_env` as the bearer token and
    /// renders remaining dollar balance.
    Openrouter,
    /// GitHub Copilot premium-request usage. Sources a GitHub OAuth
    /// token by shelling out to `gh auth token` and queries the REST
    /// billing endpoint for the authenticated user. Applies only to
    /// `copilotCli` cores.
    GithubCopilot,
}

/// Configuration for a named AI runtime. The `kind` field is a
/// tagged-union discriminator — each variant carries its own settings
/// that the runtime reads when constructing a core for an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiCore {
    pub id: String,
    pub kind: AiCoreKind,
    /// Explicit usage/quota data source for this core. `None` means no
    /// HTTP tracking; `claudeCode` cores populate their reset info
    /// from `rate_limit_event` stream lines instead of a provider here.
    pub usage_provider: Option<UsageProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AiCoreKind {
    ClaudeCode {
        /// Content for `--system-prompt-file`. When `None`, the flag is
        /// omitted and claude uses its default system prompt.
        system_prompt: Option<String>,
        /// Optional `--model <id>` passed to the `claude` CLI. `None`
        /// lets claude pick its own default. Agent/worker-level
        /// `model` overrides this value.
        model: Option<String>,
        /// Per-core default tool allowlist applied when an agent or
        /// worker using this core doesn't declare its own `tools`.
        /// Tool names depend on the core, which is why the default
        /// lives here rather than on the team.
        default_tools: Option<Vec<String>>,
    },
    /// Generic Anthropic-Messages-API-compatible core. Speaks the wire
    /// protocol directly, so the same variant targets api.anthropic.com
    /// or any compatible endpoint (Z.ai, etc.); only `api_url`,
    /// `api_key_env`, and `model` change between vendors.
    AnthropicApi {
        /// Base URL of the API (e.g. `https://api.anthropic.com` or
        /// `https://api.z.ai/api/anthropic`). The core appends
        /// `/v1/messages`.
        api_url: String,
        /// Name of the environment variable holding the API key.
        /// Resolved at turn time so missing keys surface as an
        /// `AIEvent::Error` rather than a startup failure.
        api_key_env: String,
        /// Model id sent in the `model` request field.
        model: String,
        /// Optional `system` field content. `None` ⇒ no system message
        /// sent.
        system_prompt: Option<String>,
        /// Per-core default tool allowlist. Unknown names are rejected
        /// at config parse time (see `validate_ai_core_tools`).
        default_tools: Option<Vec<String>>,
    },
    /// GitHub Copilot CLI core — drives the `copilot` binary the same
    /// way `ClaudeCode` drives `claude`. Copilot doesn't support a
    /// system-prompt flag, so this variant has no `system_prompt`
    /// field. Tool names in `default_tools` use the aciddog-side
    /// capitalized names (e.g. `Bash`, `Read`); the core translates
    /// them to copilot's lowercase natives at spawn time.
    CopilotCli {
        /// Optional `--model <id>` passed on every turn. `None` lets
        /// copilot pick its own default. Accepts copilot's native
        /// model ids (e.g. `claude-haiku-4.5`, `gpt-5.2`).
        model: Option<String>,
        /// Per-core default tool allowlist. Limited to the six names
        /// that map to copilot built-ins (Bash, Read, Write, Edit,
        /// Glob, Grep) — see `validate_ai_core_tools`.
        default_tools: Option<Vec<String>>,
    },
    /// Google Gemini CLI core — drives the `gemini` binary (install
    /// with `npm i -g @google/gemini-cli`). Gemini generates its own
    /// per-session UUID and rejects seeded ones, so the core captures
    /// gemini's id from its own `init` stream event to pass as
    /// `--resume <uuid>` on subsequent turns. Tool names in
    /// `default_tools` use the aciddog-side capitalized names; the
    /// core translates them to gemini's native names at turn time and
    /// restricts unmapped built-ins via a generated policy file.
    GeminiCli {
        /// Optional `--model <id>` passed on every turn. `None` lets
        /// gemini pick its own default. Accepts gemini's native model
        /// ids (e.g. `auto-gemini-3`, `gemini-2.5-flash-lite`).
        model: Option<String>,
        /// Per-core default tool allowlist. Limited to the six names
        /// that map to gemini built-ins (Bash, Read, Write, Edit,
        /// Glob, Grep) — see `validate_ai_core_tools`.
        default_tools: Option<Vec<String>>,
    },
    /// OpenAI ChatGPT-subscription core — targets the Codex
    /// `/responses` surface on `chatgpt.com/backend-api/codex`, gated
    /// by the user's ChatGPT Plus/Pro/Team entitlement. Authentication
    /// is OAuth 2.0 (Authorization Code + PKCE) against
    /// `auth.openai.com`; the login flow runs separately (`aciddog
    /// auth login chatgpt --name <credential>`) and persists tokens
    /// keyed by `credential`. Tools use the Responses API top-level
    /// shape; the ChatGPT endpoint enforces a `store: false` stateless
    /// contract, so history is resent on every turn.
    OpenAiChatgpt {
        /// Credential name stored in `auth.json` (e.g. `personal`,
        /// `work`). Validated as a safe identifier at parse time; the
        /// credential's presence is checked lazily at first turn so
        /// users can edit config before completing login.
        credential: String,
        /// Model id sent in the `model` request field (e.g.
        /// `gpt-5.1-codex`). Also drives which bundled Codex prompt
        /// file is sent as `instructions`.
        model: String,
        /// Optional user-supplied system prompt. **Not** placed in the
        /// Responses API `instructions` field (which the backend
        /// expects to be the Codex prompt verbatim); instead
        /// delivered as a `role: "developer"` message at the head of
        /// `input[]` on every turn.
        system_prompt: Option<String>,
        /// Per-core default tool allowlist. Same allowlist as
        /// `AnthropicApi`/`OpenAiApi` — unknown names rejected at
        /// parse time.
        default_tools: Option<Vec<String>>,
        /// `low` / `medium` (default) / `high`. `minimal` is rejected
        /// at parse time because the ChatGPT endpoint returns HTTP
        /// 400 for it.
        reasoning_effort: Option<String>,
    },
    /// Generic OpenAI-chat-completions-compatible core. Works against
    /// api.openai.com, OpenRouter, Ollama's OpenAI-compatible endpoint,
    /// or any other vendor that implements `/chat/completions`.
    OpenAiApi {
        /// **v1 base** URL of the API (e.g. `https://api.openai.com/v1`,
        /// `https://openrouter.ai/api/v1`, `http://localhost:11434/v1`).
        /// The core appends `/chat/completions`. Matches how every
        /// OpenAI-compatible vendor documents their endpoint.
        api_url: String,
        /// Name of the environment variable holding the API key.
        /// Resolved at turn time, same as the Anthropic variant.
        api_key_env: String,
        /// Model id sent in the `model` request field (e.g.
        /// `gpt-4o-mini`, `openai/gpt-5.4-mini` on OpenRouter).
        model: String,
        /// Optional system-prompt content; prepended as a
        /// `role: "system"` message at request time.
        system_prompt: Option<String>,
        /// Per-core default tool allowlist. Unknown names are rejected
        /// at config parse time (see `validate_ai_core_tools`).
        default_tools: Option<Vec<String>>,
    },
    /// Google Jules remote-worker core. Creates a session on jules.google
    /// via the Jules API, polls the session's activity stream, and
    /// translates Jules state transitions into aciddog IPC (mail to the
    /// parent when plan approval / user feedback is awaited; worker
    /// complete on terminal states). Only valid for worker templates —
    /// requires `properties.repo` ("owner/repo") at spawn time.
    Jules {
        /// Environment variable holding the Jules API key (created at
        /// jules.google.com/settings). Resolved at turn time.
        api_key_env: String,
        /// Fallback `sourceContext.githubRepoContext.startingBranch`
        /// when the spawn-time properties don't pass `branch`. `None`
        /// lets Jules default to the repo's default branch.
        default_branch: Option<String>,
        /// How often to poll `/activities` and `/sessions/{id}` while a
        /// turn is in flight. Clamped 3..=120 seconds at build time.
        /// Defaults to 10s.
        poll_interval_seconds: Option<u64>,
        /// Whether to pass `requirePlanApproval = true` when creating
        /// the session. Defaults to true so plans are gated on the
        /// parent's approval (via the magic `"approve"` reply).
        require_plan_approval: Option<bool>,
        /// `AUTOMATION_MODE_UNSPECIFIED` (default) or `AUTO_CREATE_PR`.
        /// Forwarded verbatim as the `automationMode` field on session
        /// creation.
        automation_mode: Option<String>,
    },
}

impl AiCore {
    /// Return the `default_tools` declared on this core's variant, if
    /// any. Common accessor so `effective_tools` doesn't have to match
    /// every caller.
    pub fn default_tools(&self) -> Option<&[String]> {
        match &self.kind {
            AiCoreKind::ClaudeCode { default_tools, .. }
            | AiCoreKind::AnthropicApi { default_tools, .. }
            | AiCoreKind::OpenAiApi { default_tools, .. }
            | AiCoreKind::OpenAiChatgpt { default_tools, .. }
            | AiCoreKind::CopilotCli { default_tools, .. }
            | AiCoreKind::GeminiCli { default_tools, .. } => default_tools.as_deref(),
            // Jules runs on Google's remote sandbox — it doesn't use
            // aciddog's built-in tools. Tools are only relevant for the
            // tag-IPC preamble Jules sees on its first prompt, and we
            // intentionally keep that minimal (no Bash; tag protocol
            // always applies).
            AiCoreKind::Jules { .. } => None,
        }
    }

    /// Model id this core will send to the backend, if the variant
    /// pins one. `ClaudeCode` returns `None` because the model is
    /// chosen by the `claude` CLI itself; `AnthropicApi` returns the
    /// configured `model` field.
    pub fn runtime_model(&self) -> Option<&str> {
        match &self.kind {
            AiCoreKind::ClaudeCode { model, .. } => model.as_deref(),
            AiCoreKind::AnthropicApi { model, .. }
            | AiCoreKind::OpenAiApi { model, .. }
            | AiCoreKind::OpenAiChatgpt { model, .. } => Some(model.as_str()),
            AiCoreKind::CopilotCli { model, .. } | AiCoreKind::GeminiCli { model, .. } => {
                model.as_deref()
            }
            // Jules chooses its own model on the backend; we never
            // pass a `--model` equivalent.
            AiCoreKind::Jules { .. } => None,
        }
    }

    /// Resolve which `UsageProvider` applies to this core. Explicit
    /// `usage_provider` on the config always wins; no variant infers
    /// one by default. `claudeCode` deliberately returns `None` here —
    /// its reset data is populated from `rate_limit_event` stream
    /// lines, not from an HTTP fetch, so there is no provider to
    /// dispatch on.
    pub fn effective_usage_provider(&self) -> Option<UsageProvider> {
        self.usage_provider
    }
}

/// A named compaction policy. Referenced by id from an agent or worker
/// via their `compaction` attribute. The `strategy` tagged union picks
/// which variant's fields apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Compaction {
    pub id: String,
    pub strategy: CompactionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "strategy", rename_all = "camelCase")]
pub enum CompactionStrategy {
    /// Forward `/compact` to the backing CLI harness as the next user
    /// prompt. Rejected at parse time for `anthropicApi` / `openAiApi`
    /// cores, which have no slash-command concept.
    Harness,
    /// Drop the current session and re-seed a fresh one with the
    /// first-turn preamble.
    Drop,
    /// Send one final prompt to the existing session (capturing the
    /// reply), then drop + re-seed as in `Drop`.
    PromptAndDrop { prompt: String },
    /// Serialize the TUI transcript, run it through a throwaway core
    /// with the configured summary prompt, then drop + re-seed and
    /// deliver the summary as the first user message of the new
    /// session.
    Summarise {
        prompt: String,
        /// ai_core id for the summary pass. `None` falls back to the
        /// target agent's own core (which in turn falls back to
        /// `team.default_ai_core`).
        ai_core: Option<String>,
    },
}

impl CompactionStrategy {
    /// Short name used in log messages and the post-compaction marker
    /// block. Must match the tag used in the WCL `strategy` symbol so
    /// users recognise it.
    pub fn name(&self) -> &'static str {
        match self {
            CompactionStrategy::Harness => "harness",
            CompactionStrategy::Drop => "drop",
            CompactionStrategy::PromptAndDrop { .. } => "promptAndDrop",
            CompactionStrategy::Summarise { .. } => "summarise",
        }
    }
}

/// What `/reload` does to an agent whose effective config changed. Set
/// per-agent via `on_reload = :ignore | :compact | :reset`. `Ignore`
/// (default) keeps the running session intact — the new config is picked
/// up the next time the agent compacts or resets, since
/// `teardown_and_rebuild` already re-reads from `App::config`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ReloadAction {
    /// Leave the running session alone. New config takes effect at the
    /// next `/compact` or `/reset`.
    #[default]
    Ignore,
    /// Run the agent's configured compaction strategy immediately.
    Compact,
    /// Drop the session and re-seed (equivalent to `/reset`).
    Reset,
}

impl ReloadAction {
    pub fn name(&self) -> &'static str {
        match self {
            ReloadAction::Ignore => "ignore",
            ReloadAction::Compact => "compact",
            ReloadAction::Reset => "reset",
        }
    }
}

/// A named, repeatable procedure with typed properties and sequential
/// steps. Declared at the fleet level; triggered on demand by a user
/// (from the TUI) or an agent (via `aciddog workflow run`). Each trigger
/// creates an ephemeral in-memory instance; nothing is persisted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workflow {
    pub id: String,
    pub description: String,
    /// Upper bound on the number of times `conditional` + `check` steps
    /// may jump back to their `target` across a single instance. When
    /// exceeded the instance fails.
    pub max_retries: u32,
    pub properties: Vec<WorkflowProperty>,
    pub steps: Vec<WorkflowStep>,
}

/// A typed input to a workflow. User-visible kinds appear as fields in
/// the property-entry form; `Hidden` is invisible and only mutated by
/// workers via `workflow_set`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowProperty {
    pub id: String,
    /// Optional human-readable label for the form. Falls back to `id`
    /// when absent.
    pub label: Option<String>,
    /// Required user inputs block form submission until populated.
    /// `Hidden` properties ignore this flag (they never appear in the
    /// form); `required` on them is accepted but has no effect.
    pub required: bool,
    pub kind: PropertyKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PropertyKind {
    Text {
        default: Option<String>,
    },
    Number {
        default: Option<f64>,
        min: Option<f64>,
        max: Option<f64>,
    },
    Select {
        options: Vec<String>,
        default: Option<String>,
    },
    MultiSelect {
        options: Vec<String>,
        default: Option<Vec<String>>,
    },
    /// Never surfaced in the form. Seeded from `default` at instance
    /// start, then mutated at runtime via the `workflow_set` tool.
    /// Stored as a string (workers can coerce as needed).
    Hidden {
        default: Option<String>,
    },
}

impl PropertyKind {
    /// Whether this property appears in the property-entry form.
    pub fn is_user_visible(&self) -> bool {
        !matches!(self, PropertyKind::Hidden { .. })
    }

    /// Discriminator name matching the WCL `kind` symbol.
    pub fn name(&self) -> &'static str {
        match self {
            PropertyKind::Text { .. } => "text",
            PropertyKind::Number { .. } => "number",
            PropertyKind::Select { .. } => "select",
            PropertyKind::MultiSelect { .. } => "multiSelect",
            PropertyKind::Hidden { .. } => "hidden",
        }
    }
}

/// A single step in a workflow. Steps are keyed by `id` so
/// `Conditional.target` and `Check.target` can jump to one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    pub id: String,
    pub kind: StepKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StepKind {
    /// Spawn a worker template with a rendered prompt. Step completes
    /// when the worker calls `aciddog worker complete`.
    Worker { worker: String, prompt: String },
    /// Run `/bin/sh -c <rendered>`. Non-zero exit fails the instance.
    Command {
        command: String,
        cwd: Option<String>,
    },
    /// Spawn a worker that must call `check_successful` to pass. Any
    /// other outcome (pass=false, or hibernation without the call)
    /// counts as a failure and jumps back to `target`, bounded by
    /// `Workflow::max_retries`.
    Conditional {
        worker: String,
        prompt: String,
        target: String,
    },
    /// Pure variable predicate — no worker spawned. Same
    /// retries-bounded jump semantics as `Conditional`.
    Check {
        variable: String,
        op: CheckOp,
        /// Required for `Equals` / `NotEquals` / `Contains`; rejected
        /// for `Empty` / `NotEmpty`.
        value: Option<String>,
        target: String,
    },
}

impl StepKind {
    /// Discriminator name matching the WCL `kind` symbol.
    pub fn name(&self) -> &'static str {
        match self {
            StepKind::Worker { .. } => "worker",
            StepKind::Command { .. } => "command",
            StepKind::Conditional { .. } => "conditional",
            StepKind::Check { .. } => "check",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CheckOp {
    Equals,
    NotEquals,
    Contains,
    Empty,
    NotEmpty,
}

impl CheckOp {
    pub fn name(self) -> &'static str {
        match self {
            CheckOp::Equals => "equals",
            CheckOp::NotEquals => "notEquals",
            CheckOp::Contains => "contains",
            CheckOp::Empty => "empty",
            CheckOp::NotEmpty => "notEmpty",
        }
    }

    pub fn needs_value(self) -> bool {
        matches!(
            self,
            CheckOp::Equals | CheckOp::NotEquals | CheckOp::Contains
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AciddogConfig {
    pub team: Team,
    pub agents: Vec<Agent>,
    pub workers: Vec<Worker>,
    /// Script-backed pseudo-agents declared at the fleet level. See
    /// [`Automaton`] for the runtime model.
    pub automatons: Vec<Automaton>,
    pub ai_cores: Vec<AiCore>,
    /// Named memory banks available to this team. Agents/workers opt in
    /// to specific banks via their `memory_banks` allowlist.
    pub memory_banks: Vec<MemoryBank>,
    /// Named compaction policies. Agents/workers opt in by id via their
    /// `compaction` attribute.
    pub compactions: Vec<Compaction>,
    /// Virtual chat rooms declared at the fleet level. An agent
    /// participates in a channel iff its id appears in that channel's
    /// `members` list.
    pub channels: Vec<Channel>,
    /// Named workflows declared at the fleet level. Agents opt in to
    /// trigger a workflow via their `can_run_workflows` allowlist.
    pub workflows: Vec<Workflow>,
    /// Inbound HTTP webhook endpoints. Each one mounts on a
    /// webservice-backed automaton and delivers request payloads to
    /// a configured agent/automaton as a message.
    pub webhooks: Vec<Webhook>,
    /// Final pool of messages shown by the thinking indicator while an
    /// agent is `Working`. Already merged with built-ins per
    /// `team.replace_thinking_messages`; guaranteed non-empty.
    pub thinking_messages: Vec<String>,
}

/// Built-in fallback tool allowlist used when neither the agent/worker
/// nor the team declares a list.
pub const DEFAULT_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Edit", "Grep", "Glob"];

/// Default pool of whimsical thinking messages. Users can extend or
/// replace this via `thinking_messages { messages = [...] }` blocks in
/// their WCL config.
pub const BUILT_IN_THINKING_MESSAGES: &[&str] = &[
    "petting digital cats",
    "asking the rubber duck",
    "compiling witty remarks",
    "reticulating splines",
    "bribing the optimizer",
    "untangling callback spaghetti",
    "counting semicolons",
    "negotiating with the type checker",
    "feeding the hamster",
    "teaching the linter manners",
    "summoning stack overflow",
    "yak shaving (it's fine)",
    "mashing keys thoughtfully",
    "locating the coffee",
    "herding async cats",
    "debating tabs vs spaces",
    "rebooting the universe",
    "decoding ancient regex",
    "polishing edge cases",
    "kissing the BIOS",
    "consulting the oracle",
    "rolling for initiative",
    "greasing the pipeline",
    "convincing the borrow checker",
    "rewriting it in Rust",
    "bartering with the GPU",
    "chasing rogue pointers",
    "sharpening the hash",
    "rehydrating neurons",
    "placating the kernel",
    "inverting the binary tree",
    "stirring the tensor soup",
    "whispering to the daemon",
    "refactoring my thoughts",
    "booting the brain cell",
    "warming up the neurons",
    "aligning the bits",
    "drawing the rest of the owl",
    "blaming the cache",
    "doing a barrel roll",
    "calling my lawyer",
    "lighting the incense",
    "writing a haiku about it",
    "pondering the orb",
    "googling the answer, don't tell anyone",
];

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("WCL parse errors in {path}:\n{details}")]
    Parse { path: String, details: String },

    #[error("missing required `team` block in config")]
    MissingTeam,

    #[error("block `{kind}` is missing required attribute `{attr}`")]
    MissingAttribute { kind: String, attr: String },

    #[error("block `{kind}` attribute `{attr}` is not a string")]
    BadAttributeType { kind: String, attr: String },

    #[error("block `{kind}` is missing an id (expected `{kind} <id> {{ ... }}`)")]
    MissingId { kind: String },

    #[error("block `{kind}` attribute `{attr}` must be a list")]
    NotAList { kind: String, attr: String },

    #[error("block `{kind}` attribute `{attr}` contains a non-string element")]
    BadListElementType { kind: String, attr: String },

    #[error("agent `{from}` references unknown agent `{to}` in `{attr}`")]
    UnknownAgentRef {
        from: String,
        attr: String,
        to: String,
    },

    #[error("`{from}` references unknown worker template `{to}` in `can_create_workers`")]
    UnknownWorkerRef { from: String, to: String },

    #[error("`{from}` references unknown ai_core `{to}` in `{attr}`")]
    UnknownAiCoreRef {
        from: String,
        attr: String,
        to: String,
    },

    #[error("ai_core `{core}` has unknown type `{type_name}`")]
    UnknownAiCoreType { core: String, type_name: String },

    #[error("ai_core `{core}` has unknown usage_provider `{value}` (expected `:zai`, `:chatgpt_oauth`, `:openrouter`, or `:github_copilot`)")]
    UnknownUsageProvider { core: String, value: String },

    #[error("`{from}` references unknown memory bank `{to}` in `memory_banks`")]
    UnknownMemoryBankRef { from: String, to: String },

    #[error("ai_core `{core}` (type `{type_name}`) lists unknown built-in tool `{tool}` in `default_tools`")]
    UnknownTool {
        core: String,
        type_name: String,
        tool: String,
    },

    #[error("`{from}` references skill `{name}` whose id is not a valid identifier")]
    InvalidSkillName { from: String, name: String },

    #[error("compaction `{core}` has unknown strategy `{strategy}`")]
    UnknownCompactionStrategy { core: String, strategy: String },

    #[error("agent `{agent}` has unknown `on_reload` value `{value}` (expected `:ignore`, `:compact`, or `:reset`)")]
    InvalidReloadAction { agent: String, value: String },

    #[error("`{from}` references unknown compaction `{to}` in `compaction`")]
    UnknownCompactionRef { from: String, to: String },

    #[error("channel `{channel}` references unknown member `{member}` in `members` (must be a declared `agent` or `automaton`, not a worker template)")]
    UnknownChannelMember { channel: String, member: String },

    #[error("channel `{channel}` has a duplicate member `{member}` in `members`")]
    DuplicateChannelMember { channel: String, member: String },

    #[error("duplicate channel id `{id}` — channel ids must be unique")]
    DuplicateChannelId { id: String },

    #[error(
        "compaction `{id}` on `{actor}` uses strategy `harness`, but that \
         agent/worker resolves to ai_core type `{core_type}`. Only \
         `claudeCode` actually processes `/compact` in non-interactive \
         mode — use `drop` or `summarise` for {core_type} cores."
    )]
    CompactionHarnessUnsupportedCore {
        id: String,
        actor: String,
        core_type: String,
    },

    #[error("workflow `{workflow}` has an unexpected child block `{child}` (only `property` and `step` are allowed)")]
    UnknownWorkflowChildBlock { workflow: String, child: String },

    #[error("property `{property}` has unknown kind `{kind}` (expected :text, :number, :select, :multiSelect, or :hidden)")]
    UnknownPropertyKind { property: String, kind: String },

    #[error("step `{step}` has unknown kind `{kind}` (expected :worker, :command, :conditional, or :check)")]
    UnknownStepKind { step: String, kind: String },

    #[error("check step `{step}` has unknown op `{op}` (expected :equals, :notEquals, :contains, :empty, or :notEmpty)")]
    UnknownCheckOp { step: String, op: String },

    #[error("workflow `{workflow}` has duplicate property id `{property}`")]
    DuplicatePropertyId { workflow: String, property: String },

    #[error("workflow `{workflow}` has duplicate step id `{step}`")]
    DuplicateStepId { workflow: String, step: String },

    #[error("workflow `{workflow}` property `{property}` default `{default}` is not in its options list")]
    InvalidPropertyDefault {
        workflow: String,
        property: String,
        default: String,
    },

    #[error(
        "workflow `{workflow}` property `{property}` has min ({min}) greater than max ({max})"
    )]
    InvalidPropertyRange {
        workflow: String,
        property: String,
        min: f64,
        max: f64,
    },

    #[error("workflow `{workflow}` step `{step}` references unknown worker template `{worker}`")]
    UnknownWorkflowWorkerRef {
        workflow: String,
        step: String,
        worker: String,
    },

    #[error("workflow `{workflow}` step `{step}` targets unknown step id `{target}`")]
    UnknownWorkflowStepTarget {
        workflow: String,
        step: String,
        target: String,
    },

    #[error("workflow `{workflow}` check step `{step}` references unknown property `{variable}`")]
    UnknownWorkflowVariable {
        workflow: String,
        step: String,
        variable: String,
    },

    #[error("workflow `{workflow}` check step `{step}` op `{op}` requires a `value`")]
    CheckMissingValue {
        workflow: String,
        step: String,
        op: String,
    },

    #[error("workflow `{workflow}` check step `{step}` op `{op}` must not supply a `value`")]
    CheckUnexpectedValue {
        workflow: String,
        step: String,
        op: String,
    },

    #[error("`{from}` references unknown workflow `{to}` in `can_run_workflows`")]
    UnknownWorkflowRef { from: String, to: String },

    #[error("ai_core `{core}` credential `{credential}` is invalid (must match `[a-z0-9_-]+`, length 1..=64)")]
    InvalidCredentialName { core: String, credential: String },

    #[error(
        "ai_core `{core}` reasoningEffort `{value}` is invalid (expected `low`, `medium`, `high`; `minimal` is rejected by the ChatGPT endpoint)"
    )]
    InvalidReasoningEffort { core: String, value: String },

    #[error("automaton `{automaton}` env entry `{entry}` is malformed (expected `KEY=VAL`)")]
    BadEnvEntry { automaton: String, entry: String },

    #[error("automaton `{automaton}` has unknown type `{type_name}` (expected `:script`, `:webservice`, or `:remote`)")]
    UnknownAutomatonType {
        automaton: String,
        type_name: String,
    },

    #[error("automaton `{automaton}` is `:remote`-backed but sets field `{field}` (only valid on `:script` / `:webservice` backends)")]
    RemoteUnexpectedField { automaton: String, field: String },

    #[error(
        "automaton `{automaton}` has both `url` (HTTPS) and `socket_path` (Unix) set — pick one"
    )]
    RemoteTransportConflict { automaton: String },

    #[error(
        "automaton `{automaton}` has no `url` or `socket_path` set — one is required for `:remote`"
    )]
    RemoteMissingTransport { automaton: String },

    #[error("automaton `{automaton}` is an HTTPS `:remote` automaton; `api_key_env` is required")]
    RemoteMissingApiKeyEnv { automaton: String },

    #[error("automaton `{automaton}` is a Unix-socket `:remote` automaton; `api_key_env` is rejected (trust model is filesystem perms)")]
    RemoteUnixSocketApiKey { automaton: String },

    #[error("automaton `{automaton}` is a Unix-socket `:remote` automaton; `tls_ca` is rejected (no TLS on UDS)")]
    RemoteUnixSocketTlsCa { automaton: String },

    #[error("automaton `{automaton}` is `:remote`-backed; `remote_target` is required")]
    RemoteMissingTarget { automaton: String },

    #[error("automaton `{automaton}` is `:remote`-backed; `deliver_to` is required")]
    RemoteMissingDeliverTo { automaton: String },

    #[error("automaton `{automaton}` `deliver_to = \"{target}\"` does not reference a declared agent or automaton")]
    RemoteUnknownDeliverTo { automaton: String, target: String },

    #[error("automaton `{automaton}` is `:webservice`-backed but sets script field `{field}` (only valid on `:script`-backed automatons)")]
    WebserviceUnexpectedScriptField { automaton: String, field: String },

    #[error("automaton `{automaton}` is `:script`-backed but sets webservice field `{field}` (set `type = :webservice` or remove the field)")]
    ScriptUnexpectedWebserviceField { automaton: String, field: String },

    #[error(
        "automaton `{automaton}` has both `bind` (HTTPS) and `socket_path` (Unix) set — pick one"
    )]
    WebserviceBindConflict { automaton: String },

    #[error("automaton `{automaton}` has no `bind` or `socket_path` set — one is required for `:webservice`")]
    WebserviceMissingBind { automaton: String },

    #[error("automaton `{automaton}` (HTTPS) is missing required `{field}`")]
    WebserviceMissingHttpsField { automaton: String, field: String },

    #[error("automaton `{automaton}` is a Unix-socket webservice; `api_key_env` is rejected (trust model is filesystem perms)")]
    WebserviceUnixSocketApiKey { automaton: String },

    #[error("webhook `{webhook}` mounts on unknown automaton `{mount}`")]
    WebhookUnknownMount { webhook: String, mount: String },

    #[error(
        "webhook `{webhook}` mounts on `{mount}`, which is not a `:webservice`-backed automaton"
    )]
    WebhookMountNotWebservice { webhook: String, mount: String },

    #[error("webhook `{webhook}` delivers to unknown agent/automaton `{target}`")]
    WebhookUnknownDeliverTarget { webhook: String, target: String },

    #[error("webhook `{webhook}` delivers to `{target}`, which is not in mount automaton `{mount}`'s `can_message` allowlist")]
    WebhookDeliverNotReachable {
        webhook: String,
        target: String,
        mount: String,
    },

    #[error("webhook `{webhook}` has duplicate path `{path}` on mount `{mount}`")]
    WebhookPathConflict {
        webhook: String,
        mount: String,
        path: String,
    },

    #[error("webhook `{webhook}` path `{path}` collides with a reserved webservice path (`/v1/rpc`, `/v1/events`, `/v1/health`)")]
    WebhookReservedPath { webhook: String, path: String },

    #[error("webhook `{webhook}` path `{path}` is invalid (must start with `/`)")]
    WebhookInvalidPath { webhook: String, path: String },

    #[error("webhook `{webhook}` auth `{auth_kind}` is missing required field `{field}`")]
    WebhookAuthMissingField {
        webhook: String,
        auth_kind: String,
        field: String,
    },

    #[error("webhook `{webhook}` has unknown auth kind `{auth_kind}` (expected `:none`, `:bearer`, `:header`, `:json_field`, or `:hmac_header`)")]
    WebhookUnknownAuthKind { webhook: String, auth_kind: String },

    #[error("webhook `{webhook}` hmac algo `{algo}` is invalid (expected `:sha1` or `:sha256`)")]
    WebhookUnknownHmacAlgo { webhook: String, algo: String },

    #[error(
        "{kind} id `{id}` is reserved (the `user` id is reserved for the human operator's inbox)"
    )]
    ReservedId { kind: String, id: String },
}

pub fn load_config(path: &Path) -> Result<AciddogConfig, ConfigError> {
    let source = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_config(&source, path)
}

pub fn parse_config(source: &str, origin: &Path) -> Result<AciddogConfig, ConfigError> {
    let doc = parse(source, ParseOptions::default());

    if doc.has_errors() {
        let details = doc
            .errors()
            .iter()
            .map(|d| format!("  {}: {}", d.code.as_deref().unwrap_or("?"), d.message,))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(ConfigError::Parse {
            path: origin.display().to_string(),
            details,
        });
    }

    let team_blocks = doc.blocks_of_type_resolved("team");
    let team_block = team_blocks.first().ok_or(ConfigError::MissingTeam)?;
    let team = Team {
        id: require_id(&team_block.id, &team_block.kind)?,
        description: require_string(team_block, "description")?,
        replace_thinking_messages: optional_bool(team_block, "replace_thinking_messages")?
            .unwrap_or(false),
        default_ai_core: optional_string(team_block, "default_ai_core"),
    };

    let agents = doc
        .blocks_of_type_resolved("agent")
        .iter()
        .map(|b| {
            let id = require_id(&b.id, &b.kind)?;
            let on_reload = match optional_symbol(b, "on_reload")? {
                None => ReloadAction::Ignore,
                Some(sym) => match sym.as_str() {
                    "ignore" => ReloadAction::Ignore,
                    "compact" => ReloadAction::Compact,
                    "reset" => ReloadAction::Reset,
                    other => {
                        return Err(ConfigError::InvalidReloadAction {
                            agent: id.clone(),
                            value: other.to_string(),
                        });
                    }
                },
            };
            Ok(Agent {
                id,
                role: require_string(b, "role")?,
                role_description: optional_string(b, "role_description"),
                model: optional_string(b, "model"),
                instructions: require_string(b, "instructions")?,
                can_message: optional_string_list(b, "can_message")?,
                can_create_workers: optional_string_list(b, "can_create_workers")?,
                can_run_workflows: optional_string_list(b, "can_run_workflows")?,
                tools: optional_string_list(b, "tools")?,
                ai_core: optional_string(b, "ai_core"),
                memory_banks: optional_string_list(b, "memory_banks")?,
                skills: optional_string_list(b, "skills")?,
                compaction: optional_string(b, "compaction"),
                supervisor: optional_bool(b, "supervisor")?.unwrap_or(false),
                sysadmin: optional_bool(b, "sysadmin")?.unwrap_or(false),
                leader: optional_bool(b, "leader")?.unwrap_or(false),
                on_reload,
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?
        .into_iter()
        .map(|mut a: Agent| {
            // sysadmin implies supervisor.
            if a.sysadmin {
                a.supervisor = true;
            }
            a
        })
        .collect::<Vec<_>>();

    let workers = doc
        .blocks_of_type_resolved("worker")
        .iter()
        .map(|b| {
            Ok(Worker {
                id: require_id(&b.id, &b.kind)?,
                role: require_string(b, "role")?,
                role_description: optional_string(b, "role_description"),
                model: optional_string(b, "model"),
                instructions: require_string(b, "instructions")?,
                tools: optional_string_list(b, "tools")?,
                can_create_workers: optional_string_list(b, "can_create_workers")?,
                can_run_workflows: optional_string_list(b, "can_run_workflows")?,
                ai_core: optional_string(b, "ai_core"),
                memory_banks: optional_string_list(b, "memory_banks")?,
                skills: optional_string_list(b, "skills")?,
                compaction: optional_string(b, "compaction"),
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let automatons = doc
        .blocks_of_type_resolved("automaton")
        .iter()
        .map(parse_automaton)
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let webhooks = doc
        .blocks_of_type_resolved("webhook")
        .iter()
        .map(parse_webhook)
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let memory_banks = doc
        .blocks_of_type_resolved("memory_bank")
        .iter()
        .map(|b| {
            Ok(MemoryBank {
                id: require_id(&b.id, &b.kind)?,
                description: require_string(b, "description")?,
                embed_model: optional_string(b, "embed_model"),
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let ai_cores = doc
        .blocks_of_type_resolved("ai_core")
        .iter()
        .map(|b| {
            let id = require_id(&b.id, &b.kind)?;
            let type_name = require_symbol(b, "type")?;
            let kind = match type_name.as_str() {
                "claudeCode" => AiCoreKind::ClaudeCode {
                    system_prompt: optional_string(b, "systemPrompt"),
                    model: optional_string(b, "model"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                "anthropicApi" => AiCoreKind::AnthropicApi {
                    api_url: require_string(b, "apiUrl")?,
                    api_key_env: require_string(b, "apiKeyEnv")?,
                    model: require_string(b, "model")?,
                    system_prompt: optional_string(b, "systemPrompt"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                "openAiApi" => AiCoreKind::OpenAiApi {
                    api_url: require_string(b, "apiUrl")?,
                    api_key_env: require_string(b, "apiKeyEnv")?,
                    model: require_string(b, "model")?,
                    system_prompt: optional_string(b, "systemPrompt"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                "openAiChatgpt" => {
                    let credential = require_string(b, "credential")?;
                    if !is_valid_credential_name(&credential) {
                        return Err(ConfigError::InvalidCredentialName {
                            core: id.clone(),
                            credential,
                        });
                    }
                    let reasoning_effort = optional_string(b, "reasoningEffort");
                    if let Some(v) = &reasoning_effort {
                        if !matches!(v.as_str(), "low" | "medium" | "high") {
                            return Err(ConfigError::InvalidReasoningEffort {
                                core: id.clone(),
                                value: v.clone(),
                            });
                        }
                    }
                    AiCoreKind::OpenAiChatgpt {
                        credential,
                        model: require_string(b, "model")?,
                        system_prompt: optional_string(b, "systemPrompt"),
                        default_tools: optional_string_list(b, "default_tools")?,
                        reasoning_effort,
                    }
                }
                "copilotCli" => AiCoreKind::CopilotCli {
                    model: optional_string(b, "model"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                "geminiCli" => AiCoreKind::GeminiCli {
                    model: optional_string(b, "model"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                "jules" => AiCoreKind::Jules {
                    api_key_env: require_string(b, "apiKeyEnv")?,
                    default_branch: optional_string(b, "defaultBranch"),
                    poll_interval_seconds: optional_u32(b, "pollIntervalSeconds")?
                        .map(|v| v as u64),
                    require_plan_approval: optional_bool(b, "requirePlanApproval")?,
                    automation_mode: optional_string(b, "automationMode"),
                },
                other => {
                    return Err(ConfigError::UnknownAiCoreType {
                        core: id,
                        type_name: other.to_string(),
                    })
                }
            };
            let usage_provider = match optional_symbol(b, "usage_provider")? {
                None => None,
                Some(sym) => match sym.as_str() {
                    "zai" => Some(UsageProvider::Zai),
                    "chatgpt_oauth" => Some(UsageProvider::ChatgptOauth),
                    "openrouter" => Some(UsageProvider::Openrouter),
                    "github_copilot" => Some(UsageProvider::GithubCopilot),
                    other => {
                        return Err(ConfigError::UnknownUsageProvider {
                            core: id,
                            value: other.to_string(),
                        })
                    }
                },
            };
            Ok(AiCore {
                id,
                kind,
                usage_provider,
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let compactions = doc
        .blocks_of_type_resolved("compaction")
        .iter()
        .map(|b| {
            let id = require_id(&b.id, &b.kind)?;
            let strategy_name = require_symbol(b, "strategy")?;
            let strategy = match strategy_name.as_str() {
                "harness" => CompactionStrategy::Harness,
                "drop" => CompactionStrategy::Drop,
                "promptAndDrop" => CompactionStrategy::PromptAndDrop {
                    prompt: require_string(b, "prompt")?,
                },
                "summarise" => CompactionStrategy::Summarise {
                    prompt: require_string(b, "prompt")?,
                    ai_core: optional_string(b, "ai_core"),
                },
                other => {
                    return Err(ConfigError::UnknownCompactionStrategy {
                        core: id,
                        strategy: other.to_string(),
                    })
                }
            };
            Ok(Compaction { id, strategy })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let channels = doc
        .blocks_of_type_resolved("channel")
        .iter()
        .map(|b| {
            let id = require_id(&b.id, &b.kind)?;
            // `members` is required; treat "missing" as an explicit
            // error so misspelled attributes don't silently produce a
            // dead channel.
            let members = optional_string_list(b, "members")?.ok_or_else(|| {
                ConfigError::MissingAttribute {
                    kind: b.kind.clone(),
                    attr: "members".to_string(),
                }
            })?;
            Ok(Channel {
                id,
                description: optional_string(b, "description"),
                welcome: optional_string(b, "welcome"),
                members,
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let workflows = doc
        .blocks_of_type_resolved("workflow")
        .iter()
        .map(parse_workflow)
        .collect::<Result<Vec<_>, ConfigError>>()?;

    validate_reserved_ids(&agents, &automatons, &workers, &channels, &webhooks)?;
    validate_agent_refs(&agents, &automatons)?;
    validate_worker_refs(&agents, &workers)?;
    validate_ai_core_refs(&team, &agents, &workers, &ai_cores)?;
    validate_ai_core_tools(&ai_cores)?;
    validate_memory_bank_refs(&agents, &workers, &memory_banks)?;
    validate_skill_names(&agents, &workers)?;
    validate_compaction_refs(&team, &agents, &workers, &ai_cores, &compactions)?;
    validate_channel_refs(&agents, &automatons, &channels)?;
    validate_workflow_refs(&workflows, &workers)?;
    validate_workflow_allowlists(&agents, &workers, &workflows)?;
    validate_automaton_refs(&agents, &automatons)?;
    validate_webhooks(&agents, &automatons, &webhooks)?;

    let mut user_messages: Vec<String> = Vec::new();
    for block in doc.blocks_of_type_resolved("thinking_messages").iter() {
        if let Some(list) = optional_string_list(block, "messages")? {
            user_messages.extend(list);
        }
    }

    let mut thinking_messages: Vec<String> =
        if team.replace_thinking_messages && !user_messages.is_empty() {
            user_messages
        } else {
            let mut merged: Vec<String> = BUILT_IN_THINKING_MESSAGES
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            merged.extend(user_messages);
            merged
        };

    // Guarantee the pool is never empty so the UI always has something
    // to show. Only reachable if the built-ins are disabled via replace
    // and the user supplied no messages.
    if thinking_messages.is_empty() {
        thinking_messages.push("thinking".to_string());
    }

    Ok(AciddogConfig {
        team,
        agents,
        workers,
        automatons,
        ai_cores,
        memory_banks,
        compactions,
        channels,
        workflows,
        webhooks,
        thinking_messages,
    })
}

fn parse_automaton(b: &wcl_lang::BlockRef) -> Result<Automaton, ConfigError> {
    let id = require_id(&b.id, &b.kind)?;
    let role = require_string(b, "role")?;
    let role_description = optional_string(b, "role_description");
    let can_message = optional_string_list(b, "can_message")?;

    let type_symbol = optional_symbol(b, "type")?;
    let backend = match type_symbol.as_deref() {
        None | Some("script") => {
            // Webservice / remote fields must not leak into a script backend.
            for field in [
                "bind",
                "socket_path",
                "tls_cert",
                "tls_key",
                "api_key_env",
                "url",
                "tls_ca",
                "remote_target",
                "deliver_to",
                "proxy_socket_path",
            ] {
                if b.attributes.contains_key(field) {
                    return Err(ConfigError::ScriptUnexpectedWebserviceField {
                        automaton: id.clone(),
                        field: field.to_string(),
                    });
                }
            }
            let env_raw = optional_string_list(b, "env")?.unwrap_or_default();
            let mut env: Vec<(String, String)> = Vec::with_capacity(env_raw.len());
            for entry in env_raw {
                match entry.split_once('=') {
                    Some((k, v)) if !k.is_empty() => env.push((k.to_string(), v.to_string())),
                    _ => {
                        return Err(ConfigError::BadEnvEntry {
                            automaton: id.clone(),
                            entry,
                        })
                    }
                }
            }
            AutomatonBackend::Script {
                command: require_string(b, "command")?,
                args: optional_string_list(b, "args")?.unwrap_or_default(),
                cwd: optional_string(b, "cwd"),
                env,
                timeout_seconds: optional_u32(b, "timeout_seconds")?,
            }
        }
        Some("webservice") => {
            for field in [
                "command",
                "args",
                "cwd",
                "env",
                "timeout_seconds",
                "url",
                "tls_ca",
                "remote_target",
                "deliver_to",
                "proxy_socket_path",
            ] {
                if b.attributes.contains_key(field) {
                    return Err(ConfigError::WebserviceUnexpectedScriptField {
                        automaton: id.clone(),
                        field: field.to_string(),
                    });
                }
            }
            let has_bind = b.attributes.contains_key("bind");
            let has_sock = b.attributes.contains_key("socket_path");
            let api_key_env = optional_string(b, "api_key_env");
            let bind = match (has_bind, has_sock) {
                (true, true) => {
                    return Err(ConfigError::WebserviceBindConflict {
                        automaton: id.clone(),
                    })
                }
                (false, false) => {
                    return Err(ConfigError::WebserviceMissingBind {
                        automaton: id.clone(),
                    })
                }
                (true, false) => {
                    let address = require_string(b, "bind")?;
                    let tls_cert = require_string(b, "tls_cert").map_err(|_| {
                        ConfigError::WebserviceMissingHttpsField {
                            automaton: id.clone(),
                            field: "tls_cert".to_string(),
                        }
                    })?;
                    let tls_key = require_string(b, "tls_key").map_err(|_| {
                        ConfigError::WebserviceMissingHttpsField {
                            automaton: id.clone(),
                            field: "tls_key".to_string(),
                        }
                    })?;
                    WebserviceBind::Https {
                        address,
                        tls_cert: PathBuf::from(tls_cert),
                        tls_key: PathBuf::from(tls_key),
                    }
                }
                (false, true) => {
                    if api_key_env.is_some() {
                        return Err(ConfigError::WebserviceUnixSocketApiKey {
                            automaton: id.clone(),
                        });
                    }
                    WebserviceBind::UnixSocket {
                        path: PathBuf::from(require_string(b, "socket_path")?),
                    }
                }
            };
            AutomatonBackend::Webservice { bind, api_key_env }
        }
        Some("remote") => {
            for field in [
                "command",
                "args",
                "cwd",
                "env",
                "timeout_seconds",
                "bind",
                "tls_cert",
                "tls_key",
            ] {
                if b.attributes.contains_key(field) {
                    return Err(ConfigError::RemoteUnexpectedField {
                        automaton: id.clone(),
                        field: field.to_string(),
                    });
                }
            }
            let has_url = b.attributes.contains_key("url");
            let has_sock = b.attributes.contains_key("socket_path");
            let api_key_env = optional_string(b, "api_key_env");
            let tls_ca = optional_string(b, "tls_ca").map(PathBuf::from);
            let transport = match (has_url, has_sock) {
                (true, true) => {
                    return Err(ConfigError::RemoteTransportConflict {
                        automaton: id.clone(),
                    })
                }
                (false, false) => {
                    return Err(ConfigError::RemoteMissingTransport {
                        automaton: id.clone(),
                    })
                }
                (true, false) => {
                    let url = require_string(b, "url")?;
                    let api_key_env =
                        api_key_env.ok_or_else(|| ConfigError::RemoteMissingApiKeyEnv {
                            automaton: id.clone(),
                        })?;
                    RemoteTransport::Https {
                        url,
                        api_key_env,
                        tls_ca,
                    }
                }
                (false, true) => {
                    if api_key_env.is_some() {
                        return Err(ConfigError::RemoteUnixSocketApiKey {
                            automaton: id.clone(),
                        });
                    }
                    if tls_ca.is_some() {
                        return Err(ConfigError::RemoteUnixSocketTlsCa {
                            automaton: id.clone(),
                        });
                    }
                    RemoteTransport::UnixSocket {
                        path: PathBuf::from(require_string(b, "socket_path")?),
                    }
                }
            };
            let remote_target = require_string(b, "remote_target").map_err(|_| {
                ConfigError::RemoteMissingTarget {
                    automaton: id.clone(),
                }
            })?;
            let deliver_to = require_string(b, "deliver_to").map_err(|_| {
                ConfigError::RemoteMissingDeliverTo {
                    automaton: id.clone(),
                }
            })?;
            let proxy_socket_path = optional_string(b, "proxy_socket_path").map(PathBuf::from);
            AutomatonBackend::Remote {
                transport,
                remote_target,
                deliver_to,
                proxy_socket_path,
            }
        }
        Some(other) => {
            return Err(ConfigError::UnknownAutomatonType {
                automaton: id.clone(),
                type_name: other.to_string(),
            })
        }
    };

    let sysadmin = optional_bool(b, "sysadmin")?.unwrap_or(false);
    // sysadmin implies supervisor, mirroring the agent rule.
    let supervisor = optional_bool(b, "supervisor")?.unwrap_or(false) || sysadmin;
    let leader = optional_bool(b, "leader")?.unwrap_or(false);
    Ok(Automaton {
        id,
        role,
        role_description,
        can_message,
        backend,
        supervisor,
        sysadmin,
        leader,
    })
}

fn parse_webhook(b: &wcl_lang::BlockRef) -> Result<Webhook, ConfigError> {
    let id = require_id(&b.id, &b.kind)?;
    let mount = require_string(b, "mount")?;
    let path = require_string(b, "path")?;
    if !path.starts_with('/') {
        return Err(ConfigError::WebhookInvalidPath {
            webhook: id.clone(),
            path,
        });
    }
    let deliver_to = require_string(b, "deliver_to")?;
    let max_body_bytes = optional_u32(b, "max_body_bytes")?.map(|n| n as usize);

    let auth = match optional_symbol(b, "auth_kind")?
        .as_deref()
        .unwrap_or("none")
    {
        "none" => WebhookAuth::None,
        "bearer" => {
            let secret_env = require_string(b, "auth_secret_env").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "bearer".to_string(),
                    field: "auth_secret_env".to_string(),
                }
            })?;
            WebhookAuth::Bearer { secret_env }
        }
        "header" => {
            let header = require_string(b, "auth_header").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "header".to_string(),
                    field: "auth_header".to_string(),
                }
            })?;
            let secret_env = require_string(b, "auth_secret_env").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "header".to_string(),
                    field: "auth_secret_env".to_string(),
                }
            })?;
            WebhookAuth::Header { header, secret_env }
        }
        "json_field" => {
            let field = require_string(b, "auth_field").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "json_field".to_string(),
                    field: "auth_field".to_string(),
                }
            })?;
            let secret_env = require_string(b, "auth_secret_env").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "json_field".to_string(),
                    field: "auth_secret_env".to_string(),
                }
            })?;
            WebhookAuth::JsonField { field, secret_env }
        }
        "hmac_header" => {
            let header = require_string(b, "auth_header").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "hmac_header".to_string(),
                    field: "auth_header".to_string(),
                }
            })?;
            let secret_env = require_string(b, "auth_secret_env").map_err(|_| {
                ConfigError::WebhookAuthMissingField {
                    webhook: id.clone(),
                    auth_kind: "hmac_header".to_string(),
                    field: "auth_secret_env".to_string(),
                }
            })?;
            let algo = match optional_symbol(b, "auth_algo")?.as_deref() {
                None | Some("sha256") => HmacAlgo::Sha256,
                Some("sha1") => HmacAlgo::Sha1,
                Some(other) => {
                    return Err(ConfigError::WebhookUnknownHmacAlgo {
                        webhook: id.clone(),
                        algo: other.to_string(),
                    })
                }
            };
            WebhookAuth::HmacHeader {
                header,
                secret_env,
                algo,
            }
        }
        other => {
            return Err(ConfigError::WebhookUnknownAuthKind {
                webhook: id.clone(),
                auth_kind: other.to_string(),
            })
        }
    };

    Ok(Webhook {
        id,
        mount,
        path,
        deliver_to,
        auth,
        max_body_bytes,
    })
}

fn parse_workflow(b: &wcl_lang::BlockRef) -> Result<Workflow, ConfigError> {
    let id = require_id(&b.id, &b.kind)?;
    let description = require_string(b, "description")?;
    let max_retries = optional_u32(b, "max_retries")?.unwrap_or(5);

    let mut properties: Vec<WorkflowProperty> = Vec::new();
    let mut steps: Vec<WorkflowStep> = Vec::new();
    for child in &b.children {
        match child.kind.as_str() {
            "property" => properties.push(parse_property(child)?),
            "step" => steps.push(parse_step(child)?),
            other => {
                return Err(ConfigError::UnknownWorkflowChildBlock {
                    workflow: id.clone(),
                    child: other.to_string(),
                })
            }
        }
    }
    Ok(Workflow {
        id,
        description,
        max_retries,
        properties,
        steps,
    })
}

fn parse_property(b: &wcl_lang::BlockRef) -> Result<WorkflowProperty, ConfigError> {
    let id = require_id(&b.id, &b.kind)?;
    let label = optional_string(b, "label");
    let required = optional_bool(b, "required")?.unwrap_or(false);
    let kind_name = require_symbol(b, "kind")?;
    let kind = match kind_name.as_str() {
        "text" => PropertyKind::Text {
            default: optional_string(b, "default"),
        },
        "number" => PropertyKind::Number {
            default: optional_f64(b, "default")?,
            min: optional_f64(b, "min")?,
            max: optional_f64(b, "max")?,
        },
        "select" => PropertyKind::Select {
            options: require_string_list(b, "options")?,
            default: optional_string(b, "default"),
        },
        "multiSelect" => PropertyKind::MultiSelect {
            options: require_string_list(b, "options")?,
            default: optional_string_list(b, "default")?,
        },
        "hidden" => PropertyKind::Hidden {
            default: optional_string(b, "default"),
        },
        other => {
            return Err(ConfigError::UnknownPropertyKind {
                property: id,
                kind: other.to_string(),
            })
        }
    };
    Ok(WorkflowProperty {
        id,
        label,
        required,
        kind,
    })
}

fn parse_step(b: &wcl_lang::BlockRef) -> Result<WorkflowStep, ConfigError> {
    let id = require_id(&b.id, &b.kind)?;
    let kind_name = require_symbol(b, "kind")?;
    let kind = match kind_name.as_str() {
        "worker" => StepKind::Worker {
            worker: require_string(b, "worker")?,
            prompt: require_string(b, "prompt")?,
        },
        "command" => StepKind::Command {
            command: require_string(b, "command")?,
            cwd: optional_string(b, "cwd"),
        },
        "conditional" => StepKind::Conditional {
            worker: require_string(b, "worker")?,
            prompt: require_string(b, "prompt")?,
            target: require_string(b, "target")?,
        },
        "check" => {
            let op_name = require_symbol(b, "op")?;
            let op = match op_name.as_str() {
                "equals" => CheckOp::Equals,
                "notEquals" => CheckOp::NotEquals,
                "contains" => CheckOp::Contains,
                "empty" => CheckOp::Empty,
                "notEmpty" => CheckOp::NotEmpty,
                other => {
                    return Err(ConfigError::UnknownCheckOp {
                        step: id,
                        op: other.to_string(),
                    })
                }
            };
            StepKind::Check {
                variable: require_string(b, "variable")?,
                op,
                value: optional_string(b, "value"),
                target: require_string(b, "target")?,
            }
        }
        other => {
            return Err(ConfigError::UnknownStepKind {
                step: id,
                kind: other.to_string(),
            })
        }
    };
    Ok(WorkflowStep { id, kind })
}

/// Guard against a configured entity colliding with the reserved
/// `user` id. The user's inbox routes on recipient id `"user"`, so any
/// agent / automaton / worker / channel / webhook with that id would
/// shadow it.
fn validate_reserved_ids(
    agents: &[Agent],
    automatons: &[Automaton],
    workers: &[Worker],
    channels: &[Channel],
    webhooks: &[Webhook],
) -> Result<(), ConfigError> {
    let check = |kind: &str, id: &str| -> Result<(), ConfigError> {
        if id == USER_INBOX_ID {
            Err(ConfigError::ReservedId {
                kind: kind.to_string(),
                id: id.to_string(),
            })
        } else {
            Ok(())
        }
    };
    for a in agents {
        check("agent", &a.id)?;
    }
    for a in automatons {
        check("automaton", &a.id)?;
    }
    for w in workers {
        check("worker", &w.id)?;
    }
    for c in channels {
        check("channel", &c.id)?;
    }
    for w in webhooks {
        check("webhook", &w.id)?;
    }
    Ok(())
}

/// Every id mentioned in a `can_message` list must correspond to a real
/// agent or automaton declared in the same config; otherwise the
/// allowlist silently lets nothing through and the typo is hard to
/// diagnose. The reserved `user` id is always accepted.
fn validate_agent_refs(agents: &[Agent], automatons: &[Automaton]) -> Result<(), ConfigError> {
    let mut known: std::collections::HashSet<&str> = agents
        .iter()
        .map(|a| a.id.as_str())
        .chain(automatons.iter().map(|a| a.id.as_str()))
        .collect();
    known.insert(USER_INBOX_ID);
    for a in agents {
        if let Some(list) = &a.can_message {
            for id in list {
                if !known.contains(id.as_str()) {
                    return Err(ConfigError::UnknownAgentRef {
                        from: a.id.clone(),
                        attr: "can_message".to_string(),
                        to: id.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Reserved HTTP paths the webservice hosts internally. Webhooks
/// may not collide with these.
const RESERVED_WEBHOOK_PATHS: &[&str] = &["/v1/rpc", "/v1/events", "/v1/health"];

fn validate_webhooks(
    agents: &[Agent],
    automatons: &[Automaton],
    webhooks: &[Webhook],
) -> Result<(), ConfigError> {
    let addressable: std::collections::HashSet<&str> = agents
        .iter()
        .map(|a| a.id.as_str())
        .chain(automatons.iter().map(|a| a.id.as_str()))
        .collect();

    let mut mount_paths: std::collections::HashMap<&str, std::collections::HashSet<&str>> =
        std::collections::HashMap::new();

    for w in webhooks {
        let mount_auto = automatons.iter().find(|a| a.id == w.mount).ok_or_else(|| {
            ConfigError::WebhookUnknownMount {
                webhook: w.id.clone(),
                mount: w.mount.clone(),
            }
        })?;
        if !matches!(mount_auto.backend, AutomatonBackend::Webservice { .. }) {
            return Err(ConfigError::WebhookMountNotWebservice {
                webhook: w.id.clone(),
                mount: w.mount.clone(),
            });
        }
        if !addressable.contains(w.deliver_to.as_str()) {
            return Err(ConfigError::WebhookUnknownDeliverTarget {
                webhook: w.id.clone(),
                target: w.deliver_to.clone(),
            });
        }
        if let Some(list) = &mount_auto.can_message {
            if !list.iter().any(|id| id == &w.deliver_to) {
                return Err(ConfigError::WebhookDeliverNotReachable {
                    webhook: w.id.clone(),
                    target: w.deliver_to.clone(),
                    mount: w.mount.clone(),
                });
            }
        }
        if RESERVED_WEBHOOK_PATHS.contains(&w.path.as_str()) {
            return Err(ConfigError::WebhookReservedPath {
                webhook: w.id.clone(),
                path: w.path.clone(),
            });
        }
        let paths = mount_paths.entry(w.mount.as_str()).or_default();
        if !paths.insert(w.path.as_str()) {
            return Err(ConfigError::WebhookPathConflict {
                webhook: w.id.clone(),
                mount: w.mount.clone(),
                path: w.path.clone(),
            });
        }
    }
    Ok(())
}

/// Mirror of [`validate_agent_refs`] for automaton `can_message`
/// allowlists. An automaton may target either a declared agent,
/// another declared automaton, or the reserved `user` id. Also
/// validates that `:remote`-backed automatons' `deliver_to` references
/// a declared agent/automaton.
fn validate_automaton_refs(agents: &[Agent], automatons: &[Automaton]) -> Result<(), ConfigError> {
    let mut known: std::collections::HashSet<&str> = agents
        .iter()
        .map(|a| a.id.as_str())
        .chain(automatons.iter().map(|a| a.id.as_str()))
        .collect();
    known.insert(USER_INBOX_ID);
    for a in automatons {
        if let Some(list) = &a.can_message {
            for id in list {
                if !known.contains(id.as_str()) {
                    return Err(ConfigError::UnknownAgentRef {
                        from: a.id.clone(),
                        attr: "can_message".to_string(),
                        to: id.clone(),
                    });
                }
            }
        }
        if let AutomatonBackend::Remote { deliver_to, .. } = &a.backend {
            if !known.contains(deliver_to.as_str()) {
                return Err(ConfigError::RemoteUnknownDeliverTo {
                    automaton: a.id.clone(),
                    target: deliver_to.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Every `ai_core` id referenced by the team default or an
/// agent/worker override must resolve to a declared `ai_core` block.
fn validate_ai_core_refs(
    team: &Team,
    agents: &[Agent],
    workers: &[Worker],
    cores: &[AiCore],
) -> Result<(), ConfigError> {
    let known: std::collections::HashSet<&str> = cores.iter().map(|c| c.id.as_str()).collect();
    let check = |from: &str, attr: &str, id: &str| -> Result<(), ConfigError> {
        if known.contains(id) {
            Ok(())
        } else {
            Err(ConfigError::UnknownAiCoreRef {
                from: from.to_string(),
                attr: attr.to_string(),
                to: id.to_string(),
            })
        }
    };
    if let Some(id) = &team.default_ai_core {
        check(&team.id, "default_ai_core", id)?;
    }
    for a in agents {
        if let Some(id) = &a.ai_core {
            check(&a.id, "ai_core", id)?;
        }
    }
    for w in workers {
        if let Some(id) = &w.ai_core {
            check(&w.id, "ai_core", id)?;
        }
    }
    Ok(())
}

/// Validate an OAuth credential name: non-empty, ≤64 chars, and
/// consists of `[a-z0-9_-]+`. Used to check the `credential` field
/// on `openAiChatgpt` cores against the on-disk `auth.json` keys.
/// Conservative so names survive shell, WCL, and JSON round-trips.
fn is_valid_credential_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Ensure every tool name declared in an `AnthropicApi` core's
/// `default_tools` resolves to a known built-in. `ClaudeCode` cores
/// skip this check because tool names there are decided by the
/// external `claude` binary.
fn validate_ai_core_tools(cores: &[AiCore]) -> Result<(), ConfigError> {
    let api_known: std::collections::HashSet<&str> = crate::ai::tools::BUILT_IN_TOOL_NAMES
        .iter()
        .copied()
        .collect();
    let copilot_known: std::collections::HashSet<&str> =
        crate::ai::copilot_cli::COPILOT_ALLOWED_TOOLS
            .iter()
            .copied()
            .collect();
    let gemini_known: std::collections::HashSet<&str> = crate::ai::gemini_cli::GEMINI_ALLOWED_TOOLS
        .iter()
        .copied()
        .collect();
    for core in cores {
        let (type_name, defaults, allowlist) = match &core.kind {
            AiCoreKind::AnthropicApi { default_tools, .. } => {
                ("anthropicApi", default_tools.as_deref(), &api_known)
            }
            AiCoreKind::OpenAiApi { default_tools, .. } => {
                ("openAiApi", default_tools.as_deref(), &api_known)
            }
            AiCoreKind::OpenAiChatgpt { default_tools, .. } => {
                ("openAiChatgpt", default_tools.as_deref(), &api_known)
            }
            AiCoreKind::CopilotCli { default_tools, .. } => {
                ("copilotCli", default_tools.as_deref(), &copilot_known)
            }
            AiCoreKind::GeminiCli { default_tools, .. } => {
                ("geminiCli", default_tools.as_deref(), &gemini_known)
            }
            AiCoreKind::ClaudeCode { .. } | AiCoreKind::Jules { .. } => continue,
        };
        let Some(list) = defaults else { continue };
        for name in list {
            if !allowlist.contains(name.as_str()) {
                return Err(ConfigError::UnknownTool {
                    core: core.id.clone(),
                    type_name: type_name.to_string(),
                    tool: name.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Every id in an agent's or worker's `memory_banks` list must match a
/// declared `memory_bank` block.
fn validate_memory_bank_refs(
    agents: &[Agent],
    workers: &[Worker],
    banks: &[MemoryBank],
) -> Result<(), ConfigError> {
    let known: std::collections::HashSet<&str> = banks.iter().map(|b| b.id.as_str()).collect();
    let check = |from: &str, list: &[String]| -> Result<(), ConfigError> {
        for id in list {
            if !known.contains(id.as_str()) {
                return Err(ConfigError::UnknownMemoryBankRef {
                    from: from.to_string(),
                    to: id.clone(),
                });
            }
        }
        Ok(())
    };
    for a in agents {
        if let Some(list) = &a.memory_banks {
            check(&a.id, list)?;
        }
    }
    for w in workers {
        if let Some(list) = &w.memory_banks {
            check(&w.id, list)?;
        }
    }
    Ok(())
}

/// Every id in an agent's or worker's `skills` list must be a safe
/// filesystem identifier (`is_valid_skill_name`). We don't require the
/// skill folder to exist at config-load time — missing folders are
/// rendered as a placeholder entry in the prompt so agents still boot
/// when a skill is typo'd or not yet authored.
fn validate_skill_names(agents: &[Agent], workers: &[Worker]) -> Result<(), ConfigError> {
    let check = |from: &str, list: &[String]| -> Result<(), ConfigError> {
        for name in list {
            if !crate::skills::is_valid_skill_name(name) {
                return Err(ConfigError::InvalidSkillName {
                    from: from.to_string(),
                    name: name.clone(),
                });
            }
        }
        Ok(())
    };
    for a in agents {
        if let Some(list) = &a.skills {
            check(&a.id, list)?;
        }
    }
    for w in workers {
        if let Some(list) = &w.skills {
            check(&w.id, list)?;
        }
    }
    Ok(())
}

/// Every `compaction` reference from an agent or worker must resolve
/// to a declared `compaction` block, and `strategy = :harness` is only
/// valid when the actor's resolved `ai_core` is a CLI-harness variant
/// (ClaudeCode / CopilotCli / GeminiCli). Direct API cores have no
/// slash-command concept, so we reject at parse time.
fn validate_compaction_refs(
    team: &Team,
    agents: &[Agent],
    workers: &[Worker],
    cores: &[AiCore],
    compactions: &[Compaction],
) -> Result<(), ConfigError> {
    let known: std::collections::HashMap<&str, &Compaction> =
        compactions.iter().map(|c| (c.id.as_str(), c)).collect();
    let resolve_core = |override_id: Option<&str>| -> Option<&AiCore> {
        let id = override_id.or(team.default_ai_core.as_deref())?;
        cores.iter().find(|c| c.id == id)
    };
    let check = |actor: &str,
                 compaction_id: &str,
                 core_override: Option<&str>|
     -> Result<(), ConfigError> {
        let Some(compaction) = known.get(compaction_id) else {
            return Err(ConfigError::UnknownCompactionRef {
                from: actor.to_string(),
                to: compaction_id.to_string(),
            });
        };
        if matches!(compaction.strategy, CompactionStrategy::Harness) {
            // Only claude-code's CLI actually honours `/compact` when
            // invoked via `-p` / non-interactive mode. Gemini and
            // Copilot treat the literal string as a user prompt, so
            // the model just responds "you invoked /compact" without
            // any real compaction — better to reject at parse time.
            let core_type = match resolve_core(core_override).map(|c| &c.kind) {
                Some(AiCoreKind::ClaudeCode { .. }) => return Ok(()),
                Some(AiCoreKind::CopilotCli { .. }) => "copilotCli",
                Some(AiCoreKind::GeminiCli { .. }) => "geminiCli",
                Some(AiCoreKind::AnthropicApi { .. }) => "anthropicApi",
                Some(AiCoreKind::OpenAiApi { .. }) => "openAiApi",
                Some(AiCoreKind::OpenAiChatgpt { .. }) => "openAiChatgpt",
                Some(AiCoreKind::Jules { .. }) => "jules",
                // No core resolvable — treat as "unknown", let the
                // core-ref validator surface a better error. Compaction
                // validation stays conservative here.
                None => return Ok(()),
            };
            return Err(ConfigError::CompactionHarnessUnsupportedCore {
                id: compaction.id.clone(),
                actor: actor.to_string(),
                core_type: core_type.to_string(),
            });
        }
        // Summarise may reference an ai_core for the summary pass; if
        // so, it must resolve too.
        if let CompactionStrategy::Summarise {
            ai_core: Some(core_id),
            ..
        } = &compaction.strategy
        {
            if !cores.iter().any(|c| c.id == *core_id) {
                return Err(ConfigError::UnknownAiCoreRef {
                    from: compaction.id.clone(),
                    attr: "ai_core".to_string(),
                    to: core_id.clone(),
                });
            }
        }
        Ok(())
    };
    for a in agents {
        if let Some(id) = &a.compaction {
            check(&a.id, id, a.ai_core.as_deref())?;
        }
    }
    for w in workers {
        if let Some(id) = &w.compaction {
            check(&w.id, id, w.ai_core.as_deref())?;
        }
    }
    Ok(())
}

/// Channel membership must reference declared `agent` or `automaton`
/// ids (workers are ephemeral and cannot join), and each member may
/// appear at most once per channel. Channel ids themselves must be
/// unique across the config.
fn validate_channel_refs(
    agents: &[Agent],
    automatons: &[Automaton],
    channels: &[Channel],
) -> Result<(), ConfigError> {
    let known: std::collections::HashSet<&str> = agents
        .iter()
        .map(|a| a.id.as_str())
        .chain(automatons.iter().map(|a| a.id.as_str()))
        .collect();
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for c in channels {
        if !seen_ids.insert(c.id.as_str()) {
            return Err(ConfigError::DuplicateChannelId { id: c.id.clone() });
        }
        let mut seen_members: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for m in &c.members {
            if !known.contains(m.as_str()) {
                return Err(ConfigError::UnknownChannelMember {
                    channel: c.id.clone(),
                    member: m.clone(),
                });
            }
            if !seen_members.insert(m.as_str()) {
                return Err(ConfigError::DuplicateChannelMember {
                    channel: c.id.clone(),
                    member: m.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Structural validation for declared workflows: unique ids within each
/// workflow, resolvable worker refs and step targets, well-formed
/// property defaults, and coherent `check` op/value combinations.
fn validate_workflow_refs(workflows: &[Workflow], workers: &[Worker]) -> Result<(), ConfigError> {
    let known_workers: std::collections::HashSet<&str> =
        workers.iter().map(|w| w.id.as_str()).collect();
    for wf in workflows {
        // Property ids unique; defaults make sense.
        let mut seen_props: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for p in &wf.properties {
            if !seen_props.insert(p.id.as_str()) {
                return Err(ConfigError::DuplicatePropertyId {
                    workflow: wf.id.clone(),
                    property: p.id.clone(),
                });
            }
            match &p.kind {
                PropertyKind::Number {
                    min: Some(min),
                    max: Some(max),
                    ..
                } if min > max => {
                    return Err(ConfigError::InvalidPropertyRange {
                        workflow: wf.id.clone(),
                        property: p.id.clone(),
                        min: *min,
                        max: *max,
                    })
                }
                PropertyKind::Select {
                    options,
                    default: Some(d),
                } if !options.iter().any(|o| o == d) => {
                    return Err(ConfigError::InvalidPropertyDefault {
                        workflow: wf.id.clone(),
                        property: p.id.clone(),
                        default: d.clone(),
                    })
                }
                PropertyKind::MultiSelect {
                    options,
                    default: Some(ds),
                } => {
                    for d in ds {
                        if !options.iter().any(|o| o == d) {
                            return Err(ConfigError::InvalidPropertyDefault {
                                workflow: wf.id.clone(),
                                property: p.id.clone(),
                                default: d.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        // Step ids unique.
        let mut seen_steps: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for s in &wf.steps {
            if !seen_steps.insert(s.id.as_str()) {
                return Err(ConfigError::DuplicateStepId {
                    workflow: wf.id.clone(),
                    step: s.id.clone(),
                });
            }
        }

        for s in &wf.steps {
            match &s.kind {
                StepKind::Worker { worker, .. } | StepKind::Conditional { worker, .. } => {
                    if !known_workers.contains(worker.as_str()) {
                        return Err(ConfigError::UnknownWorkflowWorkerRef {
                            workflow: wf.id.clone(),
                            step: s.id.clone(),
                            worker: worker.clone(),
                        });
                    }
                }
                StepKind::Command { .. } | StepKind::Check { .. } => {}
            }
            if let StepKind::Conditional { target, .. } = &s.kind {
                if !seen_steps.contains(target.as_str()) {
                    return Err(ConfigError::UnknownWorkflowStepTarget {
                        workflow: wf.id.clone(),
                        step: s.id.clone(),
                        target: target.clone(),
                    });
                }
            }
            if let StepKind::Check {
                variable,
                op,
                value,
                target,
            } = &s.kind
            {
                if !seen_props.contains(variable.as_str()) {
                    return Err(ConfigError::UnknownWorkflowVariable {
                        workflow: wf.id.clone(),
                        step: s.id.clone(),
                        variable: variable.clone(),
                    });
                }
                if !seen_steps.contains(target.as_str()) {
                    return Err(ConfigError::UnknownWorkflowStepTarget {
                        workflow: wf.id.clone(),
                        step: s.id.clone(),
                        target: target.clone(),
                    });
                }
                match (op.needs_value(), value) {
                    (true, None) => {
                        return Err(ConfigError::CheckMissingValue {
                            workflow: wf.id.clone(),
                            step: s.id.clone(),
                            op: op.name().to_string(),
                        })
                    }
                    (false, Some(_)) => {
                        return Err(ConfigError::CheckUnexpectedValue {
                            workflow: wf.id.clone(),
                            step: s.id.clone(),
                            op: op.name().to_string(),
                        })
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Every id in an agent's or worker's `can_run_workflows` list must
/// match a declared workflow.
fn validate_workflow_allowlists(
    agents: &[Agent],
    workers: &[Worker],
    workflows: &[Workflow],
) -> Result<(), ConfigError> {
    let known: std::collections::HashSet<&str> = workflows.iter().map(|w| w.id.as_str()).collect();
    let check = |from: &str, list: &[String]| -> Result<(), ConfigError> {
        for id in list {
            if !known.contains(id.as_str()) {
                return Err(ConfigError::UnknownWorkflowRef {
                    from: from.to_string(),
                    to: id.clone(),
                });
            }
        }
        Ok(())
    };
    for a in agents {
        if let Some(list) = &a.can_run_workflows {
            check(&a.id, list)?;
        }
    }
    for w in workers {
        if let Some(list) = &w.can_run_workflows {
            check(&w.id, list)?;
        }
    }
    Ok(())
}

/// Every id in an agent's or worker's `can_create_workers` list must
/// match a declared worker template.
fn validate_worker_refs(agents: &[Agent], workers: &[Worker]) -> Result<(), ConfigError> {
    let known: std::collections::HashSet<&str> = workers.iter().map(|w| w.id.as_str()).collect();
    let check = |from: &str, list: &[String]| -> Result<(), ConfigError> {
        for id in list {
            if !known.contains(id.as_str()) {
                return Err(ConfigError::UnknownWorkerRef {
                    from: from.to_string(),
                    to: id.clone(),
                });
            }
        }
        Ok(())
    };
    for a in agents {
        if let Some(list) = &a.can_create_workers {
            check(&a.id, list)?;
        }
    }
    for w in workers {
        if let Some(list) = &w.can_create_workers {
            check(&w.id, list)?;
        }
    }
    Ok(())
}

impl AciddogConfig {
    /// Returns true if fleet member `from` (agent or automaton) is
    /// permitted to send a message to `to`. Unknown `from` id → false
    /// (conservative default).
    pub fn can_send(&self, from: &str, to: &str) -> bool {
        // `user` is the reserved recipient id for the human operator's
        // inbox. Every agent/automaton can always reach the user,
        // regardless of `can_message` — the operator is not a
        // configured teammate and their presence is not gated by
        // teammate ACLs.
        if to == USER_INBOX_ID {
            return self.agents.iter().any(|a| a.id == from)
                || self.automatons.iter().any(|a| a.id == from);
        }
        let allowlist = if let Some(a) = self.agents.iter().find(|a| a.id == from) {
            a.can_message.as_ref()
        } else if let Some(a) = self.automatons.iter().find(|a| a.id == from) {
            a.can_message.as_ref()
        } else {
            return false;
        };
        match allowlist {
            Some(list) => list.iter().any(|id| id == to),
            None => true,
        }
    }

    /// Every id an agent with unrestricted `can_message` should be
    /// able to reach — all declared agents plus all automatons.
    /// Used to build the default "Agents you can message" list when
    /// `can_message` is omitted.
    pub fn all_addressable_ids(&self) -> Vec<String> {
        self.agents
            .iter()
            .map(|a| a.id.clone())
            .chain(self.automatons.iter().map(|a| a.id.clone()))
            .collect()
    }

    /// `true` iff `id` is a declared agent or automaton — i.e. a
    /// persistent fleet member that can be mailed, broadcast to in a
    /// channel, or added to one at runtime. Worker templates and
    /// ephemeral worker instances deliberately return `false`.
    pub fn is_addressable(&self, id: &str) -> bool {
        self.agents.iter().any(|a| a.id == id) || self.automatons.iter().any(|a| a.id == id)
    }

    /// Returns true if the actor identified by `actor_template_id` is
    /// permitted to spawn a worker of template id `worker_id`.
    ///
    /// `actor_template_id` is an agent config id for configured agents,
    /// or a worker **template** id for workers spawned from that
    /// template. Worker create capability is a template-level property:
    /// every instance of a template has identical rights.
    ///
    /// Unknown actor ids and templates with no `can_create_workers`
    /// list default to denied.
    pub fn can_create(&self, actor_template_id: &str, worker_id: &str) -> bool {
        if let Some(a) = self.agents.iter().find(|a| a.id == actor_template_id) {
            return match &a.can_create_workers {
                Some(list) => list.iter().any(|id| id == worker_id),
                None => false,
            };
        }
        if let Some(w) = self.workers.iter().find(|w| w.id == actor_template_id) {
            return match &w.can_create_workers {
                Some(list) => list.iter().any(|id| id == worker_id),
                None => false,
            };
        }
        false
    }

    pub fn worker(&self, id: &str) -> Option<&Worker> {
        self.workers.iter().find(|w| w.id == id)
    }

    /// Look up a declared memory bank by id.
    pub fn memory_bank(&self, id: &str) -> Option<&MemoryBank> {
        self.memory_banks.iter().find(|b| b.id == id)
    }

    /// Return the memory banks accessible to `actor_template_id`.
    /// `actor_template_id` is an agent config id for configured agents,
    /// or a worker template id for workers spawned from that template.
    /// Unknown ids and actors with no declared `memory_banks` list
    /// return an empty vec.
    pub fn memory_banks_for(&self, actor_template_id: &str) -> Vec<&MemoryBank> {
        let list = if let Some(a) = self.agents.iter().find(|a| a.id == actor_template_id) {
            a.memory_banks.as_deref()
        } else if let Some(w) = self.workers.iter().find(|w| w.id == actor_template_id) {
            w.memory_banks.as_deref()
        } else {
            return Vec::new();
        };
        let Some(list) = list else {
            return Vec::new();
        };
        list.iter().filter_map(|id| self.memory_bank(id)).collect()
    }

    /// Return the skill ids pre-announced for `actor_template_id`.
    /// `actor_template_id` is an agent config id for configured agents,
    /// or a worker template id for workers spawned from that template.
    /// Unknown ids and actors with no declared `skills` list return an
    /// empty vec.
    pub fn skills_for(&self, actor_template_id: &str) -> Vec<String> {
        let list = if let Some(a) = self.agents.iter().find(|a| a.id == actor_template_id) {
            a.skills.as_deref()
        } else if let Some(w) = self.workers.iter().find(|w| w.id == actor_template_id) {
            w.skills.as_deref()
        } else {
            return Vec::new();
        };
        list.map(|l| l.to_vec()).unwrap_or_default()
    }

    /// True iff `actor_template_id` has `bank_id` in its `memory_banks`
    /// allowlist. Unknown actor ids default to `false`.
    pub fn can_access_bank(&self, actor_template_id: &str, bank_id: &str) -> bool {
        let list = if let Some(a) = self.agents.iter().find(|a| a.id == actor_template_id) {
            a.memory_banks.as_deref()
        } else if let Some(w) = self.workers.iter().find(|w| w.id == actor_template_id) {
            w.memory_banks.as_deref()
        } else {
            return false;
        };
        matches!(list, Some(l) if l.iter().any(|id| id == bank_id))
    }

    /// Resolve the tool allowlist for an agent or worker. Precedence:
    /// per-agent/worker list → resolved ai_core's `default_tools` →
    /// built-in `DEFAULT_ALLOWED_TOOLS`. Tool names are core-specific,
    /// so the per-core defaults live on the `ai_core` block itself.
    pub fn effective_tools(
        &self,
        override_tools: Option<&[String]>,
        core: Option<&AiCore>,
    ) -> Vec<String> {
        if let Some(list) = override_tools {
            return list.to_vec();
        }
        if let Some(list) = core.and_then(AiCore::default_tools) {
            return list.to_vec();
        }
        DEFAULT_ALLOWED_TOOLS
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Resolve the AI core config for an agent or worker. Precedence:
    /// per-agent/worker override id → `team.default_ai_core`. Returns
    /// `None` if neither is set or the referenced id does not exist
    /// (ref validation in `parse_config` catches the latter at load
    /// time, so `None` in practice means "nothing configured").
    pub fn effective_ai_core(&self, override_id: Option<&str>) -> Option<&AiCore> {
        let id = override_id.or(self.team.default_ai_core.as_deref())?;
        self.ai_cores.iter().find(|c| c.id == id)
    }

    /// Resolve the model id used at inference time for an agent or
    /// worker. Precedence: the actor's own `model` (if set) → the
    /// resolved `ai_core`'s `runtime_model()`. Returns `None` when
    /// neither is specified — acceptable for CLI-harness cores
    /// (`claudeCode`, `copilotCli`, `geminiCli`) that let the child
    /// binary pick its own default, and unreachable for direct-API
    /// cores because their `model` field is required at parse time.
    pub fn effective_model<'a>(
        &self,
        actor_model: Option<&'a str>,
        core: Option<&'a AiCore>,
    ) -> Option<&'a str> {
        actor_model.or_else(|| core.and_then(|c| c.runtime_model()))
    }

    /// Look up a declared compaction policy by id.
    pub fn compaction(&self, id: &str) -> Option<&Compaction> {
        self.compactions.iter().find(|c| c.id == id)
    }

    /// Look up a declared channel by id.
    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// Return every channel `agent_id` is a member of, in declaration
    /// order. Unknown agent ids yield an empty vec.
    pub fn channels_for_agent(&self, agent_id: &str) -> Vec<&Channel> {
        self.channels
            .iter()
            .filter(|c| c.members.iter().any(|m| m == agent_id))
            .collect()
    }

    /// True iff `agent_id` is a declared member of channel `channel_id`.
    /// Unknown ids on either side return `false`.
    pub fn is_channel_member(&self, channel_id: &str, agent_id: &str) -> bool {
        self.channel(channel_id)
            .map(|c| c.members.iter().any(|m| m == agent_id))
            .unwrap_or(false)
    }

    /// Look up a declared workflow by id.
    pub fn workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|w| w.id == id)
    }

    /// Returns true if the actor identified by `actor_template_id` is
    /// permitted to trigger the workflow `workflow_id`. Unknown actor
    /// ids and actors without a `can_run_workflows` list default to
    /// denied. Same semantics as `can_create`.
    pub fn can_run_workflow(&self, actor_template_id: &str, workflow_id: &str) -> bool {
        if let Some(a) = self.agents.iter().find(|a| a.id == actor_template_id) {
            return match &a.can_run_workflows {
                Some(list) => list.iter().any(|id| id == workflow_id),
                None => false,
            };
        }
        if let Some(w) = self.workers.iter().find(|w| w.id == actor_template_id) {
            return match &w.can_run_workflows {
                Some(list) => list.iter().any(|id| id == workflow_id),
                None => false,
            };
        }
        false
    }

    /// Return the compaction policy attached to `actor_template_id`
    /// (an agent config id, or a worker template id), if any. Returns
    /// `None` when the actor is unknown, has no `compaction` attribute
    /// declared, or the referenced policy was removed — callers should
    /// fall back to the built-in `Drop` strategy.
    pub fn compaction_for(&self, actor_template_id: &str) -> Option<&Compaction> {
        let id = if let Some(a) = self.agents.iter().find(|a| a.id == actor_template_id) {
            a.compaction.as_deref()?
        } else if let Some(w) = self.workers.iter().find(|w| w.id == actor_template_id) {
            w.compaction.as_deref()?
        } else {
            return None;
        };
        self.compaction(id)
    }
}

fn require_id(id: &Option<String>, kind: &str) -> Result<String, ConfigError> {
    id.clone().ok_or_else(|| ConfigError::MissingId {
        kind: kind.to_string(),
    })
}

fn require_string(block: &wcl_lang::BlockRef, attr: &str) -> Result<String, ConfigError> {
    match block.get(attr) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Identifier(s)) => Ok(s.clone()),
        Some(_) => Err(ConfigError::BadAttributeType {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
        None => Err(ConfigError::MissingAttribute {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

fn optional_string(block: &wcl_lang::BlockRef, attr: &str) -> Option<String> {
    match block.get(attr) {
        Some(Value::String(s)) | Some(Value::Identifier(s)) => Some(s.clone()),
        _ => None,
    }
}

fn require_symbol(block: &wcl_lang::BlockRef, attr: &str) -> Result<String, ConfigError> {
    match block.get(attr) {
        Some(Value::Symbol(s)) => Ok(s.clone()),
        Some(_) => Err(ConfigError::BadAttributeType {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
        None => Err(ConfigError::MissingAttribute {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

fn optional_symbol(block: &wcl_lang::BlockRef, attr: &str) -> Result<Option<String>, ConfigError> {
    match block.get(attr) {
        None => Ok(None),
        Some(Value::Symbol(s)) => Ok(Some(s.clone())),
        Some(_) => Err(ConfigError::BadAttributeType {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

fn optional_bool(block: &wcl_lang::BlockRef, attr: &str) -> Result<Option<bool>, ConfigError> {
    match block.get(attr) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ConfigError::BadAttributeType {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

/// Read an optional integer attribute. Accepts `Value::Int` or the
/// large-int variant `Value::BigInt` when it fits into `u32`.
fn optional_u32(block: &wcl_lang::BlockRef, attr: &str) -> Result<Option<u32>, ConfigError> {
    match block.get(attr) {
        None => Ok(None),
        Some(Value::Int(i)) if *i >= 0 && *i <= u32::MAX as i64 => Ok(Some(*i as u32)),
        Some(Value::BigInt(i)) if *i >= 0 && *i <= u32::MAX as i128 => Ok(Some(*i as u32)),
        Some(_) => Err(ConfigError::BadAttributeType {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

/// Read an optional number (f64) attribute. Accepts `Value::Float`,
/// `Value::Int`, and `Value::BigInt`.
fn optional_f64(block: &wcl_lang::BlockRef, attr: &str) -> Result<Option<f64>, ConfigError> {
    match block.get(attr) {
        None => Ok(None),
        Some(Value::Float(f)) => Ok(Some(*f)),
        Some(Value::Int(i)) => Ok(Some(*i as f64)),
        Some(Value::BigInt(i)) => Ok(Some(*i as f64)),
        Some(_) => Err(ConfigError::BadAttributeType {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

/// Required list-of-strings attribute: errors if missing OR wrong type.
fn require_string_list(block: &wcl_lang::BlockRef, attr: &str) -> Result<Vec<String>, ConfigError> {
    optional_string_list(block, attr)?.ok_or_else(|| ConfigError::MissingAttribute {
        kind: block.kind.clone(),
        attr: attr.to_string(),
    })
}

/// Read an optional list-of-strings attribute. Returns `Ok(None)` if the
/// key is absent, `Ok(Some(vec))` if present (possibly empty), or an
/// error for the wrong outer type or non-string elements.
fn optional_string_list(
    block: &wcl_lang::BlockRef,
    attr: &str,
) -> Result<Option<Vec<String>>, ConfigError> {
    match block.get(attr) {
        None => Ok(None),
        Some(Value::List(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for v in items {
                match v {
                    Value::String(s) | Value::Identifier(s) => out.push(s.clone()),
                    _ => {
                        return Err(ConfigError::BadListElementType {
                            kind: block.kind.clone(),
                            attr: attr.to_string(),
                        })
                    }
                }
            }
            Ok(Some(out))
        }
        Some(_) => Err(ConfigError::NotAList {
            kind: block.kind.clone(),
            attr: attr.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dummy_path() -> PathBuf {
        PathBuf::from("<test>")
    }

    const MINIMAL: &str = r#"
        team default {
            description = "minimal team"
        }

        agent alpha {
            role = "planner"
            instructions = "plan things"
        }
    "#;

    #[test]
    fn parses_minimal_config() {
        let cfg = parse_config(MINIMAL, &dummy_path()).expect("parse ok");
        assert_eq!(cfg.team.id, "default");
        assert_eq!(cfg.team.description, "minimal team");
        assert_eq!(cfg.agents.len(), 1);
        assert_eq!(cfg.agents[0].id, "alpha");
        assert_eq!(cfg.agents[0].role, "planner");
        assert_eq!(cfg.agents[0].instructions, "plan things");
    }

    #[test]
    fn agent_model_is_none_when_omitted() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(cfg.agents[0].model.is_none());
    }

    #[test]
    fn keeps_explicit_model() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "x"
                model = "claude-sonnet-4-6"
                instructions = "i"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.agents[0].model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn errors_when_team_missing() {
        let src = r#"
            agent alpha { role = "x" instructions = "y" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(err, ConfigError::MissingTeam));
    }

    #[test]
    fn on_reload_defaults_to_ignore() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert_eq!(cfg.agents[0].on_reload, ReloadAction::Ignore);
    }

    #[test]
    fn agent_leader_defaults_to_false() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(!cfg.agents[0].leader);
    }

    #[test]
    fn agent_leader_parses_true() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "a"
                instructions = "b"
                leader = true
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.agents[0].leader);
    }

    #[test]
    fn sysadmin_does_not_imply_leader() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "a"
                instructions = "b"
                sysadmin = true
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.agents[0].sysadmin);
        assert!(cfg.agents[0].supervisor); // sysadmin implies supervisor
        assert!(!cfg.agents[0].leader); // but NOT leader
    }

    #[test]
    fn on_reload_parses_each_action() {
        let src = r#"
            team default { description = "t" }
            agent ignore_one { role = "a" instructions = "b" on_reload = :ignore }
            agent compact_one { role = "a" instructions = "b" on_reload = :compact }
            agent reset_one { role = "a" instructions = "b" on_reload = :reset }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.agents[0].on_reload, ReloadAction::Ignore);
        assert_eq!(cfg.agents[1].on_reload, ReloadAction::Compact);
        assert_eq!(cfg.agents[2].on_reload, ReloadAction::Reset);
    }

    #[test]
    fn on_reload_rejects_unknown_value() {
        let src = r#"
            team default { description = "t" }
            agent alpha { role = "a" instructions = "b" on_reload = :bogus }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        match &err {
            ConfigError::InvalidReloadAction { agent, value } => {
                assert_eq!(agent, "alpha");
                assert_eq!(value, "bogus");
            }
            other => panic!("expected InvalidReloadAction, got {other:?}"),
        }
    }

    #[test]
    fn on_reload_rejects_string_value() {
        // Symbols only — strings are a type error to encourage the
        // standard WCL convention.
        let src = r#"
            team default { description = "t" }
            agent alpha { role = "a" instructions = "b" on_reload = "ignore" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        match &err {
            ConfigError::BadAttributeType { attr, .. } => assert_eq!(attr, "on_reload"),
            other => panic!("expected BadAttributeType, got {other:?}"),
        }
    }

    #[test]
    fn errors_when_required_attr_missing() {
        let src = r#"
            team default { description = "t" }
            agent alpha { role = "x" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::MissingAttribute { attr, .. } if attr == "instructions"
        ));
    }

    #[test]
    fn handles_multiple_agents_in_order() {
        let src = r#"
            team default { description = "t" }
            agent one   { role = "a" instructions = "b" }
            agent two   { role = "c" instructions = "d" }
            agent three { role = "e" instructions = "f" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let ids: Vec<&str> = cfg.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["one", "two", "three"]);
    }

    #[test]
    fn config_defaults_can_message_to_none() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(cfg.agents[0].can_message.is_none());
    }

    #[test]
    fn config_parses_can_message_list() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" can_message = ["two", "three"] }
            agent two   { role = "c" instructions = "d" }
            agent three { role = "e" instructions = "f" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let allow = cfg.agents[0].can_message.as_ref().unwrap();
        assert_eq!(allow, &vec!["two".to_string(), "three".to_string()]);
    }

    #[test]
    fn config_parses_empty_can_message_list() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" can_message = [] }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.agents[0].can_message.as_deref(), Some(&[][..]));
    }

    #[test]
    fn config_rejects_can_message_with_unknown_agent_id() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" can_message = ["ghost"] }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownAgentRef { from, to, .. }
                if from == "one" && to == "ghost"
        ));
    }

    #[test]
    fn config_rejects_can_message_with_non_string_element() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" can_message = [42] }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::BadListElementType { attr, .. } if attr == "can_message"
        ));
    }

    #[test]
    fn config_allows_user_in_can_message_list() {
        // `user` is the reserved operator inbox — always a valid
        // `can_message` entry, even though it's not a declared agent.
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" can_message = ["user", "two"] }
            agent two { role = "c" instructions = "d" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.agents[0].can_message.as_deref().unwrap(),
            &["user".to_string(), "two".to_string()]
        );
    }

    #[test]
    fn config_rejects_agent_named_user() {
        let src = r#"
            team default { description = "t" }
            agent user { role = "a" instructions = "b" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(&err, ConfigError::ReservedId { kind, id } if kind == "agent" && id == "user"),
            "expected ReservedId for agent `user`, got {err:?}"
        );
    }

    #[test]
    fn can_send_to_user_is_always_true_for_configured_agents() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent restricted { role = "a" instructions = "b" can_message = [] }
                agent unrestricted { role = "c" instructions = "d" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(
            cfg.can_send("restricted", "user"),
            "agent with empty can_message should still reach user"
        );
        assert!(cfg.can_send("unrestricted", "user"));
    }

    #[test]
    fn can_send_to_user_is_false_for_unknown_sender() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent one { role = "a" instructions = "b" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(!cfg.can_send("ghost", "user"));
    }

    #[test]
    fn can_send_returns_true_when_unrestricted() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent one { role = "a" instructions = "b" }
                agent two { role = "c" instructions = "d" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(cfg.can_send("one", "two"));
        assert!(cfg.can_send("two", "one"));
    }

    #[test]
    fn can_send_respects_allowlist() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent one { role = "a" instructions = "b" can_message = ["two"] }
                agent two { role = "c" instructions = "d" }
                agent three { role = "e" instructions = "f" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(cfg.can_send("one", "two"));
        assert!(!cfg.can_send("one", "three"));
        // Unrestricted agent can still reach everyone.
        assert!(cfg.can_send("two", "one"));
    }

    #[test]
    fn can_send_honors_empty_allowlist() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent one { role = "a" instructions = "b" can_message = [] }
                agent two { role = "c" instructions = "d" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(!cfg.can_send("one", "two"));
    }

    #[test]
    fn can_send_unknown_sender_is_denied() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(!cfg.can_send("ghost", "alpha"));
    }

    #[test]
    fn role_description_defaults_to_none() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(cfg.agents[0].role_description.is_none());
    }

    #[test]
    fn parses_role_description_on_agent_and_worker() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "planner"
                role_description = "long form planner brief"
                instructions = "i"
            }
            worker w {
                role = "doer"
                role_description = "long form worker brief"
                instructions = "i"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.agents[0].role_description.as_deref(),
            Some("long form planner brief")
        );
        assert_eq!(
            cfg.workers[0].role_description.as_deref(),
            Some("long form worker brief")
        );
    }

    #[test]
    fn parses_worker_block() {
        let src = r#"
            team default { description = "t" }
            agent alpha { role = "a" instructions = "b" }
            worker researcher {
                role = "investigator"
                instructions = "Investigate things."
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.workers.len(), 1);
        assert_eq!(cfg.workers[0].id, "researcher");
        assert_eq!(cfg.workers[0].role, "investigator");
        assert!(cfg.workers[0].model.is_none());
        assert!(cfg.workers[0].tools.is_none());
        assert!(cfg.workers[0].can_create_workers.is_none());
    }

    #[test]
    fn parses_worker_tools_and_nested_can_create() {
        let src = r#"
            team default { description = "t" }
            agent alpha { role = "a" instructions = "b" }
            worker researcher {
                role = "r"
                instructions = "i"
                tools = ["Read", "Grep"]
                can_create_workers = ["fetcher"]
            }
            worker fetcher {
                role = "r"
                instructions = "i"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let r = cfg.worker("researcher").unwrap();
        assert_eq!(
            r.tools.as_deref(),
            Some(&["Read".into(), "Grep".into()][..])
        );
        assert_eq!(
            r.can_create_workers.as_deref(),
            Some(&["fetcher".to_string()][..])
        );
    }

    #[test]
    fn parses_agent_can_create_workers() {
        let src = r#"
            team default { description = "t" }
            agent coder {
                role = "i"
                instructions = "i"
                can_create_workers = ["researcher"]
            }
            worker researcher { role = "r" instructions = "i" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.agents[0].can_create_workers.as_deref(),
            Some(&["researcher".to_string()][..])
        );
    }

    #[test]
    fn rejects_agent_can_create_workers_referencing_unknown_template() {
        let src = r#"
            team default { description = "t" }
            agent coder {
                role = "i"
                instructions = "i"
                can_create_workers = ["ghost"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownWorkerRef { from, to }
                if from == "coder" && to == "ghost"
        ));
    }

    #[test]
    fn rejects_worker_can_create_workers_referencing_unknown_template() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            worker w {
                role = "r"
                instructions = "i"
                can_create_workers = ["ghost"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownWorkerRef { from, to }
                if from == "w" && to == "ghost"
        ));
    }

    #[test]
    fn parses_channel_block_with_members() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            agent b { role = "r" instructions = "i" }
            channel general {
                description = "team-wide"
                welcome = "hi"
                members = ["a", "b"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parses");
        assert_eq!(cfg.channels.len(), 1);
        let ch = &cfg.channels[0];
        assert_eq!(ch.id, "general");
        assert_eq!(ch.description.as_deref(), Some("team-wide"));
        assert_eq!(ch.welcome.as_deref(), Some("hi"));
        assert_eq!(ch.members, vec!["a".to_string(), "b".to_string()]);

        // Helpers work on the parsed config.
        let channels_for_a = cfg.channels_for_agent("a");
        assert_eq!(channels_for_a.len(), 1);
        assert_eq!(channels_for_a[0].id, "general");
        assert!(cfg.is_channel_member("general", "b"));
        assert!(!cfg.is_channel_member("general", "ghost"));
        assert!(cfg.channel("general").is_some());
        assert!(cfg.channel("nope").is_none());
    }

    #[test]
    fn rejects_channel_with_unknown_member() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            channel general {
                members = ["a", "ghost"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::UnknownChannelMember { ref channel, ref member }
                    if channel == "general" && member == "ghost"
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn rejects_channel_with_worker_member() {
        // Workers are ephemeral templates and cannot join channels —
        // the parser must treat a worker id in `members` the same as
        // an unknown agent id (since `members` only looks at agent
        // block ids).
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            worker w { role = "r" instructions = "i" }
            channel general {
                members = ["a", "w"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::UnknownChannelMember { ref channel, ref member }
                    if channel == "general" && member == "w"
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn rejects_channel_with_duplicate_members() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            channel general {
                members = ["a", "a"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::DuplicateChannelMember { ref channel, ref member }
                    if channel == "general" && member == "a"
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn channel_missing_members_is_an_error() {
        let src = r#"
            team default { description = "t" }
            channel general { }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::MissingAttribute { ref kind, ref attr }
                    if kind == "channel" && attr == "members"
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn can_create_returns_true_for_permitted_agent() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent coder {
                    role = "i"
                    instructions = "i"
                    can_create_workers = ["researcher"]
                }
                worker researcher { role = "r" instructions = "i" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(cfg.can_create("coder", "researcher"));
    }

    #[test]
    fn can_create_returns_false_when_not_listed() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent coder {
                    role = "i"
                    instructions = "i"
                    can_create_workers = ["researcher"]
                }
                worker researcher { role = "r" instructions = "i" }
                worker other      { role = "r" instructions = "i" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(!cfg.can_create("coder", "other"));
    }

    #[test]
    fn can_create_returns_false_for_agent_without_allowlist() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent coder { role = "i" instructions = "i" }
                worker researcher { role = "r" instructions = "i" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        assert!(!cfg.can_create("coder", "researcher"));
    }

    #[test]
    fn can_create_resolves_worker_template_actor() {
        let cfg = parse_config(
            r#"
                team default { description = "t" }
                agent coder { role = "i" instructions = "i" }
                worker researcher {
                    role = "r"
                    instructions = "i"
                    can_create_workers = ["fetcher"]
                }
                worker fetcher { role = "r" instructions = "i" }
            "#,
            &dummy_path(),
        )
        .unwrap();
        // Actor is a worker *template* id, not an instance id.
        assert!(cfg.can_create("researcher", "fetcher"));
        assert!(!cfg.can_create("fetcher", "researcher"));
    }

    #[test]
    fn can_create_unknown_actor_is_denied() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(!cfg.can_create("ghost", "alpha"));
    }

    #[test]
    fn thinking_messages_default_to_built_ins() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert_eq!(
            cfg.thinking_messages.len(),
            BUILT_IN_THINKING_MESSAGES.len()
        );
        assert!(!cfg.team.replace_thinking_messages);
    }

    #[test]
    fn thinking_messages_merge_user_blocks_by_default() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            thinking_messages { messages = ["mine 1", "mine 2"] }
            thinking_messages { messages = ["mine 3"] }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        // Built-ins first, then user-declared blocks in source order.
        assert_eq!(
            cfg.thinking_messages.len(),
            BUILT_IN_THINKING_MESSAGES.len() + 3
        );
        let tail: Vec<&str> = cfg
            .thinking_messages
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(String::as_str)
            .collect();
        assert_eq!(tail, vec!["mine 1", "mine 2", "mine 3"]);
    }

    #[test]
    fn thinking_messages_replace_when_flag_set_and_user_non_empty() {
        let src = r#"
            team default {
                description = "t"
                replace_thinking_messages = true
            }
            agent a { role = "r" instructions = "i" }
            thinking_messages { messages = ["only mine"] }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.team.replace_thinking_messages);
        assert_eq!(cfg.thinking_messages, vec!["only mine".to_string()]);
    }

    #[test]
    fn thinking_messages_fall_back_to_built_ins_when_replace_but_no_user_messages() {
        let src = r#"
            team default {
                description = "t"
                replace_thinking_messages = true
            }
            agent a { role = "r" instructions = "i" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.thinking_messages.len(),
            BUILT_IN_THINKING_MESSAGES.len()
        );
    }

    #[test]
    fn thinking_messages_pool_is_never_empty() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(!cfg.thinking_messages.is_empty());
    }

    #[test]
    fn replace_thinking_messages_must_be_bool() {
        let src = r#"
            team default {
                description = "t"
                replace_thinking_messages = "yes"
            }
            agent a { role = "r" instructions = "i" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::BadAttributeType { attr, .. }
                if attr == "replace_thinking_messages"
        ));
    }

    #[test]
    fn parses_ai_core_block_with_claude_code_variant() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core main {
                type = :claudeCode
                systemPrompt = "be concise"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores.len(), 1);
        assert_eq!(cfg.ai_cores[0].id, "main");
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::ClaudeCode {
                system_prompt: Some("be concise".into()),
                model: None,
                default_tools: None,
            }
        );
    }

    #[test]
    fn ai_core_usage_provider_defaults_to_none_on_omission() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core api {
                type = :anthropicApi
                apiUrl = "https://api.anthropic.com"
                apiKeyEnv = "ANTHROPIC_API_KEY"
                model = "claude-3-5-sonnet"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].usage_provider, None);
        assert_eq!(cfg.ai_cores[0].effective_usage_provider(), None);
    }

    #[test]
    fn ai_core_claude_code_has_no_usage_provider() {
        // claudeCode cores deliberately report no UsageProvider —
        // their reset data is populated from `rate_limit_event`
        // stream lines, not via an HTTP fetch.
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core cc { type = :claudeCode }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].usage_provider, None);
        assert_eq!(cfg.ai_cores[0].effective_usage_provider(), None);
    }

    #[test]
    fn ai_core_usage_provider_zai_symbol_is_parsed() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core z_ai {
                type = :anthropicApi
                usage_provider = :zai
                apiUrl = "https://api.z.ai/api/anthropic"
                apiKeyEnv = "ZAI_API_KEY"
                model = "glm-5.1"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].usage_provider, Some(UsageProvider::Zai));
    }

    #[test]
    fn ai_core_usage_provider_openrouter_symbol_is_parsed() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core openrouter {
                type = :openAiApi
                usage_provider = :openrouter
                apiUrl = "https://openrouter.ai/api/v1"
                apiKeyEnv = "OPENROUTER_API_KEY"
                model = "openai/gpt-5.4-mini"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].usage_provider,
            Some(UsageProvider::Openrouter)
        );
    }

    #[test]
    fn ai_core_usage_provider_github_copilot_symbol_is_parsed() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core copilot {
                type = :copilotCli
                usage_provider = :github_copilot
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].usage_provider,
            Some(UsageProvider::GithubCopilot)
        );
    }

    #[test]
    fn parses_automaton_block_with_full_fieldset() {
        let src = r#"
            team default { description = "t" }
            agent alice { role = "r" instructions = "i" }
            automaton quokka {
                role = "echo bot"
                role_description = "demo"
                command = "python3"
                args = ["-c", "import sys; print('hi')"]
                cwd = "/tmp"
                env = ["LOG=1", "MODE=safe"]
                timeout_seconds = 30
                can_message = ["alice"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parse ok");
        assert_eq!(cfg.automatons.len(), 1);
        let q = &cfg.automatons[0];
        assert_eq!(q.id, "quokka");
        assert_eq!(q.role, "echo bot");
        assert_eq!(q.role_description.as_deref(), Some("demo"));
        let AutomatonBackend::Script {
            command,
            args,
            cwd,
            env,
            timeout_seconds,
        } = &q.backend
        else {
            panic!("expected Script backend");
        };
        assert_eq!(command, "python3");
        assert_eq!(
            args,
            &vec!["-c".to_string(), "import sys; print('hi')".to_string()]
        );
        assert_eq!(cwd.as_deref(), Some("/tmp"));
        assert_eq!(
            env,
            &vec![
                ("LOG".to_string(), "1".to_string()),
                ("MODE".to_string(), "safe".to_string()),
            ]
        );
        assert_eq!(timeout_seconds, &Some(30));
        assert_eq!(q.can_message.as_deref(), Some(&["alice".to_string()][..]));
    }

    #[test]
    fn automaton_defaults_fill_in_when_optional_fields_omitted() {
        let src = r#"
            team default { description = "t" }
            automaton bare {
                role = "idle"
                command = "/bin/true"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parse ok");
        let b = &cfg.automatons[0];
        let AutomatonBackend::Script {
            args,
            cwd,
            env,
            timeout_seconds,
            ..
        } = &b.backend
        else {
            panic!("expected Script backend");
        };
        assert!(args.is_empty());
        assert!(cwd.is_none());
        assert!(env.is_empty());
        assert!(timeout_seconds.is_none());
        assert!(b.can_message.is_none());
    }

    #[test]
    fn parses_webservice_automaton_with_https_bind() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            automaton api_server {
                type = :webservice
                role = "HTTPS API"
                bind = "0.0.0.0:8443"
                tls_cert = "/etc/certs/aciddog.crt"
                tls_key = "/etc/certs/aciddog.key"
                api_key_env = "ACIDDOG_API_KEY"
                can_message = ["planner"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parse ok");
        let a = &cfg.automatons[0];
        let AutomatonBackend::Webservice { bind, api_key_env } = &a.backend else {
            panic!("expected Webservice backend");
        };
        let WebserviceBind::Https {
            address,
            tls_cert,
            tls_key,
        } = bind
        else {
            panic!("expected Https bind");
        };
        assert_eq!(address, "0.0.0.0:8443");
        assert_eq!(tls_cert.to_str(), Some("/etc/certs/aciddog.crt"));
        assert_eq!(tls_key.to_str(), Some("/etc/certs/aciddog.key"));
        assert_eq!(api_key_env.as_deref(), Some("ACIDDOG_API_KEY"));
    }

    #[test]
    fn parses_webservice_automaton_with_unix_socket() {
        let src = r#"
            team default { description = "t" }
            automaton local_api {
                type = :webservice
                role = "local control"
                socket_path = "/tmp/aciddog.sock"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parse ok");
        let a = &cfg.automatons[0];
        let AutomatonBackend::Webservice { bind, api_key_env } = &a.backend else {
            panic!("expected Webservice backend");
        };
        let WebserviceBind::UnixSocket { path } = bind else {
            panic!("expected UnixSocket bind");
        };
        assert_eq!(path.to_str(), Some("/tmp/aciddog.sock"));
        assert!(api_key_env.is_none());
    }

    #[test]
    fn rejects_command_on_webservice_automaton() {
        let src = r#"
            team default { description = "t" }
            automaton a {
                type = :webservice
                role = "x"
                socket_path = "/tmp/x.sock"
                command = "python3"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(&err, ConfigError::WebserviceUnexpectedScriptField { field, .. } if field == "command"),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webservice_fields_on_script_automaton() {
        let src = r#"
            team default { description = "t" }
            automaton a {
                role = "x"
                command = "cat"
                bind = "0.0.0.0:8443"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(&err, ConfigError::ScriptUnexpectedWebserviceField { field, .. } if field == "bind"),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webservice_with_both_bind_and_socket_path() {
        let src = r#"
            team default { description = "t" }
            automaton a {
                type = :webservice
                role = "x"
                bind = "0.0.0.0:8443"
                tls_cert = "c"
                tls_key = "k"
                api_key_env = "K"
                socket_path = "/tmp/x.sock"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebserviceBindConflict { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webservice_without_any_bind() {
        let src = r#"
            team default { description = "t" }
            automaton a {
                type = :webservice
                role = "x"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebserviceMissingBind { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_unix_socket_webservice_with_api_key_env() {
        let src = r#"
            team default { description = "t" }
            automaton a {
                type = :webservice
                role = "x"
                socket_path = "/tmp/x.sock"
                api_key_env = "FOO"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebserviceUnixSocketApiKey { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn parses_webhook_bearer_auth() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            automaton api_server {
                type = :webservice
                role = "HTTPS API"
                socket_path = "/tmp/x.sock"
                can_message = ["planner"]
            }
            webhook hook_bearer {
                mount = "api_server"
                path = "/hooks/x"
                deliver_to = "planner"
                auth_kind = :bearer
                auth_secret_env = "SEC"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parse ok");
        assert_eq!(cfg.webhooks.len(), 1);
        let w = &cfg.webhooks[0];
        assert_eq!(w.id, "hook_bearer");
        assert_eq!(w.mount, "api_server");
        assert_eq!(w.path, "/hooks/x");
        assert_eq!(w.deliver_to, "planner");
        assert!(matches!(w.auth, WebhookAuth::Bearer { ref secret_env } if secret_env == "SEC"));
    }

    #[test]
    fn parses_webhook_hmac_header_defaults_to_sha256() {
        let src = r#"
            team default { description = "t" }
            agent coder { role = "r" instructions = "i" }
            automaton api_server {
                type = :webservice
                role = "api"
                socket_path = "/tmp/x.sock"
                can_message = ["coder"]
            }
            webhook gh {
                mount = "api_server"
                path = "/hooks/gh"
                deliver_to = "coder"
                auth_kind = :hmac_header
                auth_header = "X-Hub-Signature-256"
                auth_secret_env = "GHSEC"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("parse ok");
        let w = &cfg.webhooks[0];
        assert!(matches!(
            w.auth,
            WebhookAuth::HmacHeader {
                algo: HmacAlgo::Sha256,
                ..
            }
        ));
    }

    #[test]
    fn rejects_webhook_mount_on_script_automaton() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            automaton script_bot {
                role = "x"
                command = "cat"
            }
            webhook w {
                mount = "script_bot"
                path = "/h"
                deliver_to = "planner"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebhookMountNotWebservice { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webhook_with_unknown_mount() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            webhook w {
                mount = "nope"
                path = "/h"
                deliver_to = "planner"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebhookUnknownMount { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webhook_deliver_not_reachable_from_mount() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            agent coder { role = "r" instructions = "i" }
            automaton api {
                type = :webservice
                role = "x"
                socket_path = "/tmp/x.sock"
                can_message = ["planner"]
            }
            webhook w {
                mount = "api"
                path = "/h"
                deliver_to = "coder"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebhookDeliverNotReachable { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_duplicate_webhook_paths_on_same_mount() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            automaton api {
                type = :webservice
                role = "x"
                socket_path = "/tmp/x.sock"
            }
            webhook a { mount = "api" path = "/h" deliver_to = "planner" }
            webhook b { mount = "api" path = "/h" deliver_to = "planner" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebhookPathConflict { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webhook_path_that_collides_with_reserved() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            automaton api {
                type = :webservice
                role = "x"
                socket_path = "/tmp/x.sock"
            }
            webhook a { mount = "api" path = "/v1/rpc" deliver_to = "planner" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebhookReservedPath { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_webhook_invalid_path() {
        let src = r#"
            team default { description = "t" }
            agent planner { role = "r" instructions = "i" }
            automaton api {
                type = :webservice
                role = "x"
                socket_path = "/tmp/x.sock"
            }
            webhook a { mount = "api" path = "missing-slash" deliver_to = "planner" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(
            matches!(err, ConfigError::WebhookInvalidPath { .. }),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn automaton_rejects_env_entry_without_equals() {
        let src = r#"
            team default { description = "t" }
            automaton bad {
                role = "x"
                command = "cat"
                env = ["OK=1", "MALFORMED"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::BadEnvEntry { automaton, entry } if automaton == "bad" && entry == "MALFORMED"
        ));
    }

    #[test]
    fn automaton_id_is_valid_in_agent_can_message_list() {
        let src = r#"
            team default { description = "t" }
            automaton logger { role = "x" command = "cat" }
            agent talker {
                role = "r"
                instructions = "i"
                can_message = ["logger"]
            }
        "#;
        parse_config(src, &dummy_path()).expect("automaton is a valid can_message target");
    }

    #[test]
    fn automaton_can_send_to_ids_in_its_allowlist() {
        let src = r#"
            team default { description = "t" }
            agent alice { role = "r" instructions = "i" }
            agent bob   { role = "r" instructions = "i" }
            automaton logger {
                role = "x"
                command = "cat"
                can_message = ["alice"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.can_send("logger", "alice"));
        assert!(!cfg.can_send("logger", "bob"));
    }

    #[test]
    fn automaton_with_no_allowlist_can_send_to_anyone() {
        let src = r#"
            team default { description = "t" }
            agent alice { role = "r" instructions = "i" }
            automaton logger { role = "x" command = "cat" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.can_send("logger", "alice"));
    }

    #[test]
    fn all_addressable_ids_includes_agents_and_automatons() {
        let src = r#"
            team default { description = "t" }
            agent alice { role = "r" instructions = "i" }
            automaton logger { role = "x" command = "cat" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let ids = cfg.all_addressable_ids();
        assert_eq!(ids, vec!["alice".to_string(), "logger".to_string()]);
    }

    #[test]
    fn automaton_id_is_valid_in_channel_members() {
        let src = r#"
            team default { description = "t" }
            agent alice { role = "r" instructions = "i" }
            automaton logger { role = "x" command = "cat" }
            channel ops {
                members = ["alice", "logger"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).expect("automaton can join a channel");
        assert_eq!(
            cfg.channels[0].members,
            vec!["alice".to_string(), "logger".to_string()]
        );
    }

    #[test]
    fn demo_aciddog_wcl_parses_with_usage_provider_annotation() {
        // Smoke test: the committed demo/aciddog.wcl should round-trip
        // through the parser, exercising every variant including the
        // z_ai core's `usage_provider = :zai` annotation.
        let src = include_str!("../../../demo/aciddog.wcl");
        let cfg = parse_config(src, &dummy_path()).expect("demo wcl parses");
        let z_ai = cfg
            .ai_cores
            .iter()
            .find(|c| c.id == "z_ai")
            .expect("z_ai core present");
        assert_eq!(z_ai.usage_provider, Some(UsageProvider::Zai));
        // claudeCode cores no longer infer an HTTP usage provider —
        // their reset data is stream-derived.
        let default_core = cfg
            .ai_cores
            .iter()
            .find(|c| c.id == "default_core")
            .expect("default_core present");
        assert!(default_core.effective_usage_provider().is_none());
    }

    #[test]
    fn ai_core_usage_provider_unknown_symbol_is_rejected() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core cc {
                type = :claudeCode
                usage_provider = :nonsense
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownUsageProvider { ref value, .. } if value == "nonsense"
        ));
    }

    #[test]
    fn ai_core_system_prompt_is_optional() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core bare { type = :claudeCode }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::ClaudeCode {
                system_prompt: None,
                model: None,
                default_tools: None,
            }
        );
    }

    #[test]
    fn parses_ai_core_default_tools() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core main {
                type = :claudeCode
                default_tools = ["Bash", "Read"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].default_tools(),
            Some(&["Bash".into(), "Read".into()][..])
        );
    }

    #[test]
    fn parses_ai_core_block_with_anthropic_api_variant() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core z_ai {
                type = :anthropicApi
                apiUrl = "https://api.z.ai/api/anthropic"
                apiKeyEnv = "ZAI_API_KEY"
                model = "glm-5.1"
                default_tools = ["Bash", "Read", "WebFetch"]
                systemPrompt = "be concise"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::AnthropicApi {
                api_url: "https://api.z.ai/api/anthropic".into(),
                api_key_env: "ZAI_API_KEY".into(),
                model: "glm-5.1".into(),
                system_prompt: Some("be concise".into()),
                default_tools: Some(vec!["Bash".into(), "Read".into(), "WebFetch".into()]),
            }
        );
    }

    #[test]
    fn anthropic_api_requires_api_url_key_env_and_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core z_ai { type = :anthropicApi }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(err, ConfigError::MissingAttribute { .. }));
    }

    #[test]
    fn anthropic_api_rejects_unknown_tool_name() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core z_ai {
                type = :anthropicApi
                apiUrl = "http://x"
                apiKeyEnv = "K"
                model = "m"
                default_tools = ["Read", "NotARealTool"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownTool { tool, .. } if tool == "NotARealTool"
        ));
    }

    #[test]
    fn parses_ai_core_block_with_openai_api_variant() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core openrouter {
                type = :openAiApi
                apiUrl = "https://openrouter.ai/api/v1"
                apiKeyEnv = "OPENROUTER_API_KEY"
                model = "openai/gpt-5.4-mini"
                default_tools = ["Read", "WebFetch"]
                systemPrompt = "be concise"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::OpenAiApi {
                api_url: "https://openrouter.ai/api/v1".into(),
                api_key_env: "OPENROUTER_API_KEY".into(),
                model: "openai/gpt-5.4-mini".into(),
                system_prompt: Some("be concise".into()),
                default_tools: Some(vec!["Read".into(), "WebFetch".into()]),
            }
        );
    }

    #[test]
    fn openai_api_requires_api_url_key_env_and_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core openrouter { type = :openAiApi }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(err, ConfigError::MissingAttribute { .. }));
    }

    #[test]
    fn openai_api_rejects_unknown_tool_name() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core openrouter {
                type = :openAiApi
                apiUrl = "http://x"
                apiKeyEnv = "K"
                model = "m"
                default_tools = ["Read", "NotARealTool"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownTool { tool, .. } if tool == "NotARealTool"
        ));
    }

    #[test]
    fn openai_api_runtime_model_returns_configured_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core openrouter {
                type = :openAiApi
                apiUrl = "http://x"
                apiKeyEnv = "K"
                model = "openai/gpt-5.4-mini"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].runtime_model(), Some("openai/gpt-5.4-mini"));
    }

    #[test]
    fn parses_ai_core_block_with_openai_chatgpt_variant() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core chatgpt {
                type = :openAiChatgpt
                credential = "personal"
                model = "gpt-5.1-codex"
                reasoningEffort = "medium"
                systemPrompt = "be concise"
                default_tools = ["Read", "Bash"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::OpenAiChatgpt {
                credential: "personal".into(),
                model: "gpt-5.1-codex".into(),
                system_prompt: Some("be concise".into()),
                default_tools: Some(vec!["Read".into(), "Bash".into()]),
                reasoning_effort: Some("medium".into()),
            }
        );
    }

    #[test]
    fn openai_chatgpt_requires_credential_and_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core chatgpt { type = :openAiChatgpt }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(err, ConfigError::MissingAttribute { .. }));
    }

    #[test]
    fn openai_chatgpt_rejects_bad_credential_name() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core chatgpt {
                type = :openAiChatgpt
                credential = "Has Spaces"
                model = "gpt-5.1-codex"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidCredentialName { credential, .. }
                if credential == "Has Spaces"
        ));
    }

    #[test]
    fn openai_chatgpt_rejects_minimal_reasoning_effort() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core chatgpt {
                type = :openAiChatgpt
                credential = "personal"
                model = "gpt-5.1-codex"
                reasoningEffort = "minimal"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidReasoningEffort { value, .. } if value == "minimal"
        ));
    }

    #[test]
    fn openai_chatgpt_rejects_unknown_tool_name() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core chatgpt {
                type = :openAiChatgpt
                credential = "personal"
                model = "gpt-5.1-codex"
                default_tools = ["Read", "NotARealTool"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownTool { tool, .. } if tool == "NotARealTool"
        ));
    }

    #[test]
    fn openai_chatgpt_runtime_model_returns_configured_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core chatgpt {
                type = :openAiChatgpt
                credential = "personal"
                model = "gpt-5.1-codex"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].runtime_model(), Some("gpt-5.1-codex"));
    }

    #[test]
    fn rejects_unknown_ai_core_type() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core main { type = :mystery }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownAiCoreType { core, type_name }
                if core == "main" && type_name == "mystery"
        ));
    }

    #[test]
    fn rejects_ai_core_missing_type() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core bad { }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::MissingAttribute { attr, .. } if attr == "type"
        ));
    }

    #[test]
    fn parses_agent_tools_and_ai_core_override() {
        let src = r#"
            team default { description = "t" }
            ai_core main { type = :claudeCode }
            agent alpha {
                role = "r"
                instructions = "i"
                tools = ["Read", "Grep"]
                ai_core = "main"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.agents[0].tools.as_deref(),
            Some(&["Read".into(), "Grep".into()][..])
        );
        assert_eq!(cfg.agents[0].ai_core.as_deref(), Some("main"));
    }

    #[test]
    fn parses_worker_ai_core_override() {
        let src = r#"
            team default { description = "t" }
            ai_core main { type = :claudeCode }
            agent a { role = "r" instructions = "i" }
            worker researcher {
                role = "r"
                instructions = "i"
                ai_core = "main"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.workers[0].ai_core.as_deref(), Some("main"));
    }

    #[test]
    fn parses_team_default_ai_core() {
        let src = r#"
            ai_core main { type = :claudeCode }
            team default {
                description = "t"
                default_ai_core = "main"
            }
            agent a { role = "r" instructions = "i" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.team.default_ai_core.as_deref(), Some("main"));
    }

    #[test]
    fn rejects_agent_ai_core_referencing_unknown_id() {
        let src = r#"
            team default { description = "t" }
            agent a {
                role = "r"
                instructions = "i"
                ai_core = "ghost"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownAiCoreRef { from, attr, to }
                if from == "a" && attr == "ai_core" && to == "ghost"
        ));
    }

    #[test]
    fn rejects_team_default_ai_core_referencing_unknown_id() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "ghost"
            }
            agent a { role = "r" instructions = "i" }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownAiCoreRef { attr, to, .. }
                if attr == "default_ai_core" && to == "ghost"
        ));
    }

    #[test]
    fn effective_tools_prefers_override_then_core_then_builtin() {
        let src = r#"
            ai_core main {
                type = :claudeCode
                default_tools = ["Bash"]
            }
            team default {
                description = "t"
                default_ai_core = "main"
            }
            agent a {
                role = "r"
                instructions = "i"
                tools = ["Read"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let core = cfg.effective_ai_core(None);
        // Override wins over the core's default_tools.
        assert_eq!(
            cfg.effective_tools(cfg.agents[0].tools.as_deref(), core),
            vec!["Read".to_string()]
        );
        // Falls back to the core's default_tools when no override.
        assert_eq!(cfg.effective_tools(None, core), vec!["Bash".to_string()]);

        // With no core default either, the built-in fallback applies.
        let cfg2 = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert_eq!(
            cfg2.effective_tools(None, None),
            DEFAULT_ALLOWED_TOOLS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn effective_ai_core_prefers_override_then_team_default() {
        let src = r#"
            ai_core primary { type = :claudeCode systemPrompt = "p" }
            ai_core alt     { type = :claudeCode systemPrompt = "a" }
            team default {
                description = "t"
                default_ai_core = "primary"
            }
            agent override_agent {
                role = "r"
                instructions = "i"
                ai_core = "alt"
            }
            agent default_agent {
                role = "r"
                instructions = "i"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let override_core = cfg.effective_ai_core(cfg.agents[0].ai_core.as_deref());
        assert_eq!(override_core.map(|c| c.id.as_str()), Some("alt"));

        let default_core = cfg.effective_ai_core(cfg.agents[1].ai_core.as_deref());
        assert_eq!(default_core.map(|c| c.id.as_str()), Some("primary"));
    }

    #[test]
    fn effective_ai_core_returns_none_when_nothing_configured() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(cfg.effective_ai_core(None).is_none());
    }

    #[test]
    fn effective_model_falls_back_to_core_when_actor_unset() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "main"
            }
            agent alpha {
                role = "r"
                instructions = "i"
            }
            ai_core main {
                type = :anthropicApi
                apiUrl = "https://api.anthropic.com"
                apiKeyEnv = "ANTHROPIC_API_KEY"
                model = "claude-sonnet-4-6"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let core = cfg.effective_ai_core(cfg.agents[0].ai_core.as_deref());
        assert_eq!(
            cfg.effective_model(cfg.agents[0].model.as_deref(), core),
            Some("claude-sonnet-4-6"),
        );
    }

    #[test]
    fn effective_model_agent_override_wins_over_core() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "main"
            }
            agent alpha {
                role = "r"
                model = "claude-opus-4-7"
                instructions = "i"
            }
            ai_core main {
                type = :anthropicApi
                apiUrl = "https://api.anthropic.com"
                apiKeyEnv = "ANTHROPIC_API_KEY"
                model = "claude-sonnet-4-6"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let core = cfg.effective_ai_core(cfg.agents[0].ai_core.as_deref());
        assert_eq!(
            cfg.effective_model(cfg.agents[0].model.as_deref(), core),
            Some("claude-opus-4-7"),
        );
    }

    #[test]
    fn claude_code_core_model_is_optional_in_wcl() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core local {
                type = :claudeCode
                model = "claude-haiku-4.5"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].runtime_model(), Some("claude-haiku-4.5"));
    }

    #[test]
    fn claude_code_core_without_model_returns_none_runtime_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core local { type = :claudeCode }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.ai_cores[0].runtime_model().is_none());
    }

    #[test]
    fn parses_ai_core_block_with_gemini_cli_variant() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core gemini_flash {
                type = :geminiCli
                model = "auto-gemini-3"
                default_tools = ["Read", "Grep"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::GeminiCli {
                model: Some("auto-gemini-3".into()),
                default_tools: Some(vec!["Read".into(), "Grep".into()]),
            }
        );
    }

    #[test]
    fn gemini_cli_model_is_optional() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core bare { type = :geminiCli }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::GeminiCli {
                model: None,
                default_tools: None,
            }
        );
    }

    #[test]
    fn gemini_cli_runtime_model_returns_configured_model() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core g { type = :geminiCli model = "auto-gemini-3" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.ai_cores[0].runtime_model(), Some("auto-gemini-3"));
    }

    #[test]
    fn parses_ai_core_block_with_jules_variant() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core jules_remote {
                type = :jules
                apiKeyEnv = "JULES_API_KEY"
                defaultBranch = "main"
                pollIntervalSeconds = 15
                requirePlanApproval = true
                automationMode = "AUTO_CREATE_PR"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::Jules {
                api_key_env: "JULES_API_KEY".into(),
                default_branch: Some("main".into()),
                poll_interval_seconds: Some(15),
                require_plan_approval: Some(true),
                automation_mode: Some("AUTO_CREATE_PR".into()),
            }
        );
    }

    #[test]
    fn jules_variant_optional_fields_default_to_none() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core jr {
                type = :jules
                apiKeyEnv = "JULES_API_KEY"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.ai_cores[0].kind,
            AiCoreKind::Jules {
                api_key_env: "JULES_API_KEY".into(),
                default_branch: None,
                poll_interval_seconds: None,
                require_plan_approval: None,
                automation_mode: None,
            }
        );
    }

    #[test]
    fn jules_variant_requires_api_key_env() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core jr { type = :jules }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(err, ConfigError::MissingAttribute { .. }));
    }

    #[test]
    fn gemini_cli_rejects_unknown_tool_name() {
        let src = r#"
            team default { description = "t" }
            agent a { role = "r" instructions = "i" }
            ai_core g {
                type = :geminiCli
                default_tools = ["Read", "WebFetch"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownTool { tool, .. } if tool == "WebFetch"
        ));
    }

    #[test]
    fn config_parses_agent_skills_list() {
        let src = r#"
            team default { description = "t" }
            agent one {
                role = "a"
                instructions = "b"
                skills = ["research", "summariser"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        let skills = cfg.agents[0].skills.as_ref().unwrap();
        assert_eq!(
            skills,
            &vec!["research".to_string(), "summariser".to_string()]
        );
        assert_eq!(
            cfg.skills_for("one"),
            vec!["research".to_string(), "summariser".to_string()]
        );
    }

    #[test]
    fn config_parses_worker_skills_list() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" }
            worker fetcher {
                role = "w"
                instructions = "c"
                skills = ["research"]
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(
            cfg.workers[0].skills.as_deref(),
            Some(&["research".to_string()][..])
        );
        assert_eq!(cfg.skills_for("fetcher"), vec!["research".to_string()]);
    }

    #[test]
    fn config_rejects_invalid_agent_skill_name() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" skills = ["Bad Name!"] }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidSkillName { from, name }
                if from == "one" && name == "Bad Name!"
        ));
    }

    #[test]
    fn config_rejects_path_traversal_skill_name() {
        let src = r#"
            team default { description = "t" }
            agent one { role = "a" instructions = "b" skills = [".."] }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidSkillName { name, .. } if name == ".."));
    }

    #[test]
    fn skills_for_unknown_actor_is_empty() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(cfg.skills_for("ghost").is_empty());
    }

    #[test]
    fn parses_all_four_compaction_strategies() {
        let src = r#"
            team default { description = "t" }
            compaction just_harness { strategy = :harness }
            compaction just_drop    { strategy = :drop }
            compaction with_prompt  {
                strategy = :promptAndDrop
                prompt   = "final thoughts?"
            }
            compaction summary_pass {
                strategy = :summarise
                prompt   = "summarise"
                ai_core  = "backup"
            }
            ai_core backup {
                type = :anthropicApi
                apiUrl = "https://x"
                apiKeyEnv = "K"
                model = "m"
            }
            agent alpha { role = "a" instructions = "b" }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.compactions.len(), 4);
        assert!(matches!(
            cfg.compaction("just_harness").unwrap().strategy,
            CompactionStrategy::Harness
        ));
        assert!(matches!(
            cfg.compaction("just_drop").unwrap().strategy,
            CompactionStrategy::Drop
        ));
        match &cfg.compaction("with_prompt").unwrap().strategy {
            CompactionStrategy::PromptAndDrop { prompt } => {
                assert_eq!(prompt, "final thoughts?")
            }
            other => panic!("expected PromptAndDrop, got {other:?}"),
        }
        match &cfg.compaction("summary_pass").unwrap().strategy {
            CompactionStrategy::Summarise { prompt, ai_core } => {
                assert_eq!(prompt, "summarise");
                assert_eq!(ai_core.as_deref(), Some("backup"));
            }
            other => panic!("expected Summarise, got {other:?}"),
        }
    }

    #[test]
    fn agent_compaction_attribute_is_parsed() {
        let src = r#"
            team default { description = "t" }
            compaction c { strategy = :drop }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "c"
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert_eq!(cfg.agents[0].compaction.as_deref(), Some("c"));
        let resolved = cfg.compaction_for("alpha").expect("resolves");
        assert_eq!(resolved.id, "c");
    }

    #[test]
    fn rejects_unknown_compaction_ref() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "ghost"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownCompactionRef { from, to }
                if from == "alpha" && to == "ghost"
        ));
    }

    #[test]
    fn rejects_harness_strategy_on_direct_api_core() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "direct"
            }
            ai_core direct {
                type = :anthropicApi
                apiUrl = "https://x"
                apiKeyEnv = "K"
                model = "m"
            }
            compaction h { strategy = :harness }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "h"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::CompactionHarnessUnsupportedCore { actor, core_type, .. }
                if actor == "alpha" && core_type == "anthropicApi"
        ));
    }

    #[test]
    fn rejects_harness_strategy_on_gemini_cli_core() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "g"
            }
            ai_core g { type = :geminiCli }
            compaction h { strategy = :harness }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "h"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::CompactionHarnessUnsupportedCore { actor, core_type, .. }
                if actor == "alpha" && core_type == "geminiCli"
        ));
    }

    #[test]
    fn rejects_harness_strategy_on_copilot_cli_core() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "c"
            }
            ai_core c { type = :copilotCli }
            compaction h { strategy = :harness }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "h"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::CompactionHarnessUnsupportedCore { actor, core_type, .. }
                if actor == "alpha" && core_type == "copilotCli"
        ));
    }

    #[test]
    fn accepts_harness_strategy_on_claude_code_core() {
        let src = r#"
            team default {
                description = "t"
                default_ai_core = "cc"
            }
            ai_core cc { type = :claudeCode }
            compaction h { strategy = :harness }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "h"
            }
        "#;
        parse_config(src, &dummy_path()).expect("claudeCode + harness is fine");
    }

    #[test]
    fn rejects_summarise_with_unknown_ai_core() {
        let src = r#"
            team default { description = "t" }
            compaction s {
                strategy = :summarise
                prompt   = "s"
                ai_core  = "ghost"
            }
            agent alpha {
                role = "a"
                instructions = "b"
                compaction = "s"
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownAiCoreRef { attr, to, .. }
                if attr == "ai_core" && to == "ghost"
        ));
    }

    #[test]
    fn compaction_for_unknown_actor_returns_none() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert!(cfg.compaction_for("ghost").is_none());
        // Known agent, no compaction attribute set.
        assert!(cfg.compaction_for("alpha").is_none());
    }

    // --------- Workflows ----------

    const WORKFLOW_BASE: &str = r#"
        team default { description = "t" }
        worker researcher {
            role = "r"
            instructions = "i"
        }
    "#;

    fn parse_with_workflow(wf: &str) -> Result<AciddogConfig, ConfigError> {
        let src = format!("{}{}", WORKFLOW_BASE, wf);
        parse_config(&src, &dummy_path())
    }

    #[test]
    fn parses_workflow_with_every_property_kind() {
        let cfg = parse_with_workflow(
            r#"
            workflow sample {
                description = "a demo"
                property b1 { kind = :text default = "hi" }
                property b2 { kind = :number default = 1 min = 0 max = 10 }
                property b3 { kind = :select options = ["a","b"] default = "a" }
                property b4 { kind = :multiSelect options = ["x","y"] default = ["x"] }
                property b5 { kind = :hidden default = "secret" }

                step only { kind = :command command = "echo $$b1$$" }
            }
        "#,
        )
        .expect("parse ok");
        let wf = &cfg.workflows[0];
        assert_eq!(wf.id, "sample");
        assert_eq!(wf.max_retries, 5);
        assert_eq!(wf.properties.len(), 5);
        assert!(matches!(wf.properties[0].kind, PropertyKind::Text { .. }));
        assert!(matches!(wf.properties[4].kind, PropertyKind::Hidden { .. }));
        assert!(!wf.properties[4].kind.is_user_visible());
    }

    #[test]
    fn parses_workflow_step_kinds() {
        let cfg = parse_with_workflow(
            r#"
            workflow sample {
                description = "d"
                max_retries = 2
                property coverage { kind = :hidden default = "0" }
                step a { kind = :command command = "true" }
                step b { kind = :worker worker = "researcher" prompt = "p" }
                step c { kind = :conditional worker = "researcher" prompt = "p" target = "a" }
                step d { kind = :check variable = "coverage" op = :equals value = "100" target = "a" }
                step e { kind = :check variable = "coverage" op = :notEmpty target = "a" }
            }
        "#,
        )
        .expect("parse ok");
        let wf = &cfg.workflows[0];
        assert_eq!(wf.max_retries, 2);
        assert_eq!(wf.steps.len(), 5);
        assert!(matches!(wf.steps[0].kind, StepKind::Command { .. }));
        assert!(matches!(wf.steps[1].kind, StepKind::Worker { .. }));
        assert!(matches!(wf.steps[2].kind, StepKind::Conditional { .. }));
        assert!(matches!(
            wf.steps[3].kind,
            StepKind::Check {
                op: CheckOp::Equals,
                ..
            }
        ));
        assert!(matches!(
            wf.steps[4].kind,
            StepKind::Check {
                op: CheckOp::NotEmpty,
                ..
            }
        ));
    }

    // Note: wcl_lang merges child blocks that share an id (see
    // merge_child_blocks_with_same_inline_id in wcl_lang), so duplicate
    // property/step ids can't actually surface via the parser. The
    // DuplicatePropertyId / DuplicateStepId variants remain as defence
    // in depth against programmatic config construction.

    #[test]
    fn rejects_select_default_not_in_options() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                property p { kind = :select options = ["a","b"] default = "c" }
                step s { kind = :command command = "true" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidPropertyDefault { default, .. } if default == "c"
        ));
    }

    #[test]
    fn rejects_number_min_greater_than_max() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                property p { kind = :number min = 5 max = 1 }
                step s { kind = :command command = "true" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidPropertyRange { .. }));
    }

    #[test]
    fn rejects_unknown_worker_ref_in_step() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                step s { kind = :worker worker = "ghost" prompt = "p" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownWorkflowWorkerRef { worker, .. } if worker == "ghost"
        ));
    }

    #[test]
    fn rejects_unknown_step_target() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                step s { kind = :conditional worker = "researcher" prompt = "p" target = "ghost" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownWorkflowStepTarget { target, .. } if target == "ghost"
        ));
    }

    #[test]
    fn rejects_check_with_unknown_variable() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                step a { kind = :command command = "true" }
                step b { kind = :check variable = "ghost" op = :notEmpty target = "a" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownWorkflowVariable { variable, .. } if variable == "ghost"
        ));
    }

    #[test]
    fn rejects_check_op_missing_required_value() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                property c { kind = :hidden }
                step a { kind = :command command = "true" }
                step b { kind = :check variable = "c" op = :equals target = "a" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::CheckMissingValue { .. }));
    }

    #[test]
    fn rejects_check_op_with_forbidden_value() {
        let err = parse_with_workflow(
            r#"
            workflow w {
                description = "d"
                property c { kind = :hidden }
                step a { kind = :command command = "true" }
                step b { kind = :check variable = "c" op = :empty value = "x" target = "a" }
            }
        "#,
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::CheckUnexpectedValue { .. }));
    }

    #[test]
    fn rejects_can_run_workflows_unknown_id() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "r" instructions = "i"
                can_run_workflows = ["ghost"]
            }
        "#;
        let err = parse_config(src, &dummy_path()).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::UnknownWorkflowRef { to, .. } if to == "ghost"
        ));
    }

    #[test]
    fn can_run_workflow_helper_honours_allowlist() {
        let src = r#"
            team default { description = "t" }
            agent alpha {
                role = "r" instructions = "i"
                can_run_workflows = ["wf1"]
            }
            agent beta {
                role = "r" instructions = "i"
            }
            workflow wf1 {
                description = "d"
                step s { kind = :command command = "true" }
            }
        "#;
        let cfg = parse_config(src, &dummy_path()).unwrap();
        assert!(cfg.can_run_workflow("alpha", "wf1"));
        assert!(!cfg.can_run_workflow("alpha", "wf2"));
        assert!(!cfg.can_run_workflow("beta", "wf1"));
        assert!(!cfg.can_run_workflow("ghost", "wf1"));
    }
}
