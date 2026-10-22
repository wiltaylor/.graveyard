//! Manager-side skills catalog.
//!
//! The catalog lives at `<data_dir>/skills/<source>/<name>/` on disk
//! (mirrors the Anthropic skill on-disk layout — a directory with a
//! `SKILL.md` plus any supporting files) and is indexed in SQLite via
//! `migrations/0003_skills.sql`.
//!
//! Three sources are supported, distinguished by the `source_kind`
//! column:
//!   - `uploaded`: the user dropped a `.zip` into the web UI.
//!   - `imported`: pulled from a registered aciddog instance.
//!   - `marketplace`: extracted from a plugin in a registered marketplace.

pub mod catalog;
pub mod import;
pub mod marketplace;
pub mod push;

use serde::{Deserialize, Serialize};

/// Public summary returned by `GET /api/skills`. Joins the catalog row
/// with a quick "last pushed to ..." snapshot so the UI can render a
/// table without an extra round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub source_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub content_hash: String,
    /// `Some` when an upstream copy (a marketplace) has a newer hash
    /// than the catalog. UI lights up the "update available" indicator
    /// when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_available: Option<bool>,
    #[serde(default)]
    pub last_pushed_to: Vec<SkillPushSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPushSummary {
    pub instance_id: String,
    pub instance_name: String,
    pub pushed_at: String,
    /// `false` when the catalog has a newer hash than what was pushed
    /// — i.e. the instance is "stale" and a re-push would deliver
    /// changed content.
    pub up_to_date: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSummary {
    pub id: String,
    pub git_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_spec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_sha: Option<String>,
    /// Total skills the marketplace exposes (not how many we've
    /// installed into the catalog — that's `installed_skill_count`).
    pub skill_count: usize,
    /// Subset of `skill_count` that the user has explicitly installed.
    pub installed_skill_count: usize,
}

/// One skill available *in a marketplace*, but not necessarily installed
/// into the catalog. Returned by `GET /api/marketplaces/:id/skills`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceAvailableSkill {
    /// Skill directory name (`is_valid_skill_name`-compliant).
    pub name: String,
    pub plugin: String,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// SHA-256 over the skill directory in the marketplace cache.
    pub upstream_hash: String,
    /// `true` if a catalog row already exists for this marketplace+skill.
    pub installed: bool,
    /// When `installed`, whether the catalog hash matches `upstream_hash`.
    /// `None` for skills that aren't installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up_to_date: Option<bool>,
}

/// Per-instance result of `POST /api/skills/:id/push`. The route
/// returns one of these per requested instance — partial success is
/// possible (push to A succeeds, B fails), so we don't bail on the
/// first error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPushResult {
    pub instance_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
