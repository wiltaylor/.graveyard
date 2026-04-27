//! Anthropic-format plugin marketplace support.
//!
//! A marketplace is a git repository whose root contains
//! `.claude-plugin/marketplace.json` listing one or more plugins. We
//! only consume the `skills/` subdirectory of each referenced plugin —
//! commands, agents, hooks, and MCP servers are out of scope for this
//! pass.
//!
//! Plugin sources supported in v1:
//!   - `"./relative/path"`: a directory in the same marketplace repo.
//!   - `{ "source": "github", "repo": "owner/repo", "ref": "..." }`:
//!     fetched into a sibling git clone.
//!   - `{ "source": "git-subdir", "url": "...", "path": "...", "ref": "..." }`:
//!     fetched and the named subdir consumed.
//!
//! `url` and `npm` sources are deferred — see "Out of scope" in the
//! plan.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use flate2::read::GzDecoder;
use git2::{FetchOptions, Repository};
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::skills::catalog::{hash_dir, is_valid_skill_name, load_skill_files, store_skill_dir};
use crate::skills::{MarketplaceAvailableSkill, MarketplaceSummary};
use crate::state::AppState;

/// Sink for human-readable progress events during long-running marketplace
/// operations. The route layer wraps this around an mpsc channel and
/// streams each line to the browser as NDJSON; non-streaming callers
/// (tests, in-process refresh) use [`ProgressSink::noop`].
pub struct ProgressSink {
    inner: Option<mpsc::UnboundedSender<String>>,
}

impl ProgressSink {
    pub fn noop() -> Self {
        Self { inner: None }
    }

    pub fn channel() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { inner: Some(tx) }, rx)
    }

    /// Best-effort send. If the receiver is gone (e.g. the browser
    /// hung up mid-import), we keep working — the marketplace state
    /// persists regardless of whether the user saw the final event.
    pub fn send(&self, msg: impl Into<String>) {
        if let Some(tx) = &self.inner {
            let _ = tx.send(msg.into());
        }
    }
}

#[derive(Debug, Deserialize)]
struct MarketplaceManifest {
    name: String,
    plugins: Vec<PluginEntry>,
}

#[derive(Debug, Deserialize)]
struct PluginEntry {
    name: String,
    source: PluginSource,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PluginSource {
    Path(String),
    Object(SourceObject),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "source")]
enum SourceObject {
    #[serde(rename = "github")]
    Github {
        repo: String,
        #[serde(default, rename = "ref")]
        ref_spec: Option<String>,
        #[serde(default)]
        sha: Option<String>,
    },
    #[serde(rename = "git-subdir")]
    GitSubdir {
        url: String,
        path: String,
        #[serde(default, rename = "ref")]
        ref_spec: Option<String>,
        #[serde(default)]
        sha: Option<String>,
    },
    /// Generic remote source. Per the Anthropic plugin spec, `url`
    /// usually points at a git remote (anything ending in `.git`),
    /// optionally pinned via `ref` + `sha`. Plain HTTP archives
    /// (`.tar.gz`, `.tgz`, `.zip`) are still supported as a fallback so
    /// release tarballs work too.
    #[serde(rename = "url")]
    Url {
        url: String,
        #[serde(default, rename = "ref")]
        ref_spec: Option<String>,
        #[serde(default)]
        sha: Option<String>,
    },
    #[serde(rename = "npm")]
    Npm {
        package: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        registry: Option<String>,
    },
}

/// Add a marketplace: clone the repo, parse the manifest, materialise
/// every plugin's source on disk so we can list its skills, and persist
/// the marketplace row. Skills are *not* auto-installed into the
/// catalog — the user picks which ones to install from the
/// marketplace's own view.
pub async fn add_marketplace(
    state: &AppState,
    git_url: &str,
    ref_spec: Option<&str>,
    name_override: Option<&str>,
    progress: &ProgressSink,
) -> AppResult<MarketplaceSummary> {
    let scratch_id = format!("pending-{}", uuid::Uuid::new_v4());
    let scratch_dir = state.marketplaces_dir().join(&scratch_id);
    let cleanup_on_error = ScratchGuard::new(&scratch_dir);

    fs::create_dir_all(state.marketplaces_dir())
        .map_err(|e| AppError::Validation(format!("create marketplaces dir: {e}")))?;
    progress.send(format!("Cloning {git_url}…"));
    clone_repo(git_url, ref_spec, &scratch_dir)?;

    progress.send("Reading marketplace.json…");
    let manifest = read_manifest(&scratch_dir)?;
    let id = name_override
        .map(str::to_string)
        .unwrap_or_else(|| manifest.name.clone());
    if !is_valid_marketplace_id(&id) {
        return Err(AppError::Validation(format!(
            "invalid marketplace id `{id}` — kebab-case, ascii alnum + dashes only"
        )));
    }

    let final_dir = state.marketplaces_dir().join(&id);
    if final_dir.exists() {
        // Two cases:
        //   1. Legitimate conflict — a DB row already references this id.
        //   2. Orphan — a previous add crashed *after* the rename but
        //      before/during the post-rename DB writes (or a delete
        //      partially failed). Auto-clean orphans so the user
        //      doesn't get stuck with an undeletable marketplace.
        let row_exists =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM skill_marketplaces WHERE id = ?1")
                .bind(&id)
                .fetch_optional(&state.pool)
                .await
                .map_err(AppError::Db)?
                .is_some();
        if row_exists {
            return Err(AppError::Conflict(format!(
                "marketplace `{id}` already exists"
            )));
        }
        tracing::warn!(
            id = %id,
            "removing orphaned marketplace dir with no DB row"
        );
        progress.send(format!(
            "Cleaning up orphaned `{id}` directory from a previous failed add…"
        ));
        fs::remove_dir_all(&final_dir).map_err(|e| {
            AppError::Validation(format!("remove orphan dir {}: {e}", final_dir.display()))
        })?;
        let bucket = state.skills_dir().join(format!("marketplace-{id}"));
        if bucket.exists() {
            let _ = fs::remove_dir_all(&bucket);
        }
    }
    fs::rename(&scratch_dir, &final_dir)
        .map_err(|e| AppError::Validation(format!("rename: {e}")))?;
    cleanup_on_error.disarm();

    // Anything from here on must roll back final_dir + DB rows on error,
    // or the next add of the same id will hit a stale orphan.
    let outcome = finalize_add(
        state, &id, git_url, ref_spec, &final_dir, &manifest, progress,
    )
    .await;
    match outcome {
        Ok(summary) => Ok(summary),
        Err(e) => {
            tracing::warn!(id = %id, error = %e, "rolling back failed marketplace add");
            rollback_marketplace(state, &id, &final_dir).await;
            Err(e)
        }
    }
}

