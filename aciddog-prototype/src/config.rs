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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub role: String,
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
}

/// A template for an ephemeral worker. Unlike `Agent`, a `Worker` is not
/// spawned at startup — it is instantiated on demand when an agent (or
/// another worker whose template opts in) calls `aciddog worker create`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Worker {
    pub id: String,
    pub role: String,
    pub model: String,
    pub instructions: String,
    /// Optional per-template tool allowlist. `None` falls back to the
    /// default tool list used by configured agents.
    pub tools: Option<Vec<String>>,
    /// Allowlist of worker template ids an instance of this template
    /// may itself spawn. Same semantics as `Agent::can_create_workers`.
    pub can_create_workers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AciddogConfig {
    pub team: Team,
    pub agents: Vec<Agent>,
    pub workers: Vec<Worker>,
    /// Final pool of messages shown by the thinking indicator while an
    /// agent is `Working`. Already merged with built-ins per
    /// `team.replace_thinking_messages`; guaranteed non-empty.
    pub thinking_messages: Vec<String>,
}

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
    };

    let agents = doc
        .blocks_of_type_resolved("agent")
        .iter()
        .map(|b| {
            Ok(Agent {
                id: require_id(&b.id, &b.kind)?,
                role: require_string(b, "role")?,
                model: optional_string(b, "model").unwrap_or_else(|| "claude-opus-4-7".to_string()),
                instructions: require_string(b, "instructions")?,
                can_message: optional_string_list(b, "can_message")?,
                can_create_workers: optional_string_list(b, "can_create_workers")?,
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
                model: optional_string(b, "model").unwrap_or_else(|| "claude-opus-4-7".to_string()),
                instructions: require_string(b, "instructions")?,
                tools: optional_string_list(b, "tools")?,
                can_create_workers: optional_string_list(b, "can_create_workers")?,
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    validate_agent_refs(&agents)?;
    validate_worker_refs(&agents, &workers)?;

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
        assert_eq!(cfg.thinking_messages.len(), BUILT_IN_THINKING_MESSAGES.len());
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
        assert_eq!(cfg.thinking_messages.len(), BUILT_IN_THINKING_MESSAGES.len());
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
}
