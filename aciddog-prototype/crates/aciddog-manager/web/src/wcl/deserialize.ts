// WCL text -> form state. Uses the wcl_wasm `query` function to pull
// out every block of each known kind, then maps the BlockRef
// attributes onto our typed state.
//
// BlockRef shape (from wcl_lang convert.rs):
//   { kind: string, id?: string, attributes?: Record<string, JsValue>, children?: BlockRef[] }
//
// Symbols and strings collapse to JS strings — we dispatch on the
// attribute name to know which is which (mirrors how the Rust parser
// in config.rs treats schema-typed fields).

import { initWcl, queryWcl } from "./index";
import type {
  AgentState,
  AiCoreState,
  AiCoreType,
  AutomatonState,
  AutomatonType,
  ChannelState,
  CompactionState,
  CompactionStrategy,
  MemoryBankState,
  ReloadAction,
  TeamConfigState,
  TeamState,
  UsageProvider,
  WebhookAuthKind,
  WebhookHmacAlgo,
  WebhookState,
  WorkerState,
  WorkflowCheckOp,
  WorkflowPropertyKind,
  WorkflowPropertyState,
  WorkflowState,
  WorkflowStepKind,
  WorkflowStepState,
} from "./state";

interface BlockRef {
  kind: string;
  id?: string;
  attributes?: Record<string, unknown>;
  children?: BlockRef[];
}

async function queryBlocks(source: string, kind: string): Promise<BlockRef[]> {
  const result = await queryWcl(source, kind);
  if (!Array.isArray(result)) return [];
  return result as BlockRef[];
}

// ---------------------------------------------------------------------
// Coercion helpers — every attribute may be missing or a different type
// than expected. Be permissive and return undefined on mismatch.
// ---------------------------------------------------------------------

function asString(v: unknown): string | undefined {
  return typeof v === "string" ? v : undefined;
}

function asNumber(v: unknown): number | undefined {
  return typeof v === "number" ? v : undefined;
}

function asBool(v: unknown): boolean | undefined {
  return typeof v === "boolean" ? v : undefined;
}

function asStringList(v: unknown): string[] | undefined {
  if (!Array.isArray(v)) return undefined;
  const out: string[] = [];
  for (const item of v) {
    if (typeof item !== "string") return undefined;
    out.push(item);
  }
  return out;
}

function attrs(b: BlockRef): Record<string, unknown> {
  return b.attributes ?? {};
}

// ---------------------------------------------------------------------
// Per-entity deserialisers
// ---------------------------------------------------------------------

function deserializeTeam(b: BlockRef): TeamState {
  const a = attrs(b);
  return {
    id: b.id ?? "team",
    description: asString(a.description) ?? "",
    replace_thinking_messages: asBool(a.replace_thinking_messages),
    default_ai_core: asString(a.default_ai_core),
  };
}

function deserializeAiCore(b: BlockRef): AiCoreState | null {
  const a = attrs(b);
  const id = b.id;
  const type = asString(a.type) as AiCoreType | undefined;
  if (!id || !type) return null;
  const usage_provider = asString(a.usage_provider) as UsageProvider | undefined;
  switch (type) {
    case "claudeCode":
      return {
        id,
        type,
        usage_provider,
        systemPrompt: asString(a.systemPrompt),
        model: asString(a.model),
        default_tools: asStringList(a.default_tools),
      };
    case "anthropicApi":
      return {
        id,
        type,
        usage_provider,
        apiUrl: asString(a.apiUrl) ?? "",
        apiKeyEnv: asString(a.apiKeyEnv) ?? "",
        model: asString(a.model) ?? "",
        systemPrompt: asString(a.systemPrompt),
        default_tools: asStringList(a.default_tools),
      };
    case "openAiApi":
      return {
        id,
        type,
        usage_provider,
        apiUrl: asString(a.apiUrl) ?? "",
        apiKeyEnv: asString(a.apiKeyEnv) ?? "",
        model: asString(a.model) ?? "",
        systemPrompt: asString(a.systemPrompt),
        default_tools: asStringList(a.default_tools),
      };
    case "copilotCli":
      return {
        id,
        type,
        usage_provider,
        model: asString(a.model),
        default_tools: asStringList(a.default_tools),
      };
    case "geminiCli":
      return {
        id,
        type,
        usage_provider,
        model: asString(a.model),
        default_tools: asStringList(a.default_tools),
      };
    case "openAiChatgpt":
      return {
        id,
        type,
        usage_provider,
        credential: asString(a.credential) ?? "",
        model: asString(a.model) ?? "",
        systemPrompt: asString(a.systemPrompt),
        default_tools: asStringList(a.default_tools),
        reasoningEffort: asString(a.reasoningEffort),
      };
    case "jules":
      return {
        id,
        type,
        usage_provider,
        apiKeyEnv: asString(a.apiKeyEnv) ?? "",
        defaultBranch: asString(a.defaultBranch),
        pollIntervalSeconds: asNumber(a.pollIntervalSeconds),
        requirePlanApproval: asBool(a.requirePlanApproval),
        automationMode: asString(a.automationMode),
      };
  }
  return null;
}