async fn finalize_add(
    state: &AppState,
    id: &str,
    git_url: &str,
    ref_spec: Option<&str>,
    final_dir: &Path,
    manifest: &MarketplaceManifest,
    progress: &ProgressSink,
) -> AppResult<MarketplaceSummary> {
    let head_sha = head_sha(final_dir).ok();
    let raw = fs::read_to_string(final_dir.join(".claude-plugin/marketplace.json"))
        .map_err(|e| AppError::Validation(format!("read manifest: {e}")))?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO skill_marketplaces(id, git_url, ref_spec, last_synced_at, last_synced_sha, raw_manifest, available_skill_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
    )
    .bind(id)
    .bind(git_url)
    .bind(ref_spec)
    .bind(&now)
    .bind(head_sha.as_deref())
    .bind(&raw)
    .execute(&state.pool)
    .await
    .map_err(AppError::Db)?;

    let skills = enumerate_marketplace_skills(state, id, manifest, final_dir, progress).await?;
    sqlx::query("UPDATE skill_marketplaces SET available_skill_count = ?1 WHERE id = ?2")
        .bind(skills.len() as i64)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(AppError::Db)?;
    progress.send(format!("Found {} skill(s).", skills.len()));
    Ok(MarketplaceSummary {
        id: id.to_string(),
        git_url: git_url.to_string(),
        ref_spec: ref_spec.map(str::to_string),
        last_synced_at: Some(now),
        last_synced_sha: head_sha,
        skill_count: skills.len(),
        installed_skill_count: 0,
    })
}

/// Best-effort rollback when an add fails after the rename. Errors are
/// swallowed because we're already returning the original error — we
/// just want to leave the manager in a state where retrying works.
async fn rollback_marketplace(state: &AppState, id: &str, final_dir: &Path) {
    if final_dir.exists() {
        if let Err(e) = fs::remove_dir_all(final_dir) {
            tracing::warn!(id, error = %e, "rollback: remove final_dir failed");
        }
    }
    let bucket = state.skills_dir().join(format!("marketplace-{id}"));
    if bucket.exists() {
        if let Err(e) = fs::remove_dir_all(&bucket) {
            tracing::warn!(id, error = %e, "rollback: remove skill bucket failed");
        }
    }
    let _ = sqlx::query("DELETE FROM skills WHERE source_kind = 'marketplace' AND source_ref = ?1")
        .bind(id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM skill_marketplaces WHERE id = ?1")
        .bind(id)
        .execute(&state.pool)
        .await;
}

/// Refresh: `git pull` (fast-forward) the cached clone, re-resolve every
/// plugin source, and recompute `upstream_hash` on every catalog row
/// already installed from this marketplace. Skills are not installed
/// or removed automatically — the user does that explicitly.
pub async fn update_marketplace(
    state: &AppState,
    id: &str,
    progress: &ProgressSink,
) -> AppResult<MarketplaceSummary> {
    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT git_url, ref_spec FROM skill_marketplaces WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)?;
    let (git_url, ref_spec) = row;
    let dir = state.marketplaces_dir().join(id);
    if !dir.is_dir() {
        return Err(AppError::Validation(format!(
            "marketplace `{id}` is registered but its clone is missing — re-add it"
        )));
    }
    progress.send("Pulling latest…");
    pull_repo(&dir, ref_spec.as_deref())?;
    let head_sha = head_sha(&dir).ok();
    progress.send("Reading marketplace.json…");
    let manifest = read_manifest(&dir)?;
    let raw = fs::read_to_string(dir.join(".claude-plugin/marketplace.json"))
        .map_err(|e| AppError::Validation(format!("read manifest: {e}")))?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE skill_marketplaces SET last_synced_at = ?1, last_synced_sha = ?2, raw_manifest = ?3 WHERE id = ?4",
    )
    .bind(&now)
    .bind(head_sha.as_deref())
    .bind(&raw)
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(AppError::Db)?;

    let skills = enumerate_marketplace_skills(state, id, &manifest, &dir, progress).await?;
    refresh_installed_upstream_hashes(state, id, &skills).await?;
    sqlx::query("UPDATE skill_marketplaces SET available_skill_count = ?1 WHERE id = ?2")
        .bind(skills.len() as i64)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(AppError::Db)?;
    let installed_count = installed_count(&state.pool, id).await?;
    progress.send(format!(
        "Found {} skill(s); {} installed in catalog.",
        skills.len(),
        installed_count
    ));
    Ok(MarketplaceSummary {
        id: id.to_string(),
        git_url,
        ref_spec,
        last_synced_at: Some(now),
        last_synced_sha: head_sha,
        skill_count: skills.len(),
        installed_skill_count: installed_count,
    })
}

