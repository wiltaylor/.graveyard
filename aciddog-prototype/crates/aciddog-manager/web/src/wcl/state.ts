// Typed form state for the team editor. Mirrors the WCL schema in
// wcl/aciddog.wcl and the Rust structs in crates/aciddog/src/config.rs.
// Field names match the WCL keys so serialise/deserialise can stay
// mechanical.

// ---------------------------------------------------------------------
// Team
// ---------------------------------------------------------------------

export interface TeamState {
  id: string;
  description: string;
  replace_thinking_messages?: boolean;
  default_ai_core?: string;
}

// ---------------------------------------------------------------------
// AI core (discriminated union on `type`)
// ---------------------------------------------------------------------

export type AiCoreType =
  | "claudeCode"
  | "anthropicApi"
  | "copilotCli"
  | "geminiCli"
  | "openAiApi"
  | "openAiChatgpt"
  | "jules";

export type UsageProvider =
  | "zai"
  | "chatgpt_oauth"
  | "openrouter"
  | "github_copilot";

interface AiCoreCommon {
  id: string;
  usage_provider?: UsageProvider;
}

export interface AiCoreClaudeCode extends AiCoreCommon {
  type: "claudeCode";
  systemPrompt?: string;
  model?: string;
  default_tools?: string[];
}

export interface AiCoreAnthropicApi extends AiCoreCommon {
  type: "anthropicApi";
  apiUrl: string;
  apiKeyEnv: string;
  model: string;
  systemPrompt?: string;
  default_tools?: string[];
}

export interface AiCoreOpenAiApi extends AiCoreCommon {
  type: "openAiApi";
  apiUrl: string;
  apiKeyEnv: string;
  model: string;
  systemPrompt?: string;
  default_tools?: string[];
}

export interface AiCoreCopilotCli extends AiCoreCommon {
  type: "copilotCli";
  model?: string;
  default_tools?: string[];
}

export interface AiCoreGeminiCli extends AiCoreCommon {
  type: "geminiCli";
  model?: string;
  default_tools?: string[];
}

export interface AiCoreOpenAiChatgpt extends AiCoreCommon {
  type: "openAiChatgpt";
  credential: string;
  model: string;
  systemPrompt?: string;
  default_tools?: string[];
  reasoningEffort?: string;
}

export interface AiCoreJules extends AiCoreCommon {
  type: "jules";
  apiKeyEnv: string;
  defaultBranch?: string;
  pollIntervalSeconds?: number;
  requirePlanApproval?: boolean;
  automationMode?: string;
}

export type AiCoreState =
  | AiCoreClaudeCode
  | AiCoreAnthropicApi
  | AiCoreOpenAiApi
  | AiCoreCopilotCli
  | AiCoreGeminiCli
  | AiCoreOpenAiChatgpt
  | AiCoreJules;

export const AI_CORE_TYPES: AiCoreType[] = [
  "claudeCode",
  "anthropicApi",
  "copilotCli",
  "geminiCli",
  "openAiApi",
  "openAiChatgpt",
  "jules",
];

// ---------------------------------------------------------------------
// Agent / Worker
// ---------------------------------------------------------------------

export type ReloadAction = "ignore" | "compact" | "reset";

export interface AgentState {
  id: string;
  role: string;
  role_description?: string;
  model?: string;
  instructions: string;
  can_message?: string[];
  can_create_workers?: string[];
  can_run_workflows?: string[];
  tools?: string[];
  ai_core?: string;
  memory_banks?: string[];
  skills?: string[];
  compaction?: string;
  supervisor?: boolean;
  sysadmin?: boolean;
  leader?: boolean;
  on_reload?: ReloadAction;
}

export interface WorkerState {
  id: string;
  role: string;
  role_description?: string;
  model?: string;
  instructions: string;
  tools?: string[];
  can_create_workers?: string[];
  can_run_workflows?: string[];
  ai_core?: string;
  memory_banks?: string[];
  skills?: string[];
  compaction?: string;
}

// ---------------------------------------------------------------------
// Automaton (discriminated union on `type`, default `:script`)
// ---------------------------------------------------------------------

export type AutomatonType = "script" | "webservice" | "remote";

interface AutomatonCommon {
  id: string;
  role: string;
  role_description?: string;
  can_message?: string[];
  supervisor?: boolean;
  sysadmin?: boolean;
  leader?: boolean;
}

export interface AutomatonScript extends AutomatonCommon {
  type: "script";
  command: string;
  args?: string[];
  cwd?: string;
  env?: string[];
  timeout_seconds?: number;
}

