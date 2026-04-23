//! Project-local skill loading.
//!
//! Skills live on disk under `.aciddog/skills/<name>/SKILL.md`. Aciddog
//! never parses or executes skill logic itself — it only surfaces the
//! existence of configured skills in each actor's first-turn prompt so
//! the model can choose to read `SKILL.md` and follow it.
//!
//! This module owns two responsibilities:
//!   1. Validate that a skill id is safe to join into a filesystem path.
//!   2. Read `SKILL.md` for a named skill and extract a one-line
//!      description for the prompt listing.

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Relative path to the skills directory used by all callers. Kept
/// cwd-relative to mirror `.aciddog/tasks.json` and `.aciddog/memory/`.
pub fn skills_root() -> PathBuf {
    PathBuf::from(".aciddog").join("skills")
}

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill name `{0}` is not a valid identifier")]
    InvalidName(String),
    #[error("SKILL.md for skill `{name}` is missing")]
    NotFound { name: String },
    #[error("SKILL.md for skill `{name}` has no description line")]
    EmptyDescription { name: String },
    #[error("reading SKILL.md for skill `{name}`: {source}")]
    Io {
        name: String,
        #[source]
        source: io::Error,
    },
}

/// Accept only lowercase alnum + `-` / `_`, leading alnum, max 64 chars.
/// Matches typical directory-id conventions used elsewhere in the repo
/// (agent / worker / memory bank ids) and blocks path traversal like
/// `..` or names with `/`.
pub fn is_valid_skill_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Read the description for a skill from its `SKILL.md`. Never joins an
/// unvalidated name into the filesystem path.
pub fn read_skill_description(root: &Path, name: &str) -> Result<String, SkillError> {
    if !is_valid_skill_name(name) {
        return Err(SkillError::InvalidName(name.to_string()));
    }
    let path = root.join(name).join("SKILL.md");
    let body = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(SkillError::NotFound {
                name: name.to_string(),
            });
        }
        Err(e) => {
            return Err(SkillError::Io {
                name: name.to_string(),
                source: e,
            });
        }
    };
    parse_description_from_markdown(&body).ok_or_else(|| SkillError::EmptyDescription {
        name: name.to_string(),
    })
}

/// Pull a short description out of a SKILL.md.
///
/// Preference order:
///   1. If the file opens with a YAML frontmatter block (a `---` line
///      followed by more `---`-terminated content), find a
///      `description: <value>` line inside it.
///   2. Otherwise, return the first non-blank, non-heading line of the
///      body.
///
/// Results are trimmed and capped at 200 chars so a single runaway
/// paragraph cannot blow up the prompt.
fn parse_description_from_markdown(src: &str) -> Option<String> {
    let mut lines = src.lines().peekable();

    while matches!(lines.peek(), Some(l) if l.trim().is_empty()) {
        lines.next();
    }

    if matches!(lines.peek(), Some(l) if l.trim() == "---") {
        lines.next();
        for fm_line in lines.by_ref() {
            if fm_line.trim() == "---" {
                break;
            }
            if let Some(desc) = extract_frontmatter_description(fm_line) {
                return Some(truncate(&desc));
            }
        }
    }

    for line in lines {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        return Some(truncate(t));
    }
    None
}

fn extract_frontmatter_description(line: &str) -> Option<String> {
    let (key, value) = line.split_once(':')?;
    if !key.trim().eq_ignore_ascii_case("description") {
        return None;
    }
    let trimmed = value.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or(trimmed);
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

fn truncate(s: &str) -> String {
    const MAX: usize = 200;
    let t = s.trim();
    if t.chars().count() <= MAX {
        return t.to_string();
    }
    let cut: String = t.chars().take(MAX).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(is_valid_skill_name("research"));
        assert!(is_valid_skill_name("r"));
        assert!(is_valid_skill_name("a_b-c9"));
        assert!(is_valid_skill_name("0leading-digit-ok"));
    }

    #[test]
    fn invalid_names() {
        assert!(!is_valid_skill_name(""));
        assert!(!is_valid_skill_name("Has-Upper"));
        assert!(!is_valid_skill_name("bad name"));
        assert!(!is_valid_skill_name("slash/path"));
        assert!(!is_valid_skill_name(".."));
        assert!(!is_valid_skill_name("-leads-with-dash"));
        assert!(!is_valid_skill_name(&"x".repeat(65)));
    }

    #[test]
    fn description_from_frontmatter() {
        let src =
            "---\ndescription: Quick literature lookups\nother: value\n---\n\n# body\nbody line\n";
        assert_eq!(
            parse_description_from_markdown(src).as_deref(),
            Some("Quick literature lookups")
        );
    }

    #[test]
    fn description_from_quoted_frontmatter() {
        let src = "---\ndescription: \"Quoted value\"\n---\n";
        assert_eq!(
            parse_description_from_markdown(src).as_deref(),
            Some("Quoted value")
        );
    }

    #[test]
    fn description_from_body_when_no_frontmatter() {
        let src = "# Title\n\nFirst real paragraph.\n\nSecond.\n";
        assert_eq!(
            parse_description_from_markdown(src).as_deref(),
            Some("First real paragraph.")
        );
    }

    #[test]
    fn description_from_body_skipping_blank_lines() {
        let src = "\n\nHello.\n";
        assert_eq!(
            parse_description_from_markdown(src).as_deref(),
            Some("Hello.")
        );
    }

    #[test]
    fn description_empty_body_returns_none() {
        assert_eq!(parse_description_from_markdown("").as_deref(), None);
        assert_eq!(
            parse_description_from_markdown("# only heading\n").as_deref(),
            None
        );
    }

    #[test]
    fn description_frontmatter_without_description_falls_through_to_body() {
        let src = "---\nname: foo\n---\n\nFallback line.\n";
        assert_eq!(
            parse_description_from_markdown(src).as_deref(),
            Some("Fallback line.")
        );
    }

    #[test]
    fn read_skill_description_rejects_invalid_name() {
        let err = read_skill_description(Path::new("."), "../etc").unwrap_err();
        assert!(matches!(err, SkillError::InvalidName(_)));
    }
}