/// Public list of available skills in a marketplace. Walks the cache
/// without modifying the catalog, then annotates each entry with
/// installation status by joining against the `skills` table.
pub async fn list_available_skills(
    state: &AppState,
    marketplace_id: &str,
) -> AppResult<Vec<MarketplaceAvailableSkill>> {
    let _ = sqlx::query_as::<_, (String,)>("SELECT id FROM skill_marketplaces WHERE id = ?1")
        .bind(marketplace_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::Db)?
        .ok_or(AppError::NotFound)?;
    let dir = state.marketplaces_dir().join(marketplace_id);
    if !dir.is_dir() {
        return Err(AppError::Validation(format!(
            "marketplace `{marketplace_id}` is registered but its clone is missing — re-add it"
        )));
    }
    let manifest = read_manifest(&dir)?;
    enumerate_marketplace_skills(
        state,
        marketplace_id,
        &manifest,
        &dir,
        &ProgressSink::noop(),
    )
    .await
}

/// Install a single available skill from a marketplace into the
/// catalog. Re-runs `store_skill_dir` + `upsert_skill` with the
/// upstream files so `content_hash == upstream_hash` post-install.
pub async fn install_skill(
    state: &AppState,
    marketplace_id: &str,
    skill_name: &str,
) -> AppResult<String> {
    let dir = state.marketplaces_dir().join(marketplace_id);
    if !dir.is_dir() {
        return Err(AppError::Validation(format!(
            "marketplace `{marketplace_id}` clone missing"
        )));
    }
    let manifest = read_manifest(&dir)?;
    let http = build_http_client()?;
    for plugin in &manifest.plugins {
        let plugin_dir = match resolve_plugin_dir(&http, &dir, plugin).await? {
            Some(d) => d,
            None => continue,
        };
        let skills_root = plugin_dir.join("skills");
        if !skills_root.is_dir() {
            continue;
        }
        let skill_dir = skills_root.join(skill_name);
        if !skill_dir.join("SKILL.md").is_file() {
            continue;
        }
        if !is_valid_skill_name(skill_name) {
            return Err(AppError::Validation(format!(
                "invalid skill name `{skill_name}`"
            )));
        }
        let files = load_skill_files(&skill_dir)?;
        let upstream_hash =
            hash_dir(&skill_dir).map_err(|e| AppError::Validation(format!("hash: {e}")))?;
        store_skill_dir(
            &state.skills_dir(),
            "marketplace",
            Some(marketplace_id),
            skill_name,
            &files,
        )?;
        let id = crate::skills::catalog::upsert_skill(
            &state.pool,
            "marketplace",
            Some(marketplace_id),
            skill_name,
            &files,
            plugin.version.as_deref(),
            Some(&upstream_hash),
        )
        .await?;
        return Ok(id);
    }
    Err(AppError::NotFound)
}