export interface AutomatonWebservice extends AutomatonCommon {
  type: "webservice";
  bind?: string;
  tls_cert?: string;
  tls_key?: string;
  api_key_env?: string;
  socket_path?: string;
}

export interface AutomatonRemote extends AutomatonCommon {
  type: "remote";
  url?: string;
  api_key_env?: string;
  tls_ca?: string;
  socket_path?: string;
  remote_target: string;
  deliver_to: string;
  proxy_socket_path?: string;
}

export type AutomatonState =
  | AutomatonScript
  | AutomatonWebservice
  | AutomatonRemote;

export const AUTOMATON_TYPES: AutomatonType[] = [
  "script",
  "webservice",
  "remote",
];

// ---------------------------------------------------------------------
// Channel / MemoryBank
// ---------------------------------------------------------------------

export interface ChannelState {
  id: string;
  description?: string;
  welcome?: string;
  members: string[];
}

export interface MemoryBankState {
  id: string;
  description: string;
  embed_model?: string;
}

// ---------------------------------------------------------------------
// Compaction (discriminated union on `strategy`)
// ---------------------------------------------------------------------

export type CompactionStrategy =
  | "harness"
  | "drop"
  | "promptAndDrop"
  | "summarise";

export interface CompactionHarness {
  id: string;
  strategy: "harness";
}

export interface CompactionDrop {
  id: string;
  strategy: "drop";
}

export interface CompactionPromptAndDrop {
  id: string;
  strategy: "promptAndDrop";
  prompt: string;
}

export interface CompactionSummarise {
  id: string;
  strategy: "summarise";
  prompt: string;
  ai_core?: string;
}

export type CompactionState =
  | CompactionHarness
  | CompactionDrop
  | CompactionPromptAndDrop
  | CompactionSummarise;

export const COMPACTION_STRATEGIES: CompactionStrategy[] = [
  "harness",
  "drop",
  "promptAndDrop",
  "summarise",
];

// ---------------------------------------------------------------------
// Webhook
// ---------------------------------------------------------------------

export type WebhookAuthKind =
  | "none"
  | "bearer"
  | "header"
  | "json_field"
  | "hmac_header";

export type WebhookHmacAlgo = "sha1" | "sha256";

export interface WebhookState {
  id: string;
  mount: string;
  path: string;
  deliver_to: string;
  max_body_bytes?: number;
  auth_kind?: WebhookAuthKind;
  auth_header?: string;
  auth_field?: string;
  auth_secret_env?: string;
  auth_algo?: WebhookHmacAlgo;
}

export const WEBHOOK_AUTH_KINDS: WebhookAuthKind[] = [
  "none",
  "bearer",
  "header",
  "json_field",
  "hmac_header",
];

// ---------------------------------------------------------------------
// Workflow (with nested property + step blocks)
// ---------------------------------------------------------------------

export type WorkflowPropertyKind =
  | "text"
  | "number"
  | "select"
  | "multiSelect"
  | "hidden";

export interface WorkflowPropertyState {
  id: string;
  kind: WorkflowPropertyKind;
  label?: string;
  required?: boolean;
  default?: string | number | string[];
  min?: number;
  max?: number;
  options?: string[];
}

export type WorkflowStepKind =
  | "command"
  | "worker"
  | "conditional"
  | "check";

export type WorkflowCheckOp =
  | "equals"
  | "notEquals"
  | "contains"
  | "empty"
  | "notEmpty";

export interface WorkflowStepState {
  id: string;
  kind: WorkflowStepKind;
  command?: string;
  worker?: string;
  prompt?: string;
  cwd?: string;
  variable?: string;
  op?: WorkflowCheckOp;
  value?: string;
  target?: string;
}

export interface WorkflowState {
  id: string;
  description: string;
  max_retries?: number;
  properties: WorkflowPropertyState[];
  steps: WorkflowStepState[];
}

// ---------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------

export interface TeamConfigState {
  team: TeamState;
  ai_cores: AiCoreState[];
  agents: AgentState[];
  workers: WorkerState[];
  automatons: AutomatonState[];
  channels: ChannelState[];
  memory_banks: MemoryBankState[];
  compactions: CompactionState[];
  webhooks: WebhookState[];
  workflows: WorkflowState[];
  thinking_messages?: string[];
}

export function emptyTeamConfig(): TeamConfigState {
  return {
    team: { id: "team", description: "" },
    ai_cores: [],
    agents: [],
    workers: [],
    automatons: [],
    channels: [],
    memory_banks: [],
    compactions: [],
    webhooks: [],
    workflows: [],
  };
}
