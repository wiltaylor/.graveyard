//! Append-only JSONL transcripts for channel messages. Each channel
//! lives in `.aciddog/chat/<id>/<session_hash>.jsonl`. A "session" is a
//! single contiguous run of the channel — `/reset` rolls the hash and
//! opens a new file, so the directory accumulates one file per
//! session over the channel's lifetime. On startup the TUI picks the
//! most recently modified file and `tail`s it to seed the display
//! buffer (the messages are NEVER replayed into any agent's context).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Length of the displayed/filename hash, in hex chars (48 bits).
pub const SESSION_HASH_LEN: usize = 12;

/// Compute the truncated hex hash for a `(channel_id, start_time)` pair.
/// Stable across runs given the same inputs.
pub fn session_hash(channel_id: &str, start_time: DateTime<Utc>) -> String {
    let stamp = start_time.to_rfc3339_opts(SecondsFormat::Nanos, true);
    let mut hasher = Sha256::new();
    hasher.update(channel_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(stamp.as_bytes());
    let digest = hasher.finalize();
    let mut hex_str = hex::encode(digest);
    hex_str.truncate(SESSION_HASH_LEN);
    hex_str
}

/// Directory holding all session files for a channel.
pub fn channel_dir(cwd: &Path, channel_id: &str) -> PathBuf {
    cwd.join(".aciddog").join("chat").join(channel_id)
}

/// Legacy flat path produced by the pre-session implementation.
fn legacy_path(cwd: &Path, channel_id: &str) -> PathBuf {
    cwd.join(".aciddog")
        .join("chat")
        .join(format!("{channel_id}.jsonl"))
}

/// A single archived channel post. `sender` is an agent id or the
/// literal string `"user"`; `body` is the raw message text with
/// newlines preserved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogLine {
    pub ts: DateTime<Utc>,
    pub sender: String,
    pub body: String,
}

/// Append-only writer for one channel session. Opened once when a
/// session begins and kept alive until the session ends (process exit
/// or `/reset`). Single writer, no flush task. The hash that names
/// this session lives on `ChannelState::session_hash`; this writer
/// just remembers its file handle and the path for `tail` calls.
#[derive(Debug)]
pub struct ChannelLog {
    file: File,
    path: PathBuf,
}