function deserializeAgent(b: BlockRef): AgentState | null {
  const a = attrs(b);
  if (!b.id) return null;
  return {
    id: b.id,
    role: asString(a.role) ?? "",
    role_description: asString(a.role_description),
    model: asString(a.model),
    instructions: asString(a.instructions) ?? "",
    can_message: asStringList(a.can_message),
    can_create_workers: asStringList(a.can_create_workers),
    can_run_workflows: asStringList(a.can_run_workflows),
    tools: asStringList(a.tools),
    ai_core: asString(a.ai_core),
    memory_banks: asStringList(a.memory_banks),
    skills: asStringList(a.skills),
    compaction: asString(a.compaction),
    supervisor: asBool(a.supervisor),
    sysadmin: asBool(a.sysadmin),
    on_reload: asString(a.on_reload) as ReloadAction | undefined,
  };
}

function deserializeWorker(b: BlockRef): WorkerState | null {
  const a = attrs(b);
  if (!b.id) return null;
  return {
    id: b.id,
    role: asString(a.role) ?? "",
    role_description: asString(a.role_description),
    model: asString(a.model),
    instructions: asString(a.instructions) ?? "",
    tools: asStringList(a.tools),
    can_create_workers: asStringList(a.can_create_workers),
    can_run_workflows: asStringList(a.can_run_workflows),
    ai_core: asString(a.ai_core),
    memory_banks: asStringList(a.memory_banks),
    skills: asStringList(a.skills),
    compaction: asString(a.compaction),
  };
}

function deserializeAutomaton(b: BlockRef): AutomatonState | null {
  const a = attrs(b);
  if (!b.id) return null;
  // `:script` is the parser default when `type` is omitted.
  const type = (asString(a.type) as AutomatonType | undefined) ?? "script";
  const common = {
    id: b.id,
    role: asString(a.role) ?? "",
    role_description: asString(a.role_description),
    can_message: asStringList(a.can_message),
    supervisor: asBool(a.supervisor),
    sysadmin: asBool(a.sysadmin),
  };
  switch (type) {
    case "script":
      return {
        ...common,
        type,
        command: asString(a.command) ?? "",
        args: asStringList(a.args),
        cwd: asString(a.cwd),
        env: asStringList(a.env),
        timeout_seconds: asNumber(a.timeout_seconds),
      };
    case "webservice":
      return {
        ...common,
        type,
        bind: asString(a.bind),
        tls_cert: asString(a.tls_cert),
        tls_key: asString(a.tls_key),
        api_key_env: asString(a.api_key_env),
        socket_path: asString(a.socket_path),
      };
    case "remote":
      return {
        ...common,
        type,
        url: asString(a.url),
        api_key_env: asString(a.api_key_env),
        tls_ca: asString(a.tls_ca),
        socket_path: asString(a.socket_path),
        remote_target: asString(a.remote_target) ?? "",
        deliver_to: asString(a.deliver_to) ?? "",
        proxy_socket_path: asString(a.proxy_socket_path),
      };
  }
  return null;
}

function deserializeChannel(b: BlockRef): ChannelState | null {
  const a = attrs(b);
  if (!b.id) return null;
  return {
    id: b.id,
    description: asString(a.description),
    welcome: asString(a.welcome),
    members: asStringList(a.members) ?? [],
  };
}

function deserializeMemoryBank(b: BlockRef): MemoryBankState | null {
  const a = attrs(b);
  if (!b.id) return null;
  return {
    id: b.id,
    description: asString(a.description) ?? "",
    embed_model: asString(a.embed_model),
  };
}

function deserializeCompaction(b: BlockRef): CompactionState | null {
  const a = attrs(b);
  if (!b.id) return null;
  const strategy = asString(a.strategy) as CompactionStrategy | undefined;
  if (!strategy) return null;
  switch (strategy) {
    case "harness":
    case "drop":
      return { id: b.id, strategy };
    case "promptAndDrop":
      return { id: b.id, strategy, prompt: asString(a.prompt) ?? "" };
    case "summarise":
      return {
        id: b.id,
        strategy,
        prompt: asString(a.prompt) ?? "",
        ai_core: asString(a.ai_core),
      };
  }
  return null;
}

function deserializeWebhook(b: BlockRef): WebhookState | null {
  const a = attrs(b);
  if (!b.id) return null;
  return {
    id: b.id,
    mount: asString(a.mount) ?? "",
    path: asString(a.path) ?? "",
    deliver_to: asString(a.deliver_to) ?? "",
    max_body_bytes: asNumber(a.max_body_bytes),
    auth_kind: asString(a.auth_kind) as WebhookAuthKind | undefined,
    auth_header: asString(a.auth_header),
    auth_field: asString(a.auth_field),
    auth_secret_env: asString(a.auth_secret_env),
    auth_algo: asString(a.auth_algo) as WebhookHmacAlgo | undefined,
  };
}

