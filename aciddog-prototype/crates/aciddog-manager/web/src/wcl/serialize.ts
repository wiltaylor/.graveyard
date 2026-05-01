// Form state -> WCL text. The output is canonical (no comments, no
// alignment): each block is `<kind> <id> { ... }` with one field per
// line, two-space indent, fields in declaration order. Optional fields
// are omitted when undefined; empty arrays are omitted too.

import type {
  AgentState,
  AiCoreState,
  AutomatonState,
  ChannelState,
  CompactionState,
  MemoryBankState,
  TeamConfigState,
  TeamState,
  WebhookState,
  WorkerState,
  WorkflowPropertyState,
  WorkflowState,
  WorkflowStepState,
} from "./state";

// ---------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------

const INDENT = "  ";

/** Escape a string for the standard `"..."` literal. */
function escapeString(value: string): string {
  return value
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r")
    .replace(/\t/g, "\\t");
}

/** Render a string. Multi-line strings use heredoc syntax. */
function quoteString(value: string): string {
  if (value.includes("\n")) {
    // Heredoc: indent each line by two spaces (relative to the field
    // line) and terminate with `EOT` on its own line at the same
    // indent. WCL strips the common leading whitespace.
    const indented = value
      .split("\n")
      .map((line) => `${INDENT}${INDENT}${line}`)
      .join("\n");
    return `<<EOT\n${indented}\n${INDENT}EOT`;
  }
  return `"${escapeString(value)}"`;
}

/** Render a list of strings as `["a", "b"]`. */
function quoteList(values: string[]): string {
  return `[${values.map((v) => `"${escapeString(v)}"`).join(", ")}]`;
}

/** WCL symbol literal — `:foo`. */
function symbol(value: string): string {
  return `:${value}`;
}

// ---------------------------------------------------------------------
// Field emission helpers
// ---------------------------------------------------------------------

class BlockBuilder {
  private lines: string[] = [];

  string(name: string, value: string | undefined): this {
    if (value === undefined) return this;
    return this.push(name, quoteString(value));
  }

  /** Required string — emits even if empty. */
  requiredString(name: string, value: string): this {
    return this.push(name, quoteString(value));
  }

  number(name: string, value: number | undefined): this {
    if (value === undefined) return this;
    return this.push(name, String(value));
  }

  bool(name: string, value: boolean | undefined): this {
    if (value === undefined) return this;
    return this.push(name, value ? "true" : "false");
  }

  symbol(name: string, value: string | undefined): this {
    if (value === undefined) return this;
    return this.push(name, symbol(value));
  }

  list(name: string, value: string[] | undefined): this {
    if (value === undefined || value.length === 0) return this;
    return this.push(name, quoteList(value));
  }

  /** Raw value (already-formatted WCL on the right of `=`). */
  raw(name: string, value: string | undefined): this {
    if (value === undefined) return this;
    return this.push(name, value);
  }

  /** Append a nested block already serialised as a multi-line string. */
  block(text: string): this {
    this.lines.push(text);
    return this;
  }

  build(kind: string, id: string): string {
    const body = this.lines.map((l) => `${INDENT}${l}`).join("\n");
    if (body.length === 0) return `${kind} ${id} {\n}`;
    return `${kind} ${id} {\n${body}\n}`;
  }

  /** Build a block with no `id` slot (e.g. `thinking_messages`). */
  buildNamedOnly(kind: string): string {
    const body = this.lines.map((l) => `${INDENT}${l}`).join("\n");
    if (body.length === 0) return `${kind} {\n}`;
    return `${kind} {\n${body}\n}`;
  }

  private push(name: string, rendered: string): this {
    this.lines.push(`${name} = ${rendered}`);
    return this;
  }
}

// ---------------------------------------------------------------------
// Per-entity serialisers
// ---------------------------------------------------------------------

export function serializeTeam(t: TeamState): string {
  return new BlockBuilder()
    .requiredString("description", t.description)
    .bool("replace_thinking_messages", t.replace_thinking_messages)
    .string("default_ai_core", t.default_ai_core)
    .build("team", t.id);
}