/// Remove a marketplace and every skill sourced from it.
pub async fn remove_marketplace(state: &AppState, id: &str) -> AppResult<()> {
    let dir = state.marketplaces_dir().join(id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| AppError::Validation(format!("remove clone: {e}")))?;
    }
    let bucket = state.skills_dir().join(format!("marketplace-{id}"));
    if bucket.exists() {
        fs::remove_dir_all(&bucket)
            .map_err(|e| AppError::Validation(format!("remove skill bucket: {e}")))?;
    }
    sqlx::query("DELETE FROM skills WHERE source_kind = 'marketplace' AND source_ref = ?1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(AppError::Db)?;
    sqlx::query("DELETE FROM skill_marketplaces WHERE id = ?1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

pub async fn list_marketplaces(pool: &SqlitePool) -> AppResult<Vec<MarketplaceSummary>> {
    let rows = sqlx::query_as::<_, MarketplaceRow>(
        "SELECT id, git_url, ref_spec, last_synced_at, last_synced_sha, available_skill_count FROM skill_marketplaces ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let installed = installed_count(pool, &row.id).await?;
        out.push(MarketplaceSummary {
            id: row.id,
            git_url: row.git_url,
            ref_spec: row.ref_spec,
            last_synced_at: row.last_synced_at,
            last_synced_sha: row.last_synced_sha,
            skill_count: row.available_skill_count as usize,
            installed_skill_count: installed,
        });
    }
    Ok(out)
}

#[derive(Debug, sqlx::FromRow)]
struct MarketplaceRow {
    id: String,
    git_url: String,
    ref_spec: Option<String>,
    last_synced_at: Option<String>,
    last_synced_sha: Option<String>,
    available_skill_count: i64,
}

/// Re-fetch a single marketplace-sourced skill from the cached clone.
/// Implemented as "sync the whole marketplace" since the bottleneck is
/// FS walks rather than per-skill cost — refreshing one or many is
/// indistinguishable.
pub async fn refresh_skill(state: &AppState, skill_id: &str) -> AppResult<()> {
    let (source_kind, source_ref, name) = sqlx::query_as::<_, (String, Option<String>, String)>(
        "SELECT source_kind, source_ref, name FROM skills WHERE id = ?1",
    )
    .bind(skill_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)?;
    if source_kind != "marketplace" {
        return Err(AppError::Validation(format!(
            "skill `{skill_id}` is not marketplace-sourced — nothing to refresh"
        )));
    }
    let marketplace_id = source_ref
        .ok_or_else(|| AppError::Validation("marketplace skill has no source_ref".into()))?;
    install_skill(state, &marketplace_id, &name).await?;
    Ok(())
}

/// Walk the marketplace cache and report every skill it exposes. No DB
/// mutation, no catalog writes — purely a read of what's available
/// upstream. Pairs with [`refresh_installed_upstream_hashes`] when the
/// caller wants the catalog's `upstream_hash` to follow.
async fn enumerate_marketplace_skills(
    state: &AppState,
    marketplace_id: &str,
    manifest: &MarketplaceManifest,
    marketplace_dir: &Path,
    progress: &ProgressSink,
) -> AppResult<Vec<MarketplaceAvailableSkill>> {
    let http = build_http_client()?;
    let installed = installed_skill_hashes(&state.pool, marketplace_id).await?;
    let mut out: Vec<MarketplaceAvailableSkill> = Vec::new();
    for plugin in &manifest.plugins {
        progress.send(format!("Resolving plugin `{}`…", plugin.name));
        let plugin_dir = match resolve_plugin_dir(&http, marketplace_dir, plugin).await? {
            Some(d) => d,
            None => {
                progress.send(format!(
                    "Skipped plugin `{}` (unsupported source or missing files).",
                    plugin.name
                ));
                continue;
            }
        };
        let skills_root = plugin_dir.join("skills");
        if !skills_root.is_dir() {
            progress.send(format!(
                "Plugin `{}` has no skills/ directory.",
                plugin.name
            ));
            continue;
        }
        for entry in fs::read_dir(&skills_root)
            .map_err(|e| AppError::Validation(format!("read skills/: {e}")))?
        {
            let entry = entry.map_err(|e| AppError::Validation(format!("read skills/: {e}")))?;
            let file_type = entry
                .file_type()
                .map_err(|e| AppError::Validation(format!("file_type: {e}")))?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_valid_skill_name(&name) {
                tracing::warn!(name, "skipping skill with invalid name");
                continue;
            }
            let dir = entry.path();
            if !dir.join("SKILL.md").is_file() {
                continue;
            }
            let upstream_hash = match hash_dir(&dir) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(name, error = %e, "hash upstream skill");
                    continue;
                }
            };
            let description = read_skill_description(&dir).unwrap_or_default();
            let installed_hash = installed.iter().find(|(n, _)| n == &name).map(|(_, h)| h);
            let entry = MarketplaceAvailableSkill {
                name,
                plugin: plugin.name.clone(),
                description,
                version: plugin.version.clone(),
                up_to_date: installed_hash.map(|h| h == &upstream_hash),
                installed: installed_hash.is_some(),
                upstream_hash,
            };
            out.push(entry);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// For every catalog row already installed from this marketplace,
/// recompute and persist the latest `upstream_hash`. The catalog's
/// "update available" indicator (`upstream_hash != content_hash`)
/// follows directly.
async fn refresh_installed_upstream_hashes(
    state: &AppState,
    marketplace_id: &str,
    skills: &[MarketplaceAvailableSkill],
) -> AppResult<()> {
    for skill in skills {
        if !skill.installed {
            continue;
        }
        sqlx::query(
            "UPDATE skills SET upstream_hash = ?1, updated_at = ?2
             WHERE source_kind = 'marketplace' AND source_ref = ?3 AND name = ?4",
        )
        .bind(&skill.upstream_hash)
        .bind(Utc::now().to_rfc3339())
        .bind(marketplace_id)
        .bind(&skill.name)
        .execute(&state.pool)
        .await
        .map_err(AppError::Db)?;
    }
    Ok(())
}

async fn installed_skill_hashes(
    pool: &SqlitePool,
    marketplace_id: &str,
) -> AppResult<Vec<(String, String)>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT name, content_hash FROM skills WHERE source_kind = 'marketplace' AND source_ref = ?1",
    )
    .bind(marketplace_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(rows)
}

async fn installed_count(pool: &SqlitePool, marketplace_id: &str) -> AppResult<usize> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM skills WHERE source_kind = 'marketplace' AND source_ref = ?1",
    )
    .bind(marketplace_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(row.0 as usize)
}

fn read_skill_description(dir: &Path) -> Option<String> {
    let body = fs::read_to_string(dir.join("SKILL.md")).ok()?;
    crate::skills::catalog::extract_description(&body)
}

fn build_http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("aciddog-manager/0.1 (+marketplaces)")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Validation(format!("build http client: {e}")))
}

