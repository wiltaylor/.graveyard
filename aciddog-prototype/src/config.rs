use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wcl_lang::{parse, ParseOptions, Value};

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
    pub model: String,
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
    pub model: String,
    pub instructions: String,
    /// Optional per-template tool allowlist. `None` falls back to the
    /// resolved ai_core's own `default_tools`, then the built-in
    /// `DEFAULT_ALLOWED_TOOLS`.
    pub tools: Option<Vec<String>>,
    /// Allowlist of worker template ids an instance of this template
    /// may itself spawn. Same semantics as `Agent::can_create_workers`.
    pub can_create_workers: Option<Vec<String>>,
    /// Optional ai_core id override. `None` falls back to
    /// `Team::default_ai_core`.
    pub ai_core: Option<String>,
    /// Allowlist of memory bank ids this worker template may read from
    /// and write to. Same semantics as `Agent::memory_banks`.
    pub memory_banks: Option<Vec<String>>,
    /// Skills pre-announced to instances of this worker template. Same
    /// semantics as `Agent::skills`.
    pub skills: Option<Vec<String>>,
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

/// Configuration for a named AI runtime. The `kind` field is a
/// tagged-union discriminator — each variant carries its own settings
/// that the runtime reads when constructing a core for an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiCore {
    pub id: String,
    pub kind: AiCoreKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AiCoreKind {
    ClaudeCode {
        /// Content for `--system-prompt-file`. When `None`, the flag is
        /// omitted and claude uses its default system prompt.
        system_prompt: Option<String>,
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
            | AiCoreKind::CopilotCli { default_tools, .. }
            | AiCoreKind::GeminiCli { default_tools, .. } => default_tools.as_deref(),
        }
    }

    /// Model id this core will send to the backend, if the variant
    /// pins one. `ClaudeCode` returns `None` because the model is
    /// chosen by the `claude` CLI itself; `AnthropicApi` returns the
    /// configured `model` field.
    pub fn runtime_model(&self) -> Option<&str> {
        match &self.kind {
            AiCoreKind::ClaudeCode { .. } => None,
            AiCoreKind::AnthropicApi { model, .. } | AiCoreKind::OpenAiApi { model, .. } => {
                Some(model.as_str())
            }
            AiCoreKind::CopilotCli { model, .. } | AiCoreKind::GeminiCli { model, .. } => {
                model.as_deref()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AciddogConfig {
    pub team: Team,
    pub agents: Vec<Agent>,
    pub workers: Vec<Worker>,
    pub ai_cores: Vec<AiCore>,
    /// Named memory banks available to this team. Agents/workers opt in
    /// to specific banks via their `memory_banks` allowlist.
    pub memory_banks: Vec<MemoryBank>,
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
            Ok(Agent {
                id: require_id(&b.id, &b.kind)?,
                role: require_string(b, "role")?,
                role_description: optional_string(b, "role_description"),
                model: optional_string(b, "model").unwrap_or_else(|| "claude-opus-4-7".to_string()),
                instructions: require_string(b, "instructions")?,
                can_message: optional_string_list(b, "can_message")?,
                can_create_workers: optional_string_list(b, "can_create_workers")?,
                tools: optional_string_list(b, "tools")?,
                ai_core: optional_string(b, "ai_core"),
                memory_banks: optional_string_list(b, "memory_banks")?,
                skills: optional_string_list(b, "skills")?,
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    let workers = doc
        .blocks_of_type_resolved("worker")
        .iter()
        .map(|b| {
            Ok(Worker {
                id: require_id(&b.id, &b.kind)?,
                role: require_string(b, "role")?,
                role_description: optional_string(b, "role_description"),
                model: optional_string(b, "model").unwrap_or_else(|| "claude-opus-4-7".to_string()),
                instructions: require_string(b, "instructions")?,
                tools: optional_string_list(b, "tools")?,
                can_create_workers: optional_string_list(b, "can_create_workers")?,
                ai_core: optional_string(b, "ai_core"),
                memory_banks: optional_string_list(b, "memory_banks")?,
                skills: optional_string_list(b, "skills")?,
            })
        })
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
                "copilotCli" => AiCoreKind::CopilotCli {
                    model: optional_string(b, "model"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                "geminiCli" => AiCoreKind::GeminiCli {
                    model: optional_string(b, "model"),
                    default_tools: optional_string_list(b, "default_tools")?,
                },
                other => {
                    return Err(ConfigError::UnknownAiCoreType {
                        core: id,
                        type_name: other.to_string(),
                    })
                }
            };
            Ok(AiCore { id, kind })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    validate_agent_refs(&agents)?;
    validate_worker_refs(&agents, &workers)?;
    validate_ai_core_refs(&team, &agents, &workers, &ai_cores)?;
    validate_ai_core_tools(&ai_cores)?;
    validate_memory_bank_refs(&agents, &workers, &memory_banks)?;
    validate_skill_names(&agents, &workers)?;

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
        ai_cores,
        memory_banks,
        thinking_messages,
    })
}

/// Every id mentioned in a `can_message` list must correspond to a real
/// agent declared in the same config; otherwise the allowlist silently
/// lets nothing through and the typo is hard to diagnose.
fn validate_agent_refs(agents: &[Agent]) -> Result<(), ConfigError> {
    let known: std::collections::HashSet<&str> = agents.iter().map(|a| a.id.as_str()).collect();
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
            AiCoreKind::CopilotCli { default_tools, .. } => {
                ("copilotCli", default_tools.as_deref(), &copilot_known)
            }
            AiCoreKind::GeminiCli { default_tools, .. } => {
                ("geminiCli", default_tools.as_deref(), &gemini_known)
            }
            AiCoreKind::ClaudeCode { .. } => continue,
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
    /// Returns true if agent `from` is permitted to send a message to
    /// agent `to`. Unknown `from` id → false (conservative default).
    pub fn can_send(&self, from: &str, to: &str) -> bool {
        match self
            .agents
            .iter()
            .find(|a| a.id == from)
            .map(|a| a.can_message.as_ref())
        {
            Some(Some(list)) => list.iter().any(|id| id == to),
            Some(None) => true,
            None => false,
        }
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
    fn applies_default_model_when_absent() {
        let cfg = parse_config(MINIMAL, &dummy_path()).unwrap();
        assert_eq!(cfg.agents[0].model, "claude-opus-4-7");
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
        assert_eq!(cfg.agents[0].model, "claude-sonnet-4-6");
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
        assert_eq!(cfg.workers[0].model, "claude-opus-4-7");
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
                default_tools: None,
            }
        );
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
        assert_eq!(skills, &vec!["research".to_string(), "summariser".to_string()]);
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
        assert_eq!(cfg.workers[0].skills.as_deref(), Some(&["research".to_string()][..]));
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
}