export function serializeAiCore(c: AiCoreState): string {
  const b = new BlockBuilder().symbol("type", c.type);
  if (c.usage_provider) b.symbol("usage_provider", c.usage_provider);
  switch (c.type) {
    case "claudeCode":
      b.string("systemPrompt", c.systemPrompt)
        .string("model", c.model)
        .list("default_tools", c.default_tools);
      break;
    case "anthropicApi":
      b.requiredString("apiUrl", c.apiUrl)
        .requiredString("apiKeyEnv", c.apiKeyEnv)
        .requiredString("model", c.model)
        .string("systemPrompt", c.systemPrompt)
        .list("default_tools", c.default_tools);
      break;
    case "openAiApi":
      b.requiredString("apiUrl", c.apiUrl)
        .requiredString("apiKeyEnv", c.apiKeyEnv)
        .requiredString("model", c.model)
        .string("systemPrompt", c.systemPrompt)
        .list("default_tools", c.default_tools);
      break;
    case "copilotCli":
      b.string("model", c.model).list("default_tools", c.default_tools);
      break;
    case "geminiCli":
      b.string("model", c.model).list("default_tools", c.default_tools);
      break;
    case "openAiChatgpt":
      b.requiredString("credential", c.credential)
        .requiredString("model", c.model)
        .string("systemPrompt", c.systemPrompt)
        .list("default_tools", c.default_tools)
        .string("reasoningEffort", c.reasoningEffort);
      break;
    case "jules":
      b.requiredString("apiKeyEnv", c.apiKeyEnv)
        .string("defaultBranch", c.defaultBranch)
        .number("pollIntervalSeconds", c.pollIntervalSeconds)
        .bool("requirePlanApproval", c.requirePlanApproval)
        .string("automationMode", c.automationMode);
      break;
  }
  return b.build("ai_core", c.id);
}

export function serializeAgent(a: AgentState): string {
  const b = new BlockBuilder().requiredString("role", a.role);
  if (a.role_description !== undefined) {
    b.requiredString("role_description", a.role_description);
  }
  b.string("model", a.model)
    .requiredString("instructions", a.instructions)
    .list("can_message", a.can_message)
    .list("can_create_workers", a.can_create_workers)
    .list("can_run_workflows", a.can_run_workflows)
    .list("tools", a.tools)
    .string("ai_core", a.ai_core)
    .list("memory_banks", a.memory_banks)
    .list("skills", a.skills)
    .string("compaction", a.compaction)
    .bool("supervisor", a.supervisor)
    .bool("sysadmin", a.sysadmin)
    .bool("leader", a.leader)
    .string("group", a.group)
    .symbol("on_reload", a.on_reload);
  return b.build("agent", a.id);
}

export function serializeWorker(w: WorkerState): string {
  const b = new BlockBuilder().requiredString("role", w.role);
  if (w.role_description !== undefined) {
    b.requiredString("role_description", w.role_description);
  }
  b.string("model", w.model)
    .requiredString("instructions", w.instructions)
    .list("tools", w.tools)
    .list("can_create_workers", w.can_create_workers)
    .list("can_run_workflows", w.can_run_workflows)
    .string("ai_core", w.ai_core)
    .list("memory_banks", w.memory_banks)
    .list("skills", w.skills)
    .string("compaction", w.compaction)
    .string("group", w.group);
  return b.build("worker", w.id);
}

export function serializeAutomaton(a: AutomatonState): string {
  const b = new BlockBuilder().requiredString("role", a.role);
  if (a.role_description !== undefined) {
    b.requiredString("role_description", a.role_description);
  }
  // `:script` is the parser default — emit anyway for clarity.
  b.symbol("type", a.type);
  switch (a.type) {
    case "script":
      b.requiredString("command", a.command)
        .list("args", a.args)
        .string("cwd", a.cwd)
        .list("env", a.env)
        .number("timeout_seconds", a.timeout_seconds);
      break;
    case "webservice":
      b.string("bind", a.bind)
        .string("tls_cert", a.tls_cert)
        .string("tls_key", a.tls_key)
        .string("api_key_env", a.api_key_env)
        .string("socket_path", a.socket_path);
      break;
    case "remote":
      b.string("url", a.url)
        .string("api_key_env", a.api_key_env)
        .string("tls_ca", a.tls_ca)
        .string("socket_path", a.socket_path)
        .requiredString("remote_target", a.remote_target)
        .requiredString("deliver_to", a.deliver_to)
        .string("proxy_socket_path", a.proxy_socket_path);
      break;
  }
  b.list("can_message", a.can_message)
    .bool("supervisor", a.supervisor)
    .bool("sysadmin", a.sysadmin)
    .bool("leader", a.leader)
    .string("group", a.group);
  return b.build("automaton", a.id);
}

export function serializeChannel(c: ChannelState): string {
  return new BlockBuilder()
    .string("description", c.description)
    .string("welcome", c.welcome)
    .list("members", c.members)
    .build("channel", c.id);
}