impl ChannelLog {
    /// Open (creating if absent) the session file
    /// `<cwd>/.aciddog/chat/<channel_id>/<hash>.jsonl` in append mode.
    pub fn open_session(cwd: &Path, channel_id: &str, hash: &str) -> std::io::Result<Self> {
        let dir = channel_dir(cwd, channel_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{hash}.jsonl"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { file, path })
    }

    /// Append one line describing a freshly-posted message. Each
    /// call does a single `write_all` + `flush`; JSONL guarantees
    /// crash-safety as long as individual writes are under the
    /// pipe buffer (messages here are tiny).
    pub fn append(&mut self, sender: &str, body: &str) -> std::io::Result<()> {
        let line = LogLine {
            ts: Utc::now(),
            sender: sender.to_string(),
            body: body.to_string(),
        };
        let mut serialised = serde_json::to_string(&line)?;
        serialised.push('\n');
        self.file.write_all(serialised.as_bytes())?;
        self.file.flush()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Find the most recently modified session file in a channel's
/// directory. Returns the file path and its hash (parsed from the
/// filename stem). `None` if the directory doesn't exist or contains
/// no `*.jsonl` files.
pub fn latest_session(cwd: &Path, channel_id: &str) -> Option<(PathBuf, String)> {
    let dir = channel_dir(cwd, channel_id);
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut best: Option<(PathBuf, String, std::time::SystemTime)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.len() != SESSION_HASH_LEN || !stem.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let stem = stem.to_string();
        match &best {
            Some((_, _, prev)) if *prev >= mtime => {}
            _ => best = Some((path, stem, mtime)),
        }
    }
    best.map(|(p, h, _)| (p, h))
}

/// One-shot migration: if a legacy `.aciddog/chat/<id>.jsonl` file
/// exists, derive a hash from `(channel_id, first-line ts || mtime)`
/// and move the file to `.aciddog/chat/<id>/<hash>.jsonl`. Returns the
/// new path + hash on success, `Ok(None)` if no legacy file was
/// present, or `Err` on IO failure. Idempotent — re-running it after
/// a successful migration is a no-op.
pub fn migrate_legacy(cwd: &Path, channel_id: &str) -> std::io::Result<Option<(PathBuf, String)>> {
    let legacy = legacy_path(cwd, channel_id);
    if !legacy.exists() {
        return Ok(None);
    }
    let stamp = first_line_ts(&legacy)
        .or_else(|| {
            std::fs::metadata(&legacy)
                .and_then(|m| m.modified())
                .ok()
                .map(DateTime::<Utc>::from)
        })
        .unwrap_or_else(Utc::now);
    let hash = session_hash(channel_id, stamp);
    let dir = channel_dir(cwd, channel_id);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(format!("{hash}.jsonl"));
    if dest.exists() {
        // Collision (extraordinarily unlikely) — keep the legacy file
        // out of the way without clobbering a real session file.
        std::fs::remove_file(&legacy)?;
        return Ok(None);
    }
    std::fs::rename(&legacy, &dest)?;
    Ok(Some((dest, hash)))
}

fn first_line_ts(path: &Path) -> Option<DateTime<Utc>> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    if reader.read_line(&mut buf).ok()? == 0 {
        return None;
    }
    let line: LogLine = serde_json::from_str(buf.trim_end()).ok()?;
    Some(line.ts)
}

/// Load up to the last `max` lines from `path`. Returns them in
/// chronological order. Used at startup to seed the channel's
/// display buffer — the result is **never** pushed into any
/// agent's context, only rendered in the TUI.
///
/// Malformed lines are skipped silently (a warning would spam
/// restart loops if someone corrupts the file by hand).
pub fn tail(path: &Path, max: usize) -> std::io::Result<Vec<LogLine>> {
    if !path.exists() || max == 0 {
        return Ok(Vec::new());
    }
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    // For small files the naive approach is fine; for big files we
    // read the last ~64KB * (max / 100) bytes to avoid scanning the
    // whole transcript. Simple heuristic: read a 128KB window.
    // If that doesn't contain `max` full lines we widen.
    const WINDOW: u64 = 128 * 1024;
    let start = size.saturating_sub(WINDOW);
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start))?;

    // If we started mid-line, drop the partial prefix.
    let mut buf = String::new();
    if start > 0 {
        let _ = reader.read_line(&mut buf);
        buf.clear();
    }

    let mut all: Vec<LogLine> = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        let trimmed = buf.trim_end_matches(&['\n', '\r'][..]);
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(line) = serde_json::from_str::<LogLine>(trimmed) {
            all.push(line);
        }
    }

    let n = all.len();
    if n > max {
        Ok(all.split_off(n - max))
    } else {
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn hash_is_stable_for_same_inputs() {
        let ts = "2026-05-02T12:00:00.000000000Z"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let a = session_hash("planning", ts);
        let b = session_hash("planning", ts);
        assert_eq!(a, b);
        assert_eq!(a.len(), SESSION_HASH_LEN);
    }

    #[test]
    fn hash_changes_when_start_time_changes() {
        let ts1 = "2026-05-02T12:00:00.000000000Z"
            .parse::<DateTime<Utc>>()
            .unwrap();
        let ts2 = "2026-05-02T12:00:00.000000001Z"
            .parse::<DateTime<Utc>>()
            .unwrap();
        assert_ne!(session_hash("planning", ts1), session_hash("planning", ts2));
    }

    #[test]
    fn hash_changes_when_channel_id_changes() {
        let ts = Utc::now();
        assert_ne!(session_hash("a", ts), session_hash("b", ts));
    }

    #[test]
    fn open_session_then_tail_roundtrip() {
        let dir = tempdir().unwrap();
        let hash = session_hash("test", Utc::now());
        let mut log = ChannelLog::open_session(dir.path(), "test", &hash).unwrap();
        log.append("alice", "hello").unwrap();
        log.append("bob", "hi there").unwrap();
        log.append("user", "line\nwith\nnewlines").unwrap();

        let loaded = tail(log.path(), 10).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].sender, "alice");
        assert_eq!(loaded[0].body, "hello");
        assert_eq!(loaded[1].sender, "bob");
        assert_eq!(loaded[2].sender, "user");
        assert_eq!(loaded[2].body, "line\nwith\nnewlines");
        assert!(log
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s == hash)
            .unwrap_or(false));
    }

    #[test]
    fn open_session_writes_under_channel_subdir() {
        let dir = tempdir().unwrap();
        let hash = session_hash("ch", Utc::now());
        let log = ChannelLog::open_session(dir.path(), "ch", &hash).unwrap();
        let expected = dir
            .path()
            .join(".aciddog")
            .join("chat")
            .join("ch")
            .join(format!("{hash}.jsonl"));
        assert_eq!(log.path(), expected);
    }

    #[test]
    fn latest_session_picks_newest_mtime() {
        let dir = tempdir().unwrap();
        let h1 = "aaaaaaaaaaaa";
        let h2 = "bbbbbbbbbbbb";
        let mut log1 = ChannelLog::open_session(dir.path(), "ch", h1).unwrap();
        log1.append("a", "older").unwrap();
        // Sleep just enough that mtime ordering is well-defined on
        // filesystems with second-granularity timestamps.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut log2 = ChannelLog::open_session(dir.path(), "ch", h2).unwrap();
        log2.append("b", "newer").unwrap();

        let (path, hash) = latest_session(dir.path(), "ch").unwrap();
        assert_eq!(hash, h2);
        assert_eq!(path, log2.path());
    }

    #[test]
    fn latest_session_skips_non_hash_filenames() {
        let dir = tempdir().unwrap();
        let chan_dir = channel_dir(dir.path(), "ch");
        std::fs::create_dir_all(&chan_dir).unwrap();
        std::fs::write(chan_dir.join("not-a-hash.jsonl"), b"").unwrap();
        std::fs::write(chan_dir.join("zzz.txt"), b"").unwrap();
        assert!(latest_session(dir.path(), "ch").is_none());
    }

    #[test]
    fn latest_session_none_for_missing_dir() {
        let dir = tempdir().unwrap();
        assert!(latest_session(dir.path(), "nope").is_none());
    }

    #[test]
    fn migrate_legacy_moves_flat_file_into_subdir() {
        let dir = tempdir().unwrap();
        let chat_dir = dir.path().join(".aciddog").join("chat");
        std::fs::create_dir_all(&chat_dir).unwrap();
        let legacy = chat_dir.join("oldname.jsonl");
        let line = LogLine {
            ts: Utc::now(),
            sender: "alice".into(),
            body: "preserved".into(),
        };
        let mut serialised = serde_json::to_string(&line).unwrap();
        serialised.push('\n');
        std::fs::write(&legacy, serialised).unwrap();

        let migrated = migrate_legacy(dir.path(), "oldname").unwrap();
        let (new_path, hash) = migrated.expect("legacy file should migrate");
        assert!(!legacy.exists(), "flat file should be moved");
        assert!(new_path.exists());
        assert_eq!(hash.len(), SESSION_HASH_LEN);
        let loaded = tail(&new_path, 10).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].body, "preserved");

        // Idempotent: a second call is a no-op since the legacy file
        // is already gone.
        assert!(migrate_legacy(dir.path(), "oldname").unwrap().is_none());
    }

    #[test]
    fn migrate_legacy_no_op_when_no_flat_file() {
        let dir = tempdir().unwrap();
        assert!(migrate_legacy(dir.path(), "nope").unwrap().is_none());
    }

    #[test]
    fn tail_respects_max() {
        let dir = tempdir().unwrap();
        let hash = session_hash("ch", Utc::now());
        let mut log = ChannelLog::open_session(dir.path(), "ch", &hash).unwrap();
        for i in 0..10 {
            log.append("s", &format!("msg {i}")).unwrap();
        }
        let last3 = tail(log.path(), 3).unwrap();
        assert_eq!(last3.len(), 3);
        assert_eq!(last3[0].body, "msg 7");
        assert_eq!(last3[1].body, "msg 8");
        assert_eq!(last3[2].body, "msg 9");
    }

    #[test]
    fn tail_skips_malformed_lines() {
        let dir = tempdir().unwrap();
        let hash = session_hash("ch", Utc::now());
        let mut log = ChannelLog::open_session(dir.path(), "ch", &hash).unwrap();
        log.append("alice", "good").unwrap();
        let path = log.path().to_path_buf();
        drop(log);
        // Corrupt the file with a junk line in between.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "not json").unwrap();
        drop(f);
        let mut log = ChannelLog::open_session(dir.path(), "ch", &hash).unwrap();
        log.append("bob", "also good").unwrap();

        let loaded = tail(&path, 10).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].sender, "alice");
        assert_eq!(loaded[1].sender, "bob");
    }

    #[test]
    fn tail_on_missing_file_is_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.jsonl");
        let loaded = tail(&path, 10).unwrap();
        assert!(loaded.is_empty());
    }
}