async fn resolve_plugin_dir(
    http: &reqwest::Client,
    marketplace_dir: &Path,
    plugin: &PluginEntry,
) -> AppResult<Option<PathBuf>> {
    match &plugin.source {
        PluginSource::Path(rel) => {
            let p = marketplace_dir.join(rel.trim_start_matches("./"));
            if !p.is_dir() {
                tracing::warn!(plugin = %plugin.name, path = %p.display(), "plugin path missing");
                return Ok(None);
            }
            Ok(Some(p))
        }
        PluginSource::Object(SourceObject::Github {
            repo,
            ref_spec,
            sha,
        }) => {
            let cache_key = format!(
                "github--{}--{}--{}",
                repo.replace('/', "_"),
                ref_spec.as_deref().unwrap_or("default"),
                sha.as_deref().unwrap_or("head"),
            );
            let cache = marketplace_dir.join(".plugin-cache").join(cache_key);
            if !cache.exists() {
                let url = format!("https://github.com/{repo}.git");
                clone_repo_pinned(&url, ref_spec.as_deref(), sha.as_deref(), &cache)?;
            }
            Ok(Some(cache))
        }
        PluginSource::Object(SourceObject::GitSubdir {
            url,
            path,
            ref_spec,
            sha,
        }) => {
            let cache_key = format!(
                "gitsubdir--{}--{}--{}",
                sanitize_url(url),
                ref_spec.as_deref().unwrap_or("default"),
                sha.as_deref().unwrap_or("head"),
            );
            let cache = marketplace_dir.join(".plugin-cache").join(cache_key);
            if !cache.exists() {
                clone_repo_pinned(url, ref_spec.as_deref(), sha.as_deref(), &cache)?;
            }
            let sub = cache.join(path);
            if !sub.is_dir() {
                tracing::warn!(plugin = %plugin.name, path = %sub.display(), "git-subdir path missing");
                return Ok(None);
            }
            Ok(Some(sub))
        }
        PluginSource::Object(SourceObject::Url { url, ref_spec, sha }) => {
            let cache_key = format!(
                "url--{}--{}--{}",
                sanitize_url(url),
                ref_spec.as_deref().unwrap_or("default"),
                sha.as_deref().unwrap_or("head"),
            );
            let cache = marketplace_dir.join(".plugin-cache").join(cache_key);
            if !cache.exists() {
                if let Err(e) =
                    ensure_url_plugin(http, url, ref_spec.as_deref(), sha.as_deref(), &cache).await
                {
                    tracing::warn!(plugin = %plugin.name, url, error = %e, "url plugin fetch failed");
                    return Ok(None);
                }
            }
            // Archives sometimes wrap their contents in a single
            // top-level directory; descend transparently if `skills/`
            // isn't right at the cache root.
            let plugin_root = pick_archive_root(&cache);
            Ok(Some(plugin_root))
        }
        PluginSource::Object(SourceObject::Npm {
            package,
            version,
            registry,
        }) => {
            let registry = registry
                .as_deref()
                .unwrap_or("https://registry.npmjs.org")
                .trim_end_matches('/');
            let cache_key = format!(
                "npm--{}--{}",
                package.replace('/', "_"),
                version.as_deref().unwrap_or("latest")
            );
            let cache = marketplace_dir.join(".plugin-cache").join(&cache_key);
            if !cache.exists() {
                if let Err(e) =
                    ensure_npm_plugin(http, registry, package, version.as_deref(), &cache).await
                {
                    tracing::warn!(plugin = %plugin.name, package, error = %e, "npm plugin fetch failed");
                    return Ok(None);
                }
            }
            // npm tarballs always wrap contents in `package/` at the root.
            let plugin_root = cache.join("package");
            if !plugin_root.is_dir() {
                tracing::warn!(plugin = %plugin.name, path = %plugin_root.display(), "npm package/ subdir missing");
                return Ok(None);
            }
            Ok(Some(plugin_root))
        }
    }
}

/// Materialise a `url`-source plugin. Per the Anthropic plugin spec
/// the URL is *usually* a git remote (anything ending in `.git`) with
/// optional `ref`/`sha` pinning — that's how Anthropic's own marketplace
/// links to third-party plugins. Plain HTTP archives (`.tar.gz`,
/// `.tgz`, `.zip`) are still supported as a fallback so release
/// tarballs work too. Detection is purely by `.git` suffix; if you have
/// a non-`.git` git URL, the `github` or `git-subdir` source variants
/// are the right way to declare it.
async fn ensure_url_plugin(
    http: &reqwest::Client,
    url: &str,
    ref_spec: Option<&str>,
    sha: Option<&str>,
    dest: &Path,
) -> AppResult<()> {
    if is_git_url(url) {
        clone_repo_pinned(url, ref_spec, sha, dest)?;
        return Ok(());
    }
    if ref_spec.is_some() || sha.is_some() {
        return Err(AppError::Validation(format!(
            "url `{url}` is not a `.git` URL, but `ref`/`sha` were supplied — \
             remove them, or change the URL to point at the git remote (`*.git`)"
        )));
    }
    let bytes = http_get_bytes(http, url).await?;
    extract_archive_to_dir(&bytes, dest)
}

fn is_git_url(url: &str) -> bool {
    // `.git` suffix is the canonical signal for a git remote URL.
    // Trailing slash is unusual but tolerated.
    let trimmed = url.trim_end_matches('/');
    trimmed.ends_with(".git")
}