export function serializeMemoryBank(m: MemoryBankState): string {
  return new BlockBuilder()
    .requiredString("description", m.description)
    .string("embed_model", m.embed_model)
    .build("memory_bank", m.id);
}

export function serializeCompaction(c: CompactionState): string {
  const b = new BlockBuilder().symbol("strategy", c.strategy);
  switch (c.strategy) {
    case "harness":
    case "drop":
      break;
    case "promptAndDrop":
      b.requiredString("prompt", c.prompt);
      break;
    case "summarise":
      b.requiredString("prompt", c.prompt).string("ai_core", c.ai_core);
      break;
  }
  return b.build("compaction", c.id);
}

export function serializeWebhook(w: WebhookState): string {
  return new BlockBuilder()
    .requiredString("mount", w.mount)
    .requiredString("path", w.path)
    .requiredString("deliver_to", w.deliver_to)
    .number("max_body_bytes", w.max_body_bytes)
    .symbol("auth_kind", w.auth_kind)
    .string("auth_header", w.auth_header)
    .string("auth_field", w.auth_field)
    .string("auth_secret_env", w.auth_secret_env)
    .symbol("auth_algo", w.auth_algo)
    .build("webhook", w.id);
}

function serializeWorkflowProperty(p: WorkflowPropertyState): string {
  const b = new BlockBuilder()
    .symbol("kind", p.kind)
    .string("label", p.label)
    .bool("required", p.required);
  if (p.default !== undefined) {
    if (Array.isArray(p.default)) {
      b.list("default", p.default);
    } else if (typeof p.default === "number") {
      b.number("default", p.default);
    } else {
      b.string("default", p.default);
    }
  }
  b.number("min", p.min).number("max", p.max).list("options", p.options);
  return b.build("property", p.id);
}

function serializeWorkflowStep(s: WorkflowStepState): string {
  const b = new BlockBuilder().symbol("kind", s.kind);
  if (s.command !== undefined) b.requiredString("command", s.command);
  if (s.worker !== undefined) b.requiredString("worker", s.worker);
  if (s.prompt !== undefined) b.requiredString("prompt", s.prompt);
  b.string("cwd", s.cwd)
    .string("variable", s.variable)
    .symbol("op", s.op)
    .string("value", s.value)
    .string("target", s.target);
  return b.build("step", s.id);
}

export function serializeWorkflow(w: WorkflowState): string {
  // Workflows nest `property` and `step` blocks. Build the body
  // ourselves (BlockBuilder.block helpers don't quite cover the
  // mix-and-indent case for nested blocks).
  const lines: string[] = [];
  lines.push(`${INDENT}description = ${quoteString(w.description)}`);
  if (w.max_retries !== undefined) {
    lines.push(`${INDENT}max_retries = ${w.max_retries}`);
  }
  for (const p of w.properties) {
    lines.push("");
    lines.push(indentBlock(serializeWorkflowProperty(p), INDENT));
  }
  for (const s of w.steps) {
    lines.push("");
    lines.push(indentBlock(serializeWorkflowStep(s), INDENT));
  }
  return `workflow ${w.id} {\n${lines.join("\n")}\n}`;
}

function indentBlock(block: string, indent: string): string {
  return block
    .split("\n")
    .map((line) => `${indent}${line}`)
    .join("\n");
}

function serializeThinkingMessages(messages: string[]): string {
  return new BlockBuilder()
    .list("messages", messages)
    .buildNamedOnly("thinking_messages");
}

// ---------------------------------------------------------------------
// Top-level
// ---------------------------------------------------------------------

export function serializeTeamConfig(state: TeamConfigState): string {
  const blocks: string[] = [];
  for (const c of state.ai_cores) blocks.push(serializeAiCore(c));
  blocks.push(serializeTeam(state.team));
  for (const m of state.memory_banks) blocks.push(serializeMemoryBank(m));
  for (const c of state.compactions) blocks.push(serializeCompaction(c));
  for (const a of state.agents) blocks.push(serializeAgent(a));
  for (const w of state.workers) blocks.push(serializeWorker(w));
  for (const a of state.automatons) blocks.push(serializeAutomaton(a));
  for (const w of state.webhooks) blocks.push(serializeWebhook(w));
  for (const c of state.channels) blocks.push(serializeChannel(c));
  for (const w of state.workflows) blocks.push(serializeWorkflow(w));
  if (state.thinking_messages && state.thinking_messages.length > 0) {
    blocks.push(serializeThinkingMessages(state.thinking_messages));
  }
  return blocks.join("\n\n") + "\n";
}
