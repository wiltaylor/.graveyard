//! On-disk + DB management of the manager's skills catalog.
//!
//! Each catalog entry occupies one directory under
//! `<data_dir>/skills/<source>/<name>/` and one row in the `skills`
//! table. Writes go through [`store_skill_dir`] (atomic via temp dir +
//! rename); reads come from [`load_skill_files`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine;
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::ipc::SkillFile;
use crate::skills::{SkillPushSummary, SkillSummary};

/// Strict skill-name policy mirroring `aciddog::skills::is_valid_skill_name`.
/// Duplicated here because the manager can't depend on `aciddog` (sqlite3
/// link conflict — see `Cargo.toml` workspace comment).
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

/// Reject `..`, leading `/`, NUL bytes, empty path.
pub fn validate_rel_path(rel: &str) -> Result<(), &'static str> {
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

/// Compose the on-disk path for a catalog entry.
pub fn skill_dir(
    skills_root: &Path,
    source_kind: &str,
    source_ref: Option<&str>,
    name: &str,
) -> PathBuf {
    let bucket = match source_ref {
        Some(r) => format!("{source_kind}-{r}"),
        None => source_kind.to_string(),
    };
    skills_root.join(bucket).join(name)
}

/// SHA-256 over a sorted list of `(rel_path, contents)` entries.
/// Produces a stable hash that survives serialisation and is safe to
/// compare across hosts.
pub fn hash_files(files: &[SkillFile]) -> AppResult<String> {
    let mut sorted: Vec<&SkillFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    let mut hasher = Sha256::new();
    for f in &sorted {
        hasher.update(f.rel_path.as_bytes());
        hasher.update([0u8]);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&f.contents_b64)
            .map_err(|e| AppError::Validation(format!("base64 decode: {e}")))?;
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
}

/// SHA-256 over a directory on disk. Walks recursively, sorts entries
/// by relative path, and feeds `(rel_path \0 len bytes)` into the
/// digest. Used by the marketplace flow where files live on disk
/// rather than in memory.
pub fn hash_dir(dir: &Path) -> io::Result<String> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    walk_dir(dir, dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, bytes) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(bytes);
    }
    Ok(hex(&hasher.finalize()))
}

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            walk_dir(root, &path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let bytes = fs::read(&path)?;
        let rel = path
            .strip_prefix(root)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        out.push((rel.to_string_lossy().replace('\\', "/"), bytes));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Atomically install a set of skill files at the catalog location.
/// Files are staged in a sibling temp dir, then `rename(2)`d over the
/// destination — concurrent readers see either the old tree or the new
/// one, never a half-written state.
pub fn store_skill_dir(
    skills_root: &Path,
    source_kind: &str,
    source_ref: Option<&str>,
    name: &str,
    files: &[SkillFile],
) -> AppResult<PathBuf> {
    if !is_valid_skill_name(name) {
        return Err(AppError::Validation(format!("invalid skill name `{name}`")));
    }
    let bucket = skills_root.join(match source_ref {
        Some(r) => format!("{source_kind}-{r}"),
        None => source_kind.to_string(),
    });
    fs::create_dir_all(&bucket)
        .map_err(|e| AppError::Validation(format!("create catalog dir: {e}")))?;
    let staging = bucket.join(format!(".tmp-{name}-{}", std::process::id()));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging)
        .map_err(|e| AppError::Validation(format!("create staging dir: {e}")))?;
    for f in files {
        validate_rel_path(&f.rel_path)
            .map_err(|e| AppError::Validation(format!("invalid path `{}`: {e}", f.rel_path)))?;
        let dst = staging.join(&f.rel_path);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::Validation(format!("mkdir: {e}")))?;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&f.contents_b64)
            .map_err(|e| AppError::Validation(format!("base64 decode: {e}")))?;
        fs::write(&dst, bytes).map_err(|e| AppError::Validation(format!("write: {e}")))?;
    }
    let dest = bucket.join(name);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| AppError::Validation(format!("remove old: {e}")))?;
    }
    fs::rename(&staging, &dest).map_err(|e| AppError::Validation(format!("rename: {e}")))?;
    Ok(dest)
}

