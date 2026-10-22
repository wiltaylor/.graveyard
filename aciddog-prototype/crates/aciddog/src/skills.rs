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

use base64::Engine;
use thiserror::Error;

use crate::agent_ipc::SkillFile;
use crate::wire_skills::{WireSkillContents, WireSkillFile, WireSkillSummary};

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

/// Walk `<root>/<skill>/SKILL.md` for every entry under `root`, returning
/// one summary per directory. Skips hidden / staging entries (`.tmp-…`).
/// Missing root → empty list (a daemon with no skills configured is
/// valid, not an error).
pub fn read_skills_dir(root: &Path) -> io::Result<Vec<WireSkillSummary>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if !is_valid_skill_name(&name) {
            continue;
        }
        let description = read_skill_description(root, &name).unwrap_or_default();
        out.push(WireSkillSummary { name, description });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Read every regular file under a skill directory (recursively),
/// base64-encoding contents. Symlinks and non-regular entries are
/// skipped — we don't want a writeback to materialise them on a target
/// host.
pub fn read_skill_files(dir: &Path) -> io::Result<Vec<WireSkillFile>> {
    let mut out = Vec::new();
    walk_skill_files(dir, dir, &mut out)?;
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

fn walk_skill_files(root: &Path, dir: &Path, out: &mut Vec<WireSkillFile>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            walk_skill_files(root, &path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let rel = path
            .strip_prefix(root)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        out.push(WireSkillFile {
            rel_path: rel_str,
            contents_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
    }
    Ok(())
}

/// Reject `..`, leading `/`, NUL bytes, empty path, and paths whose
/// canonical form would escape the skill root. Component-based check —
/// no string heuristics.
pub fn validate_skill_rel_path(rel: &str) -> Result<(), &'static str> {
    if rel.is_empty() {
        return Err("empty path");
    }
    if rel.contains('\0') {
        return Err("contains NUL byte");
    }
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err("absolute path");
    }
    for component in path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => return Err("must be a relative path with no `..` or `.` components"),
        }
    }
    Ok(())
}

/// Stage the given files into a sibling temp dir, then `rename(2)` over
/// `<root>/<name>/`. The destination is fully replaced — any prior
/// contents are removed first.
///
/// Caller must have validated `name` with `is_valid_skill_name` and
/// every `rel_path` with `validate_skill_rel_path`.
pub fn write_skill_atomic(name: &str, files: &[SkillFile]) -> io::Result<()> {
    let root = skills_root();
    std::fs::create_dir_all(&root)?;

    let staging = root.join(format!(".tmp-{name}-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    for f in files {
        let dst = staging.join(&f.rel_path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&f.contents_b64)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("base64: {e}")))?;
        std::fs::write(&dst, bytes)?;
    }

    let dest = root.join(name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::rename(&staging, &dest)
}

/// Decode a wire `WireSkillContents` payload back into the
/// `SkillFile` shape `write_skill_atomic` expects. Useful for the
/// import path on the manager.
pub fn skill_files_from_contents(contents: WireSkillContents) -> Vec<SkillFile> {
    contents
        .files
        .into_iter()
        .map(|f| SkillFile {
            rel_path: f.rel_path,
            contents_b64: f.contents_b64,
        })
        .collect()
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
