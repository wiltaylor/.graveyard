export type Transport = "https" | "unix";

export interface InstanceSummary {
  id: string;
  name: string;
  transport: Transport;
  url: string | null;
  socket_path: string | null;
  tls_ca_path: string | null;
  has_api_key: boolean;
  created_at: string;
}

export interface CreateInstanceInput {
  name: string;
  transport: Transport;
  url?: string;
  socket_path?: string;
  api_key?: string;
  tls_ca_path?: string;
}

export interface AgentRow {
  id: string;
  kind: string;
  status: string;
  parent: string | null;
  ai_core: string | null;
}

export interface AgentListResponse {
  agents: AgentRow[];
  ok: boolean;
  raw: string | null;
  error: string | null;
}

export interface HealthResponse {
  ok: boolean;
  status: number;
  body: string;
}

export interface ApiError {
  error: string;
}

// ---------------------------------------------------------------------------
// Chat — mirrors `aciddog::wire_chat::WireChatBlock` and `ChatBlockEvent`.
// ---------------------------------------------------------------------------

export type WireSystemLevel = "info" | "warn" | "error";
export type WireMailDirection = "sent" | "received";
export type WireTaskOpKind =
  | "add"
  | "list"
  | "show"
  | "update"
  | "remove"
  | "take"
  | "note"
  | "todo_add"
  | "todo_list"
  | "todo_update"
  | "todo_remove"
  | "comment_add"
  | "comment_list"
  | "comment_update"
  | "comment_remove";

export type WireChatBlock =
  | {
      kind: "agent_info";
      core: string;
      tools: string[];
      prompt: string;
      context_input_tokens?: number;
      context_output_tokens?: number;
      context_cache_read_tokens?: number;
      context_cache_creation_tokens?: number;
      context_window?: number;
    }
  | { kind: "user_text"; text: string }
  | { kind: "ai_markdown"; raw: string }
  | {
      kind: "bash_tool";
      command: string;
      stdout: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "edit_tool";
      file_path: string;
      diff: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "write_tool";
      file_path: string;
      diff: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "read_tool";
      file_path: string;
      range?: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "grep_tool";
      pattern: string;
      path?: string;
      matches: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "glob_tool";
      pattern: string;
      path?: string;
      matches: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "generic_tool";
      name: string;
      input_pretty: string;
      result_pretty: string;
      is_error: boolean;
      pending: boolean;
    }
  | { kind: "system"; text: string; level: WireSystemLevel }
  | {
      kind: "mail";
      direction: WireMailDirection;
      counterpart: string;
      body: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "worker_spawn";
      template: string;
      worker_id?: string;
      task: string;
      is_error: boolean;
      pending: boolean;
    }
  | { kind: "worker_complete"; body: string; is_error: boolean; pending: boolean }
  | {
      kind: "channel_message";
      sender: string;
      body: string;
      is_welcome: boolean;
    }
  | {
      kind: "remote_patch";
      summary: string;
      diff: string;
      is_error: boolean;
      pending: boolean;
    }
  | {
      kind: "task_op";
      op: WireTaskOpKind;
      subject: string;
      body: string;
      reply: string;
      is_error: boolean;
      pending: boolean;
    }
  | { kind: "automaton_stdout"; line: string }
  | { kind: "automaton_stderr"; line: string };

export interface ChatBlockEvent {
  agent: string;
  block: WireChatBlock;
  ts: string;
  seq: number;
}

export interface ChatHistoryResponse {
  ok: boolean;
  blocks: WireChatBlock[];
  error?: string;
}

export interface SendResponse {
  ok: boolean;
  error?: string;
}

// ---------------------------------------------------------------------------
// Tasks — mirrors `aciddog::tasks` on the wire.
// ---------------------------------------------------------------------------

export type TaskStatus =
  | "open"
  | "assigned"
  | "in_progress"
  | "done"
  | "cancelled";

export type TodoStatus = "pending" | "in_progress" | "completed";

export interface Note {
  author: string;
  timestamp_secs: number;
  body: string;
}

export interface Todo {
  id: string;
  body: string;
  status: TodoStatus;
  created_by: string;
  created_at_secs: number;
  updated_at_secs: number;
}

