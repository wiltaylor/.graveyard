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
