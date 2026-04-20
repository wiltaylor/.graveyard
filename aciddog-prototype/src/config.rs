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
            })
        })
        .collect::<Result<Vec<_>, ConfigError>>()?;

    Ok(AciddogConfig { team, agents })
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
}