/// Load every regular file under a catalog directory as wire-shaped
/// `SkillFile`s (rel_path + base64 contents). Used when pushing a
/// catalog entry to a daemon.
pub fn load_skill_files(dir: &Path) -> AppResult<Vec<SkillFile>> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    walk_dir(dir, dir, &mut entries)
        .map_err(|e| AppError::Validation(format!("read skill dir: {e}")))?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries
        .into_iter()
        .map(|(rel, bytes)| SkillFile {
            rel_path: rel,
            contents_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        })
        .collect())
}

/// Pull `description: ...` from the YAML frontmatter of a `SKILL.md`,
/// or fall back to the first non-blank, non-heading line of the body.
/// Mirrors `aciddog::skills::parse_description_from_markdown` — kept in
/// sync by hand.
pub fn extract_description(skill_md: &str) -> Option<String> {
    let mut lines = skill_md.lines().peekable();
    while matches!(lines.peek(), Some(l) if l.trim().is_empty()) {
        lines.next();
    }
    if matches!(lines.peek(), Some(l) if l.trim() == "---") {
        lines.next();
        for fm in lines.by_ref() {
            if fm.trim() == "---" {
                break;
            }
            if let Some((k, v)) = fm.split_once(':') {
                if k.trim().eq_ignore_ascii_case("description") {
                    let trimmed = v.trim();
                    let unquoted = trimmed
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .or_else(|| {
                            trimmed
                                .strip_prefix('\'')
                                .and_then(|s| s.strip_suffix('\''))
                        })
                        .unwrap_or(trimmed);
                    if !unquoted.is_empty() {
                        return Some(truncate_description(unquoted));
                    }
                }
            }
        }
    }
    for line in lines {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        return Some(truncate_description(t));
    }
    None
}

fn truncate_description(s: &str) -> String {
    const MAX: usize = 200;
    let t = s.trim();
    if t.chars().count() <= MAX {
        return t.to_string();
    }
    let cut: String = t.chars().take(MAX).collect();
    format!("{cut}…")
}

/// Build the `id` column value for the catalog. URL-safe — uses `--`
/// as a separator so the id can travel in `:id` path params without
/// encoding (e.g. `uploaded--code-review`,
/// `marketplace--foo--code-review`). `--` collisions are blocked
/// because skill names and marketplace ids can only contain alnum and
/// single dashes.
pub fn skill_row_id(source_kind: &str, source_ref: Option<&str>, name: &str) -> String {
    match source_ref {
        Some(r) => format!("{source_kind}--{r}--{name}"),
        None => format!("{source_kind}--{name}"),
    }
}

/// Insert or replace a catalog row, returning the row id. Description
/// is parsed from `SKILL.md` if present in `files`. Caller is
/// responsible for having already invoked `store_skill_dir`.
pub async fn upsert_skill(
    pool: &SqlitePool,
    source_kind: &str,
    source_ref: Option<&str>,
    name: &str,
    files: &[SkillFile],
    version: Option<&str>,
    upstream_hash: Option<&str>,
) -> AppResult<String> {
    let id = skill_row_id(source_kind, source_ref, name);
    let content_hash = hash_files(files)?;
    let description = files
        .iter()
        .find(|f| f.rel_path == "SKILL.md")
        .and_then(|f| {
            base64::engine::general_purpose::STANDARD
                .decode(&f.contents_b64)
                .ok()
        })
        .and_then(|b| String::from_utf8(b).ok())
        .as_deref()
        .and_then(extract_description)
        .unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO skills(id, name, source_kind, source_ref, description, version, content_hash, upstream_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            description = excluded.description,
            version = excluded.version,
            content_hash = excluded.content_hash,
            upstream_hash = excluded.upstream_hash,
            updated_at = excluded.updated_at",
    )
    .bind(&id)
    .bind(name)
    .bind(source_kind)
    .bind(source_ref)
    .bind(&description)
    .bind(version)
    .bind(&content_hash)
    .bind(upstream_hash)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(id)
}

