//! Append-only JSONL transcripts for agent / worker / automaton
//! chats. Each chat lives in
//! `.aciddog/agent-chat/<sanitized_chat_id>/<session_hash>.jsonl`
//! and accumulates one line per `WireChatBlock` written via
//! `AgentChat::push_block` (and per re-broadcast via `emit_block_at`).
//!
//! `/compact` and `/reset` close the current file and open a new one
//! under a fresh hash (the prior file is preserved). Mirrors the
//! channel-side flow in [`super::channel_log`]; the hash function is
//! re-used unchanged.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

pub use super::channel_log::session_hash;

use crate::wire_chat::WireChatBlock;

/// Strip everything except `[A-Za-z0-9_-]` from a chat id so it is
/// safe to use as a path component on every supported OS. Worker
/// display names contain `#`, which would otherwise leak out of the
/// agent-chat namespace on Windows.
fn sanitize_chat_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Directory holding all session files for one chat.
pub fn chat_dir(cwd: &Path, chat_id: &str) -> PathBuf {
    cwd.join(".aciddog")
        .join("agent-chat")
        .join(sanitize_chat_id(chat_id))
}

/// Append-only writer for one chat session. Owned by the matching
/// `AgentChat` so the writer's lifetime tracks the chat slot.
#[derive(Debug)]
pub struct ChatLog {
    file: File,
    path: PathBuf,
    /// Stored so `roll` can open the next session file in the same
    /// directory without re-deriving it from the chat state.
    dir: PathBuf,
    chat_id: String,
}

impl ChatLog {
    /// Open (creating if absent) the session file
    /// `<cwd>/.aciddog/agent-chat/<id>/<hash>.jsonl` in append mode.
    pub fn open_session(cwd: &Path, chat_id: &str, hash: &str) -> std::io::Result<Self> {
        let dir = chat_dir(cwd, chat_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{hash}.jsonl"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            file,
            path,
            dir,
            chat_id: chat_id.to_string(),
        })
    }

    /// Serialise one block as a single JSON line and append it. The
    /// whole transcript is stored verbatim — including tool spans,
    /// thinking, and system markers — so the reader gets a lossless
    /// audit trail. Mutations (a tool result resolving the `pending`
    /// flag, etc.) are captured by re-appending the updated block on
    /// every `emit_block_at` call.
    pub fn append(&mut self, block: &WireChatBlock) -> std::io::Result<()> {
        let mut serialised = serde_json::to_string(block)?;
        serialised.push('\n');
        self.file.write_all(serialised.as_bytes())?;
        self.file.flush()?;
        Ok(())
    }

    /// Roll to a new session: derive a fresh hash from
    /// `(chat_id, now)`, open the new file in the same chat directory,
    /// and switch. Returns `(now, new_hash)` so the caller can update
    /// the matching `AgentChat::session_started` / `session_hash`
    /// fields. The previous file is left untouched on disk.
    pub fn roll(&mut self, now: DateTime<Utc>) -> std::io::Result<(DateTime<Utc>, String)> {
        let hash = session_hash(&self.chat_id, now);
        let path = self.dir.join(format!("{hash}.jsonl"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.file = file;
        self.path = path;
        Ok((now, hash))
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sanitize_replaces_non_alnum_chars() {
        assert_eq!(sanitize_chat_id("planner"), "planner");
        assert_eq!(sanitize_chat_id("worker#abcd1234"), "worker_abcd1234");
        assert_eq!(sanitize_chat_id("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_chat_id("good-id_123"), "good-id_123");
    }

    #[test]
    fn open_session_creates_subdir_under_agent_chat() {
        let dir = tempdir().unwrap();
        let hash = session_hash("planner", Utc::now());
        let log = ChatLog::open_session(dir.path(), "planner", &hash).unwrap();
        let expected = dir
            .path()
            .join(".aciddog")
            .join("agent-chat")
            .join("planner")
            .join(format!("{hash}.jsonl"));
        assert_eq!(log.path(), expected);
    }

    #[test]
    fn worker_id_is_sanitized() {
        let dir = tempdir().unwrap();
        let hash = session_hash("scout#abc12345", Utc::now());
        let log = ChatLog::open_session(dir.path(), "scout#abc12345", &hash).unwrap();
        let expected = dir
            .path()
            .join(".aciddog")
            .join("agent-chat")
            .join("scout_abc12345")
            .join(format!("{hash}.jsonl"));
        assert_eq!(log.path(), expected);
    }

    #[test]
    fn append_writes_jsonl_with_kind_tag() {
        let dir = tempdir().unwrap();
        let hash = session_hash("a", Utc::now());
        let mut log = ChatLog::open_session(dir.path(), "a", &hash).unwrap();
        log.append(&WireChatBlock::UserText {
            text: "hello".into(),
        })
        .unwrap();
        let body = std::fs::read_to_string(log.path()).unwrap();
        assert_eq!(body.lines().count(), 1);
        // Tagged enum: `"kind":"user_text"` plus the text payload.
        assert!(body.contains("\"kind\":\"user_text\""));
        assert!(body.contains("\"text\":\"hello\""));
        assert!(body.ends_with('\n'));
    }

    #[test]
    fn append_emits_system_prompt_kind_tag() {
        let dir = tempdir().unwrap();
        let hash = session_hash("a", Utc::now());
        let mut log = ChatLog::open_session(dir.path(), "a", &hash).unwrap();
        log.append(&WireChatBlock::SystemPrompt {
            text: "be terse".into(),
        })
        .unwrap();
        let body = std::fs::read_to_string(log.path()).unwrap();
        assert!(body.contains("\"kind\":\"system_prompt\""));
        assert!(body.contains("\"text\":\"be terse\""));
    }

    #[test]
    fn roll_opens_new_file_under_new_hash() {
        let dir = tempdir().unwrap();
        let h1 = session_hash("a", Utc::now());
        let mut log = ChatLog::open_session(dir.path(), "a", &h1).unwrap();
        log.append(&WireChatBlock::UserText {
            text: "first".into(),
        })
        .unwrap();
        let first_path = log.path().to_path_buf();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let (_, h2) = log.roll(Utc::now()).unwrap();
        assert_ne!(h1, h2);
        assert_ne!(log.path(), first_path);
        log.append(&WireChatBlock::UserText {
            text: "second".into(),
        })
        .unwrap();
        // Both files preserved on disk.
        assert!(first_path.exists());
        assert!(log.path().exists());
        let first = std::fs::read_to_string(&first_path).unwrap();
        let second = std::fs::read_to_string(log.path()).unwrap();
        assert!(first.contains("first"));
        assert!(second.contains("second"));
        assert!(!second.contains("first"));
    }
}
