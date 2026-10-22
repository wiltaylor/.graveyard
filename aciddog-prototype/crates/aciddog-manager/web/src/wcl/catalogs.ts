// Static catalogs mirrored from the Rust crates.
//
// Sources:
//   - Built-in tools:           crates/aciddog/src/ai/tools/mod.rs:83-92
//   - Copilot/Gemini tools:     crates/aciddog/src/ai/{copilot_cli,gemini_cli}.rs
//   - Embed models (fastembed): crates/aciddog/src/memory/embedder.rs:100-116
//   - Usage providers:          crates/aciddog/src/config.rs:399-415
//
// Keep these in sync if the Rust enums grow.

import type { AiCoreType, UsageProvider } from "./state";

const FULL_TOOL_SET: readonly string[] = [
  "Bash",
  "Read",
  "Write",
  "Edit",
  "Glob",
  "Grep",
  "WebFetch",
  "ManagerSkillRequest",
];

// CopilotCli + GeminiCli only allow the subset that the binaries expose
// — no WebFetch, no ManagerSkillRequest. ClaudeCode + Jules don't
// validate against the catalog (tools are decided by the underlying
// CLI), so we offer the full set as a convenience.
const CLI_TOOL_SUBSET: readonly string[] = [
  "Bash",
  "Read",
  "Write",
  "Edit",
  "Glob",
  "Grep",
];

export function toolsForCoreType(type: AiCoreType): readonly string[] {
  if (type === "copilotCli" || type === "geminiCli") return CLI_TOOL_SUBSET;
  return FULL_TOOL_SET;
}

export const ALL_TOOLS = FULL_TOOL_SET;

export const USAGE_PROVIDERS: readonly UsageProvider[] = [
  "zai",
  "chatgpt_oauth",
  "openrouter",
  "github_copilot",
];

export const EMBED_MODELS: readonly string[] = [
  "BGE-small-en-v1.5",
  "BGE-small-en-v1.5-q",
  "BGE-base-en-v1.5",
  "BGE-base-en-v1.5-q",
  "BGE-large-en-v1.5",
  "all-minilm-l6-v2",
  "all-minilm-l12-v2",
  "multilingual-e5-small",
  "multilingual-e5-base",
  "nomic-embed-text-v1.5",
];
