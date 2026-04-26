//! Wire types for the sysadmin skill RPCs (`SkillsList`, `SkillRead`).
//!
//! Distinct from the agent-scoped `SkillList` / `SkillRun` commands —
//! those return plaintext for the model. These wire types are consumed
//! by the manager portal's skills catalog: it lists skills installed on
//! a daemon and pulls full content when importing one.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSkillSummary {
    pub name: String,
    /// First non-blank description from `SKILL.md` (frontmatter or
    /// first body line). Empty string if the file is missing or has no
    /// description — listing should still surface the directory.
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSkillContents {
    pub name: String,
    pub files: Vec<WireSkillFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSkillFile {
    /// Path relative to the skill root (`.aciddog/skills/<name>/`).
    pub rel_path: String,
    pub contents_b64: String,
}
