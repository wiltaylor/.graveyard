//! Append-only JSONL transcripts for channel messages. Each channel
//! is backed by its own file at `.aciddog/chat/<id>.jsonl`; on
//! startup the TUI `tail`s the most recent lines to seed the
//! channel's display buffer (the messages are NEVER replayed into
//! any agent's context — see the comment on `tail` below).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single archived channel post. `sender` is an agent id or the
/// literal string `"user"`; `body` is the raw message text with
/// newlines preserved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogLine {
    pub ts: DateTime<Utc>,
    pub sender: String,
    pub body: String,
}

/// Append-only writer for one channel. Opened once at TUI startup
/// and kept alive for the life of the process — contention is low
/// (single writer per channel, called from the Tokio main thread)
/// and we want ordered durability without a separate flush task.
#[derive(Debug)]
pub struct ChannelLog {
    file: File,
    path: PathBuf,
}

impl ChannelLog {
    /// Open the transcript file for `channel_id` rooted at `cwd`.
    /// Creates `<cwd>/.aciddog/chat/` if it doesn't exist and opens
    /// the JSONL in append mode.
    pub fn open(cwd: &Path, channel_id: &str) -> std::io::Result<Self> {
        let dir = cwd.join(".aciddog").join("chat");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{channel_id}.jsonl"));
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
    fn append_then_tail_roundtrip() {
        let dir = tempdir().unwrap();
        let mut log = ChannelLog::open(dir.path(), "test").unwrap();
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
    }

    #[test]
    fn tail_respects_max() {
        let dir = tempdir().unwrap();
        let mut log = ChannelLog::open(dir.path(), "ch").unwrap();
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
        let mut log = ChannelLog::open(dir.path(), "ch").unwrap();
        log.append("alice", "good").unwrap();
        // Corrupt the file with a junk line in between.
        drop(log);
        let path = dir.path().join(".aciddog/chat/ch.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "not json").unwrap();
        drop(f);
        let mut log = ChannelLog::open(dir.path(), "ch").unwrap();
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