/// Resolve the requested version, fetch the tarball URL, download, and
/// extract. `version = None` (or `"latest"`/`"*"`) hits the registry's
/// `/latest` dist-tag. Otherwise we look up the version manifest by
/// exact version. Range specifiers (`^1.2.3`, `~1.2`, etc.) are
/// rejected — we don't link an npm CLI and don't ship a semver
/// resolver.
async fn ensure_npm_plugin(
    http: &reqwest::Client,
    registry: &str,
    package: &str,
    version: Option<&str>,
    dest: &Path,
) -> AppResult<()> {
    let version_path = match version {
        None => "latest".to_string(),
        Some(v) if v == "latest" || v == "*" || v.is_empty() => "latest".to_string(),
        Some(v) if v.starts_with(|c: char| c.is_ascii_digit()) => v.to_string(),
        Some(v) => {
            return Err(AppError::Validation(format!(
                "npm version range `{v}` is unsupported — pin to an exact version (e.g. `1.2.3`) \
                 or omit `version` to use the latest dist-tag"
            )));
        }
    };
    let manifest_url = format!(
        "{}/{}/{}",
        registry,
        npm_path_segment(package),
        version_path
    );
    let manifest_bytes = http_get_bytes(http, &manifest_url).await?;
    let manifest: NpmVersionManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| AppError::Validation(format!("parse npm manifest at {manifest_url}: {e}")))?;
    let tarball = manifest.dist.tarball;
    let tarball_bytes = http_get_bytes(http, &tarball).await?;
    extract_archive_to_dir(&tarball_bytes, dest)
}

#[derive(Debug, Deserialize)]
struct NpmVersionManifest {
    dist: NpmDist,
}

#[derive(Debug, Deserialize)]
struct NpmDist {
    tarball: String,
}

/// Encode the package name for use as a single URL path segment.
/// Scoped packages (`@org/name`) need the `/` percent-encoded so the
/// registry sees one segment, not two.
fn npm_path_segment(package: &str) -> String {
    package.replace('/', "%2F")
}

async fn http_get_bytes(http: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Validation(format!("GET {url}: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::Validation(format!("GET {url}: HTTP {status}")));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Validation(format!("read body of {url}: {e}")))?;
    Ok(bytes.to_vec())
}

/// Sniff the archive's magic bytes and extract to `dest` via a sibling
/// staging dir + `rename(2)`. Supports zip, tar, and gzip-wrapped tar.
fn extract_archive_to_dir(bytes: &[u8], dest: &Path) -> AppResult<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::Validation("destination has no parent".into()))?;
    fs::create_dir_all(parent)
        .map_err(|e| AppError::Validation(format!("mkdir {}: {e}", parent.display())))?;
    let staging = parent.join(format!(
        ".tmp-extract-{}",
        std::process::id() as u64 ^ uuid::Uuid::new_v4().as_u128() as u64
    ));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging)
        .map_err(|e| AppError::Validation(format!("create staging: {e}")))?;
    let outcome = if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        extract_zip_to(bytes, &staging)
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        extract_tar_gz_to(bytes, &staging)
    } else if has_tar_magic(bytes) {
        extract_tar_to(bytes, &staging)
    } else {
        Err(AppError::Validation(
            "downloaded archive is not zip, gzip, or tar — refusing to extract".into(),
        ))
    };
    if let Err(e) = outcome {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| AppError::Validation(format!("remove old: {e}")))?;
    }
    fs::rename(&staging, dest).map_err(|e| AppError::Validation(format!("rename: {e}")))
}

/// `ustar` magic at offset 257 — the structural marker that says
/// "this byte stream is an uncompressed tar archive".
fn has_tar_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 265 && &bytes[257..262] == b"ustar"
}

fn extract_zip_to(bytes: &[u8], dest: &Path) -> AppResult<()> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| AppError::Validation(format!("open zip: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Validation(format!("zip entry {i}: {e}")))?;
        let raw = entry
            .enclosed_name()
            .ok_or_else(|| AppError::Validation(format!("zip entry {i} has unsafe path")))?
            .to_path_buf();
        check_entry_path(&raw)?;
        let out = dest.join(&raw);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| AppError::Validation(format!("mkdir: {e}")))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::Validation(format!("mkdir: {e}")))?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| AppError::Validation(format!("read entry: {e}")))?;
        fs::write(&out, buf).map_err(|e| AppError::Validation(format!("write: {e}")))?;
    }
    Ok(())
}

fn extract_tar_gz_to(bytes: &[u8], dest: &Path) -> AppResult<()> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    extract_tar_archive(tar::Archive::new(decoder), dest)
}

fn extract_tar_to(bytes: &[u8], dest: &Path) -> AppResult<()> {
    extract_tar_archive(tar::Archive::new(Cursor::new(bytes)), dest)
}

