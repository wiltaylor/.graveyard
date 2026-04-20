use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wcl_lang::{parse, ParseOptions, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Team {
    pub id: String,
    pub description: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AciddogConfig {
    pub team: Team,
    pub agents: Vec<Agent>,
}

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
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    validate_agent_refs(&agents)?;

    Ok(AciddogConfig { team, agents })
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
}
