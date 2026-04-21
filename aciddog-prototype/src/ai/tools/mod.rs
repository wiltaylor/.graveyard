//! Native Rust tool implementations for AI cores that speak a wire
//! protocol directly (e.g. the Anthropic Messages API) and therefore
//! must execute tool calls themselves, rather than delegating to a CLI
//! that already knows how to run them.
//!
//! The trait is deliberately minimal so future cores (OpenAI, local
//! inference, etc.) can reuse it as-is. Each tool owns its JSON-Schema
//! literal and an async `invoke` method. Errors surface as
//! `ToolOutput { is_error: true, .. }` so the model can self-correct
//! — `Result::Err` would abort the turn.

pub mod bash;
pub mod docs;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod web_fetch;
pub mod write;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

pub use docs::built_in_tool_doc;

/// A tool the AI can invoke. Implementations must be `Send + Sync` so a
/// single registry can be shared across concurrent turn tasks.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Canonical tool name. Must match the `name` surfaced to the model
    /// in the Anthropic `tools` array and in any `tool_use` block.
    fn name(&self) -> &'static str;

    /// One-line description for the `tools` array.
    fn description(&self) -> &'static str;

    /// JSON-Schema object describing the `input` shape. Returned as
    /// `serde_json::Value` so each tool can embed its schema as a
    /// literal without a builder.
    fn input_schema(&self) -> Value;

    /// Execute the tool. `input` is the raw `tool_use.input` value from
    /// the model. Implementations should return recoverable failures as
    /// `ToolOutput::error(..)` so the model can retry; only registry-
    /// level impossibilities should panic.
    async fn invoke(&self, input: Value) -> ToolOutput;
}

/// Uniform tool-output envelope. `text` becomes the `content` of an
/// Anthropic `tool_result` block; `is_error = true` sets the block's
/// `is_error` field so the model retries.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

/// Set of tool names the native registry knows how to construct. Used
/// by config validation to reject unknown names at parse time.
pub const BUILT_IN_TOOL_NAMES: &[&str] =
    &["Bash", "Read", "Write", "Edit", "Glob", "Grep", "WebFetch"];

/// Name-keyed registry of `Arc<dyn Tool>`. Built once at core
/// construction from the allowed-tools list and cloned cheaply into
/// per-turn dispatch futures.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

/// Per-registry construction knobs. Today only `bash_env` exists (so a
/// core can inject `ACIDDOG_AGENT_ID`/`ACIDDOG_AGENT_SOCKET` into every
/// spawned shell). New fields can be added without breaking callers.
#[derive(Default, Clone)]
pub struct RegistryConfig {
    pub bash_env: Vec<(String, String)>,
}

impl ToolRegistry {
    /// Build a registry containing only the built-in tools whose names
    /// appear in `allowed`. Returns the registry and the list of names
    /// that were successfully resolved (caller uses this to render the
    /// `<tools>` preamble and to diagnose silently-dropped names).
    pub fn from_allowed(allowed: &[String]) -> (Self, Vec<String>) {
        Self::from_allowed_with(allowed, &RegistryConfig::default())
    }

    /// Same as `from_allowed` but lets the caller customise the tools
    /// that need per-core state (today only `Bash`'s env inheritance).
    pub fn from_allowed_with(allowed: &[String], cfg: &RegistryConfig) -> (Self, Vec<String>) {
        let mut tools: BTreeMap<String, Arc<dyn Tool>> = BTreeMap::new();
        let mut resolved: Vec<String> = Vec::new();
        for name in allowed {
            if let Some(tool) = construct_with(name, cfg) {
                tools.insert(name.clone(), tool);
                resolved.push(name.clone());
            }
        }
        (Self { tools }, resolved)
    }

    /// Anthropic-shaped spec array for the `tools` request field:
    /// `[{ "name": ..., "description": ..., "input_schema": {...} }]`.
    pub fn api_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.input_schema(),
                })
            })
            .collect()
    }

    /// OpenAI-shaped spec array for the `tools` request field:
    /// `[{ "type": "function", "function": { "name": .., "description": .., "parameters": .. } }]`.
    /// OpenAI calls the schema field `parameters` (Anthropic calls it
    /// `input_schema`) and wraps each entry in a `type: "function"`
    /// envelope to leave room for future non-function tool kinds.
    pub fn openai_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.input_schema(),
                    },
                })
            })
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(String::as_str).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

fn construct_with(name: &str, cfg: &RegistryConfig) -> Option<Arc<dyn Tool>> {
    match name {
        "Bash" => Some(Arc::new(
            bash::BashTool::default().with_env(cfg.bash_env.clone()),
        )),
        "Read" => Some(Arc::new(read::ReadTool)),
        "Write" => Some(Arc::new(write::WriteTool)),
        "Edit" => Some(Arc::new(edit::EditTool)),
        "Glob" => Some(Arc::new(glob::GlobTool)),
        "Grep" => Some(Arc::new(grep::GrepTool)),
        "WebFetch" => Some(Arc::new(web_fetch::WebFetchTool::default())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_tools_and_skips_unknown() {
        let (reg, resolved) = ToolRegistry::from_allowed(&[
            "Read".to_string(),
            "Nope".to_string(),
            "Bash".to_string(),
        ]);
        assert_eq!(resolved, vec!["Read".to_string(), "Bash".to_string()]);
        assert!(reg.contains("Read"));
        assert!(reg.contains("Bash"));
        assert!(!reg.contains("Nope"));
    }

    #[test]
    fn api_tools_emits_anthropic_shape() {
        let (reg, _) = ToolRegistry::from_allowed(&["Read".to_string()]);
        let tools = reg.api_tools();
        assert_eq!(tools.len(), 1);
        let t = &tools[0];
        assert_eq!(t["name"], "Read");
        assert!(t["description"].is_string());
        assert!(t["input_schema"].is_object());
    }

    #[test]
    fn openai_tools_emits_function_envelope() {
        let (reg, _) = ToolRegistry::from_allowed(&["Read".to_string()]);
        let tools = reg.openai_tools();
        assert_eq!(tools.len(), 1);
        let t = &tools[0];
        assert_eq!(t["type"], "function");
        assert_eq!(t["function"]["name"], "Read");
        assert!(t["function"]["description"].is_string());
        assert!(t["function"]["parameters"].is_object());
        // No top-level name / description / input_schema fields at the
        // outer level — those only appear on Anthropic's shape.
        assert!(t.get("name").is_none());
        assert!(t.get("input_schema").is_none());
    }
}