fn extract_tar_archive<R: Read>(mut archive: tar::Archive<R>, dest: &Path) -> AppResult<()> {
    let entries = archive
        .entries()
        .map_err(|e| AppError::Validation(format!("tar entries: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| AppError::Validation(format!("tar entry: {e}")))?;
        let raw = entry
            .path()
            .map_err(|e| AppError::Validation(format!("tar path: {e}")))?
            .into_owned();
        check_entry_path(&raw)?;
        // Skip non-regular entries (symlinks, hard links, devices,
        // fifos, etc.). We only want files and directories.
        let etype = entry.header().entry_type();
        if !etype.is_file() && !etype.is_dir() {
            continue;
        }
        let out = dest.join(&raw);
        if etype.is_dir() {
            fs::create_dir_all(&out).map_err(|e| AppError::Validation(format!("mkdir: {e}")))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::Validation(format!("mkdir: {e}")))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| AppError::Validation(format!("read tar entry: {e}")))?;
        fs::write(&out, buf).map_err(|e| AppError::Validation(format!("write: {e}")))?;
    }
    Ok(())
}

/// Reject paths that would escape the extraction root: absolute paths,
/// or any component that's `..`.
fn check_entry_path(p: &Path) -> AppResult<()> {
    if p.is_absolute() {
        return Err(AppError::Validation(format!(
            "archive entry has absolute path `{}`",
            p.display()
        )));
    }
    for component in p.components() {
        match component {
            std::path::Component::Normal(_) => {}
            std::path::Component::CurDir => {}
            _ => {
                return Err(AppError::Validation(format!(
                    "archive entry escapes root: `{}`",
                    p.display()
                )));
            }
        }
    }
    Ok(())
}

/// If `cache` directly contains a `skills/` directory, use it as-is.
/// Otherwise, if it has a single subdirectory and *that* directory
/// contains `skills/`, descend. Mirrors the typical layout where a
/// release archive wraps everything in a single project-named folder.
fn pick_archive_root(cache: &Path) -> PathBuf {
    if cache.join("skills").is_dir() {
        return cache.to_path_buf();
    }
    let mut subdirs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = fs::read_dir(cache) {
        for entry in rd.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                subdirs.push(entry.path());
                if subdirs.len() > 1 {
                    break;
                }
            }
        }
    }
    if subdirs.len() == 1 && subdirs[0].join("skills").is_dir() {
        return subdirs.into_iter().next().unwrap();
    }
    cache.to_path_buf()
}