function deserializeWorkflowProperty(b: BlockRef): WorkflowPropertyState | null {
  const a = attrs(b);
  if (!b.id) return null;
  const kind = asString(a.kind) as WorkflowPropertyKind | undefined;
  if (!kind) return null;
  let def: string | number | string[] | undefined;
  if (Array.isArray(a.default)) {
    def = asStringList(a.default);
  } else if (typeof a.default === "number") {
    def = a.default;
  } else if (typeof a.default === "string") {
    def = a.default;
  }
  return {
    id: b.id,
    kind,
    label: asString(a.label),
    required: asBool(a.required),
    default: def,
    min: asNumber(a.min),
    max: asNumber(a.max),
    options: asStringList(a.options),
  };
}

function deserializeWorkflowStep(b: BlockRef): WorkflowStepState | null {
  const a = attrs(b);
  if (!b.id) return null;
  const kind = asString(a.kind) as WorkflowStepKind | undefined;
  if (!kind) return null;
  return {
    id: b.id,
    kind,
    command: asString(a.command),
    worker: asString(a.worker),
    prompt: asString(a.prompt),
    cwd: asString(a.cwd),
    variable: asString(a.variable),
    op: asString(a.op) as WorkflowCheckOp | undefined,
    value: asString(a.value),
    target: asString(a.target),
  };
}

function deserializeWorkflow(b: BlockRef): WorkflowState | null {
  const a = attrs(b);
  if (!b.id) return null;
  const properties: WorkflowPropertyState[] = [];
  const steps: WorkflowStepState[] = [];
  for (const child of b.children ?? []) {
    if (child.kind === "property") {
      const p = deserializeWorkflowProperty(child);
      if (p) properties.push(p);
    } else if (child.kind === "step") {
      const s = deserializeWorkflowStep(child);
      if (s) steps.push(s);
    }
  }
  return {
    id: b.id,
    description: asString(a.description) ?? "",
    max_retries: asNumber(a.max_retries),
    properties,
    steps,
  };
}

// ---------------------------------------------------------------------
// Top-level
// ---------------------------------------------------------------------

export async function deserializeTeamConfig(
  source: string,
): Promise<TeamConfigState> {
  await initWcl();
  const teams = await queryBlocks(source, "team");
  const team = teams[0]
    ? deserializeTeam(teams[0])
    : { id: "team", description: "" };

  const ai_cores: AiCoreState[] = [];
  for (const b of await queryBlocks(source, "ai_core")) {
    const c = deserializeAiCore(b);
    if (c) ai_cores.push(c);
  }

  const agents: AgentState[] = [];
  for (const b of await queryBlocks(source, "agent")) {
    const a = deserializeAgent(b);
    if (a) agents.push(a);
  }

  const workers: WorkerState[] = [];
  for (const b of await queryBlocks(source, "worker")) {
    const w = deserializeWorker(b);
    if (w) workers.push(w);
  }

  const automatons: AutomatonState[] = [];
  for (const b of await queryBlocks(source, "automaton")) {
    const a = deserializeAutomaton(b);
    if (a) automatons.push(a);
  }

  const channels: ChannelState[] = [];
  for (const b of await queryBlocks(source, "channel")) {
    const c = deserializeChannel(b);
    if (c) channels.push(c);
  }

  const memory_banks: MemoryBankState[] = [];
  for (const b of await queryBlocks(source, "memory_bank")) {
    const m = deserializeMemoryBank(b);
    if (m) memory_banks.push(m);
  }

  const compactions: CompactionState[] = [];
  for (const b of await queryBlocks(source, "compaction")) {
    const c = deserializeCompaction(b);
    if (c) compactions.push(c);
  }

  const webhooks: WebhookState[] = [];
  for (const b of await queryBlocks(source, "webhook")) {
    const w = deserializeWebhook(b);
    if (w) webhooks.push(w);
  }

  const workflows: WorkflowState[] = [];
  for (const b of await queryBlocks(source, "workflow")) {
    const w = deserializeWorkflow(b);
    if (w) workflows.push(w);
  }

  let thinking_messages: string[] | undefined;
  const tm = await queryBlocks(source, "thinking_messages");
  if (tm[0]) {
    const list = asStringList(attrs(tm[0]).messages);
    if (list && list.length > 0) thinking_messages = list;
  }

  return {
    team,
    ai_cores,
    agents,
    workers,
    automatons,
    channels,
    memory_banks,
    compactions,
    webhooks,
    workflows,
    thinking_messages,
  };
}