export interface Comment {
  id: string;
  author: string;
  timestamp_secs: number;
  body: string;
}

export interface Task {
  id: string;
  title: string;
  description?: string;
  status: TaskStatus;
  assignee?: string;
  created_by: string;
  created_at_secs: number;
  updated_at_secs: number;
  notes: Note[];
  blocked_by: string[];
  todos: Todo[];
  comments: Comment[];
}

export interface TaskListResponse {
  ok: boolean;
  tasks: Task[];
  error?: string;
}

export interface MutationResponse {
  ok: boolean;
  message?: string;
  error?: string;
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

export interface WireInboxMessage {
  from: string;
  body: string;
  /// Unix-epoch seconds with sub-second precision.
  received_at: number;
}

export interface InboxListResponse {
  ok: boolean;
  messages: WireInboxMessage[];
  error?: string;
}

// ---------------------------------------------------------------------------
// Team
// ---------------------------------------------------------------------------

export type UsageProvider =
  | "zai"
  | "chatgpt_oauth"
  | "openrouter"
  | "github_copilot";

export interface WireTeam {
  id: string;
  description: string;
  replace_thinking_messages: boolean;
  default_ai_core?: string;
}

export interface WireFleetCounts {
  agents: number;
  workers: number;
  automatons: number;
  channels: number;
  workflows: number;
}

export interface WireAiCore {
  id: string;
  kind_label: string;
  model?: string;
  credential?: string;
  default_tools?: string[];
  has_system_prompt: boolean;
  usage_provider?: UsageProvider;
  is_team_default: boolean;
  agents_using: string[];
}

export interface WireTeamInfo {
  team: WireTeam;
  ai_cores: WireAiCore[];
  fleet: WireFleetCounts;
}

export interface TeamInfoResponse {
  ok: boolean;
  info?: WireTeamInfo;
  error?: string;
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

export interface WireChannelInfo {
  id: string;
  description?: string;
  welcome?: string;
  members: string[];
  message_count: number;
}

export interface WireChannelMessage {
  sender: string;
  body: string;
  is_welcome: boolean;
}

export interface ChannelListResponse {
  ok: boolean;
  channels: WireChannelInfo[];
  error?: string;
}

export interface ChannelHistoryResponse {
  ok: boolean;
  messages: WireChannelMessage[];
  error?: string;
}

export interface ChannelMessageEvent {
  channel: string;
  sender: string;
  body: string;
  ts: string;
  seq: number;
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

export interface WireBankInfo {
  id: string;
  description: string;
  embed_model?: string;
  entry_count: number;
}

export interface WireHit {
  id: string;
  title: string;
  body: string;
  tags: string[];
  created_at: string;
  distance?: number;
}

export type WireLinkDirection = "out" | "in" | "both";

export interface WireLink {
  peer_id: string;
  peer_title: string;
  peer_body: string;
  peer_tags: string[];
  kind: string;
  direction: WireLinkDirection;
  created_at: string;
}

export interface WireGraphNode {
  id: string;
  title: string;
  tags: string[];
  created_at: string;
}

export interface WireGraphEdge {
  from: string;
  to: string;
  kind: string;
  created_at: string;
}

export interface WireGraph {
  bank: string;
  nodes: WireGraphNode[];
  edges: WireGraphEdge[];
}

export interface BanksResponse {
  ok: boolean;
  banks: WireBankInfo[];
  error?: string;
}

export interface SearchResponse {
  ok: boolean;
  hits: WireHit[];
  error?: string;
}

export interface LinksResponse {
  ok: boolean;
  links: WireLink[];
  error?: string;
}

export interface GraphResponse {
  ok: boolean;
  graph?: WireGraph;
  error?: string;
}

export interface MemoryMutationResponse {
  ok: boolean;
  message?: string;
  error?: string;
  id?: string;
}

// ---------------------------------------------------------------------------
// Workflows — mirrors `aciddog::wire_workflow`.
// ---------------------------------------------------------------------------

export type WorkflowInstanceStatus = "running" | "failed" | "succeeded";
export type WorkflowStepStatus =
  | "pending"
  | "running"
  | "succeeded"
  | "failed"
  | "retrying";

export type WirePropertyKind =
  | { kind: "text"; default?: string }
  | { kind: "number"; default?: number; min?: number; max?: number }
  | { kind: "select"; options: string[]; default?: string }
  | { kind: "multi_select"; options: string[]; default?: string[] };

export interface WireWorkflowProperty {
  id: string;
  label?: string;
  required: boolean;
  kind: WirePropertyKind;
}

export interface WireTemplateStep {
  id: string;
  kind: string;
  /// Retry-target step id — set for `conditional` and `check` steps.
  target?: string;
}

export interface WireWorkflowTemplate {
  id: string;
  description: string;
  max_retries: number;
  properties: WireWorkflowProperty[];
  steps: WireTemplateStep[];
}

export interface WireStepProgress {
  id: string;
  kind: string;
  status: WorkflowStepStatus;
  retry_count: number;
  reason?: string;
}

export interface WireVarSnapshot {
  name: string;
  display: string;
}

export interface WireWorkflowInstance {
  id: number;
  workflow_id: string;
  status: WorkflowInstanceStatus;
  current_step: number;
  failure_reason?: string;
  steps: WireStepProgress[];
  tail_output: string[];
  vars: WireVarSnapshot[];
  remove_in_ms?: number;
}

export type WireStepOutcome =
  | { outcome: "ok" }
  | { outcome: "retry"; reason: string }
  | { outcome: "failed"; reason: string };

/// Discriminated union matching `wire_workflow::WireWorkflowEvent`.
export type WireWorkflowEvent =
  | {
      event: "started";
      instance_id: number;
      workflow_id: string;
      vars: WireVarSnapshot[];
    }
  | { event: "step_started"; instance_id: number; step_idx: number }
  | { event: "step_output"; instance_id: number; line: string }
  | {
      event: "var_changed";
      instance_id: number;
      name: string;
      display: string;
    }
  | {
      event: "step_finished";
      instance_id: number;
      step_idx: number;
      outcome: WireStepOutcome;
    }
  | {
      event: "retrying";
      instance_id: number;
      target_step_idx: number;
      attempt: number;
    }
  | { event: "finished"; instance_id: number; ok: boolean; reason?: string };

export interface WorkflowTemplatesResponse {
  ok: boolean;
  templates: WireWorkflowTemplate[];
  error?: string;
}

export interface WorkflowInstancesResponse {
  ok: boolean;
  instances: WireWorkflowInstance[];
  error?: string;
}

// ---------------------------------------------------------------------------
// Cron — mirrors `aciddog_manager::wire_cron` and the server's Cron routes.
// ---------------------------------------------------------------------------

export type WireCronTz = "utc" | "local";

export type WireCronPayload =
  | { kind: "message"; body: string }
  | { kind: "slash_command"; command: string };

export type WireCronSchedule =
  | { kind: "recurring"; expression: string }
  | { kind: "one_off"; run_at_local: string };

export interface WireCronJob {
  id: string;
  target_agent: string;
  payload: WireCronPayload;
  schedule: WireCronSchedule;
  timezone: WireCronTz;
  catchup_on_startup: boolean;
  enabled: boolean;
  created_by: string;
  created_at: string;
  last_run: string | null;
  next_run: string | null;
}

export type WireCronEvent =
  | { kind: "created"; job: WireCronJob }
  | { kind: "updated"; job: WireCronJob }
  | { kind: "removed"; id: string }
  | { kind: "fired"; id: string; at: string };

export interface CronListResponse {
  ok: boolean;
  jobs: WireCronJob[];
  error?: string;
}

export interface CreateCronRequest {
  target: string;
  /// `"message"` or `"command"`.
  kind: string;
  payload: string;
  schedule: string;
  tz?: string | null;
  oneoff?: boolean;
  catchup_on_startup?: boolean;
  disabled?: boolean;
}

export interface PatchCronRequest {
  target?: string;
  kind?: string;
  payload?: string;
  schedule?: string;
  tz?: string | null;
  oneoff?: boolean;
  catchup_on_startup?: boolean;
  enabled?: boolean;
}