fn sanitize_url(url: &str) -> String {
    url.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn read_manifest(repo_dir: &Path) -> AppResult<MarketplaceManifest> {
    let path = repo_dir.join(".claude-plugin/marketplace.json");
    let raw = fs::read_to_string(&path)
        .map_err(|e| AppError::Validation(format!("read {}: {e}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("parse marketplace.json: {e}")))
}

fn clone_repo(url: &str, ref_spec: Option<&str>, dest: &Path) -> AppResult<()> {
    clone_repo_pinned(url, ref_spec, None, dest)
}

/// Clone with optional branch/tag (`ref_spec`) and commit (`sha`).
/// Without a sha we shallow-clone (`depth=1`) for speed. With a sha we
/// pull the full history because libgit2 can't always check out an
/// arbitrary commit from a depth-1 clone.
fn clone_repo_pinned(
    url: &str,
    ref_spec: Option<&str>,
    sha: Option<&str>,
    dest: &Path,
) -> AppResult<()> {
    let mut fetch_opts = FetchOptions::new();
    if sha.is_none() {
        fetch_opts.depth(1);
    }
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);
    if let Some(r) = ref_spec {
        builder.branch(r);
    }
    let repo = builder
        .clone(url, dest)
        .map_err(|e| AppError::Validation(format!("clone {url}: {e}")))?;
    if let Some(sha) = sha {
        let oid = git2::Oid::from_str(sha)
            .map_err(|e| AppError::Validation(format!("invalid sha `{sha}`: {e}")))?;
        let object = repo
            .find_object(oid, None)
            .map_err(|e| AppError::Validation(format!("find commit {sha} in {url}: {e}")))?;
        repo.checkout_tree(&object, Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(|e| AppError::Validation(format!("checkout {sha}: {e}")))?;
        repo.set_head_detached(oid)
            .map_err(|e| AppError::Validation(format!("set_head_detached {sha}: {e}")))?;
    }
    Ok(())
}

fn pull_repo(dir: &Path, ref_spec: Option<&str>) -> AppResult<()> {
    let repo =
        Repository::open(dir).map_err(|e| AppError::Validation(format!("open repo: {e}")))?;
    let branch = ref_spec.unwrap_or("HEAD");
    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| AppError::Validation(format!("find origin: {e}")))?;
    remote
        .fetch(&[branch], None, None)
        .map_err(|e| AppError::Validation(format!("fetch: {e}")))?;
    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .map_err(|e| AppError::Validation(format!("FETCH_HEAD: {e}")))?;
    let commit = repo
        .reference_to_annotated_commit(&fetch_head)
        .map_err(|e| AppError::Validation(format!("annotated commit: {e}")))?;
    let analysis = repo
        .merge_analysis(&[&commit])
        .map_err(|e| AppError::Validation(format!("merge_analysis: {e}")))?;
    if analysis.0.is_up_to_date() {
        return Ok(());
    }
    if !analysis.0.is_fast_forward() {
        return Err(AppError::Validation(
            "non-fast-forward update — manually resolve in the marketplace clone".into(),
        ));
    }
    let refname = format!("refs/heads/{}", branch.trim_start_matches("refs/heads/"));
    if let Ok(mut head_ref) = repo.find_reference(&refname) {
        head_ref
            .set_target(commit.id(), "fast-forward")
            .map_err(|e| AppError::Validation(format!("set_target: {e}")))?;
    } else {
        repo.reference(&refname, commit.id(), true, "create-and-fast-forward")
            .map_err(|e| AppError::Validation(format!("reference: {e}")))?;
    }
    repo.set_head(&refname)
        .map_err(|e| AppError::Validation(format!("set_head: {e}")))?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .map_err(|e| AppError::Validation(format!("checkout: {e}")))?;
    Ok(())
}

fn head_sha(dir: &Path) -> AppResult<String> {
    let repo =
        Repository::open(dir).map_err(|e| AppError::Validation(format!("open repo: {e}")))?;
    let head = repo
        .head()
        .map_err(|e| AppError::Validation(format!("head: {e}")))?;
    let oid = head
        .target()
        .ok_or_else(|| AppError::Validation("HEAD has no target oid".into()))?;
    Ok(oid.to_string())
}

fn is_valid_marketplace_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// On-error cleanup: removes the scratch dir if `disarm()` isn't called
/// before drop. Used during `add_marketplace` so an aborted clone
/// doesn't litter the data directory.
struct ScratchGuard {
    path: PathBuf,
    armed: bool,
}

impl ScratchGuard {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            armed: true,
        }
    }
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        if self.armed && self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let raw = r#"{
            "name": "my-mp",
            "owner": { "name": "Team" },
            "plugins": [
                { "name": "p1", "source": "./plugins/p1" },
                { "name": "p2", "source": { "source": "github", "repo": "a/b" } }
            ]
        }"#;
        let m: MarketplaceManifest = serde_json::from_str(raw).expect("parse");
        assert_eq!(m.name, "my-mp");
        assert_eq!(m.plugins.len(), 2);
        match &m.plugins[0].source {
            PluginSource::Path(p) => assert_eq!(p, "./plugins/p1"),
            _ => panic!("expected path"),
        }
        match &m.plugins[1].source {
            PluginSource::Object(SourceObject::Github { repo, .. }) => assert_eq!(repo, "a/b"),
            _ => panic!("expected github"),
        }
    }

    #[test]
    fn marketplace_id_validation() {
        assert!(is_valid_marketplace_id("anthropics-skills"));
        assert!(is_valid_marketplace_id("v1"));
        assert!(!is_valid_marketplace_id("Has-Upper"));
        assert!(!is_valid_marketplace_id("with space"));
        assert!(!is_valid_marketplace_id(""));
    }

    #[test]
    fn npm_path_segment_encodes_scoped_packages() {
        assert_eq!(npm_path_segment("lodash"), "lodash");
        assert_eq!(npm_path_segment("@anthropic/skills"), "@anthropic%2Fskills");
    }

    #[test]
    fn check_entry_path_rejects_traversal() {
        assert!(check_entry_path(Path::new("foo/bar.txt")).is_ok());
        assert!(check_entry_path(Path::new("./foo/bar.txt")).is_ok());
        assert!(check_entry_path(Path::new("../escape")).is_err());
        assert!(check_entry_path(Path::new("foo/../escape")).is_err());
        assert!(check_entry_path(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn parse_url_and_npm_sources() {
        let raw = r#"{
            "name": "mp",
            "owner": { "name": "x" },
            "plugins": [
                { "name": "u",  "source": { "source": "url", "url": "https://example.com/p.tgz" } },
                { "name": "ug", "source": { "source": "url", "url": "https://github.com/o/r.git", "ref": "main", "sha": "abc1234" } },
                { "name": "n",  "source": { "source": "npm", "package": "@org/p", "version": "1.2.3" } },
                { "name": "n2", "source": { "source": "npm", "package": "lodash" } }
            ]
        }"#;
        let m: MarketplaceManifest = serde_json::from_str(raw).expect("parse");
        match &m.plugins[0].source {
            PluginSource::Object(SourceObject::Url { url, ref_spec, sha }) => {
                assert_eq!(url, "https://example.com/p.tgz");
                assert!(ref_spec.is_none());
                assert!(sha.is_none());
            }
            _ => panic!("expected url"),
        }
        match &m.plugins[1].source {
            PluginSource::Object(SourceObject::Url { url, ref_spec, sha }) => {
                assert_eq!(url, "https://github.com/o/r.git");
                assert_eq!(ref_spec.as_deref(), Some("main"));
                assert_eq!(sha.as_deref(), Some("abc1234"));
            }
            _ => panic!("expected url"),
        }
        match &m.plugins[2].source {
            PluginSource::Object(SourceObject::Npm {
                package,
                version,
                registry,
            }) => {
                assert_eq!(package, "@org/p");
                assert_eq!(version.as_deref(), Some("1.2.3"));
                assert!(registry.is_none());
            }
            _ => panic!("expected npm"),
        }
        match &m.plugins[3].source {
            PluginSource::Object(SourceObject::Npm {
                package, version, ..
            }) => {
                assert_eq!(package, "lodash");
                assert!(version.is_none());
            }
            _ => panic!("expected npm"),
        }
    }

    #[test]
    fn detects_git_urls() {
        assert!(is_git_url("https://github.com/owner/repo.git"));
        assert!(is_git_url("https://gitlab.com/team/plugin.git/"));
        assert!(is_git_url("git@github.com:owner/repo.git"));
        assert!(!is_git_url("https://example.com/release.tar.gz"));
        assert!(!is_git_url("https://github.com/owner/repo"));
        assert!(!is_git_url("https://example.com/some.zip"));
    }
}