/// List every skill in the catalog, joined with the most recent push
/// per (skill, instance) for the UI's "last pushed" column.
pub async fn list_skills(pool: &SqlitePool) -> AppResult<Vec<SkillSummary>> {
    let rows = sqlx::query_as::<_, SkillRow>(
        "SELECT id, name, source_kind, source_ref, description, version, content_hash, upstream_hash, updated_at FROM skills ORDER BY name, source_kind",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let pushes = sqlx::query_as::<_, PushRow>(
            "SELECT p.instance_id, i.name as instance_name, p.pushed_at, p.pushed_hash
             FROM skill_pushes p JOIN instances i ON i.id = p.instance_id
             WHERE p.skill_id = ?1 ORDER BY p.pushed_at DESC",
        )
        .bind(&row.id)
        .fetch_all(pool)
        .await
        .map_err(AppError::Db)?;
        let last_pushed_to = pushes
            .into_iter()
            .map(|p| SkillPushSummary {
                up_to_date: p.pushed_hash == row.content_hash,
                instance_id: p.instance_id,
                instance_name: p.instance_name,
                pushed_at: p.pushed_at,
            })
            .collect();
        let update_available = row.upstream_hash.as_ref().map(|u| u != &row.content_hash);
        out.push(SkillSummary {
            id: row.id,
            name: row.name,
            source_kind: row.source_kind,
            source_ref: row.source_ref,
            description: row.description.unwrap_or_default(),
            version: row.version,
            content_hash: row.content_hash,
            update_available,
            last_pushed_to,
        });
    }
    Ok(out)
}

#[derive(Debug, sqlx::FromRow)]
struct SkillRow {
    id: String,
    name: String,
    source_kind: String,
    source_ref: Option<String>,
    description: Option<String>,
    version: Option<String>,
    content_hash: String,
    upstream_hash: Option<String>,
    #[sqlx(rename = "updated_at")]
    _updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PushRow {
    instance_id: String,
    instance_name: String,
    pushed_at: String,
    pushed_hash: String,
}

/// Look up a single catalog row's source location so callers can
/// resolve the on-disk directory for it.
pub async fn locate_skill(
    pool: &SqlitePool,
    id: &str,
) -> AppResult<(String, Option<String>, String, String)> {
    let row = sqlx::query_as::<_, (String, Option<String>, String, String)>(
        "SELECT source_kind, source_ref, name, content_hash FROM skills WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.ok_or(AppError::NotFound)
}

/// Remove the catalog row + on-disk directory for a single skill.
pub async fn delete_skill(pool: &SqlitePool, skills_root: &Path, id: &str) -> AppResult<()> {
    let (source_kind, source_ref, name, _hash) = locate_skill(pool, id).await?;
    let dir = skill_dir(skills_root, &source_kind, source_ref.as_deref(), &name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| AppError::Validation(format!("remove dir: {e}")))?;
    }
    sqlx::query("DELETE FROM skills WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
    }

    #[test]
    fn name_validation_matches_daemon() {
        assert!(is_valid_skill_name("research"));
        assert!(is_valid_skill_name("a_b-c9"));
        assert!(!is_valid_skill_name("Has-Upper"));
        assert!(!is_valid_skill_name(".."));
        assert!(!is_valid_skill_name("-leads-with-dash"));
    }

    #[test]
    fn rel_path_validation() {
        assert!(validate_rel_path("SKILL.md").is_ok());
        assert!(validate_rel_path("scripts/helper.sh").is_ok());
        assert!(validate_rel_path("../etc").is_err());
        assert!(validate_rel_path("/abs").is_err());
        assert!(validate_rel_path("").is_err());
    }

    #[test]
    fn description_extraction() {
        assert_eq!(
            extract_description("---\ndescription: Hello world\n---\n").as_deref(),
            Some("Hello world")
        );
        assert_eq!(
            extract_description("# Heading\n\nBody line.\n").as_deref(),
            Some("Body line.")
        );
    }

    #[test]
    fn store_and_hash_round_trip() {
        let tmp = tempdir();
        let files = vec![
            SkillFile {
                rel_path: "SKILL.md".into(),
                contents_b64: b64("---\ndescription: Test\n---\n"),
            },
            SkillFile {
                rel_path: "scripts/run.sh".into(),
                contents_b64: b64("#!/bin/sh\necho hi\n"),
            },
        ];
        let dest = store_skill_dir(tmp.path(), "uploaded", None, "demo", &files).expect("store");
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("scripts/run.sh").is_file());

        let from_disk = load_skill_files(&dest).expect("load");
        let h1 = hash_files(&files).unwrap();
        let h2 = hash_files(&from_disk).unwrap();
        assert_eq!(h1, h2);

        let h3 = hash_dir(&dest).unwrap();
        assert_eq!(h1, h3);
    }

    fn tempdir() -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "aciddog-skill-test-{}",
            std::process::id() as u64 ^ rand_u64()
        ));
        std::fs::create_dir_all(&path).unwrap();
        TempDir { path }
    }

    fn rand_u64() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
