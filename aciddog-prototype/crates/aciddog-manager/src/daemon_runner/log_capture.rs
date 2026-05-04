//! Manager-side stdio capture for managed daemons (sibling 3bc8a0d0).
//!
//! See ADR-0006 + sibling b4520e26 for the `.aciddog/daemon-runner.log`
//! path convention. Decoupled from `cron_event_tx` — captures raw bytes,
//! not structured events.
//!
//! Layout:
//!   * [`CapturePolicy`] — caps for the rotating writer + ring buffer,
//!     defaulted to 10MB×3 on disk and 1MB in memory.
//!   * [`Stream`] — `Stdout` or `Stderr` tag per [`CapturedLine`].
//!   * [`CapturedLine`] — one tagged line; `offset` is the byte cursor
//!     after the line (the SSE `since` cursor is byte-anchored).
//!   * [`RingBuffer`] — std-only `VecDeque` capped at `ring_bytes`.
//!   * [`RingBufferHandle`] — clonable wrapper around the ring + a
//!     `tokio::sync::broadcast::Sender<CapturedLine>` for the SSE
//!     follow path.
//!   * [`RotatingWriter`] — append + size-counter-driven rotate, never
//!     panics, never propagates errors.
//!   * [`start_capture`] — spawns the per-child fan-out task that
//!     drives stdout/stderr → ring + writer + broadcast.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::{broadcast, oneshot, Mutex};

/// Cap on the on-disk rotating log + the in-memory ring buffer.
///
/// Defaults (matching plan §CapturePolicy):
///   * `max_size_bytes` — 10 MiB before rotation triggers.
///   * `max_files` — keep `.log` + 3 historical files (`.log.1`–`.log.3`).
///   * `ring_bytes` — 1 MiB in-memory tail.
#[derive(Clone, Debug)]
pub struct CapturePolicy {
    pub max_size_bytes: u64,
    pub max_files: u8,
    pub ring_bytes: usize,
}

impl Default for CapturePolicy {
    fn default() -> Self {
        Self {
            max_size_bytes: 10 * 1024 * 1024,
            max_files: 3,
            ring_bytes: 1024 * 1024,
        }
    }
}

/// Field-by-field `Option<...>` overrides for a [`CapturePolicy`].
///
/// Stub for now — schema columns on `managed_daemons` are a follow-up.
/// Ships with zero call sites in this slice but documents the intended
/// shape so the schema bring-up can fill in `merged_with`.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct OptionalCapturePolicy {
    pub max_size_bytes: Option<u64>,
    pub max_files: Option<u8>,
    pub ring_bytes: Option<usize>,
}

impl CapturePolicy {
    /// Stub merge — returns `self.clone()` so callers can wire the API
    /// today. Real per-row overrides arrive with the schema follow-up.
    #[allow(dead_code)]
    pub fn merged_with(&self, _overrides: &OptionalCapturePolicy) -> Self {
        self.clone()
    }
}

/// Tag attached to each [`CapturedLine`] so the SSE consumer can colour
/// stdout vs stderr without sniffing the body.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    /// `"stdout"` / `"stderr"` for the rotating writer's prefix.
    fn as_str(self) -> &'static str {
        match self {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        }
    }
}

/// One captured line. `offset` is the byte cursor *after* this line —
/// i.e. the value the SSE `Last-Event-ID` header carries on reconnect.
#[derive(Clone, Debug)]
pub struct CapturedLine {
    pub ts: DateTime<Utc>,
    pub stream: Stream,
    /// Monotonic byte offset (cumulative bytes written across the
    /// child's lifetime) AT THE END of this line. The SSE `since`
    /// cursor is therefore byte-anchored and an SSE consumer can resume
    /// without re-receiving partial payloads.
    pub offset: u64,
    pub bytes: Bytes,
}

/// Std-only ring buffer holding the in-memory tail of captured lines.
///
/// Eviction is byte-bound: pushing a line drops the oldest entries
/// until `total_bytes + line.bytes.len() <= ring_bytes`. The monotonic
/// `next_offset` is tracked across evictions so the SSE `since` cursor
/// can detect a gap (`since < oldest.offset`) and emit a `gap` event.
pub struct RingBuffer {
    entries: VecDeque<CapturedLine>,
    total_bytes: usize,
    ring_bytes: usize,
    next_offset: u64,
}

impl RingBuffer {
    pub fn new(ring_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            total_bytes: 0,
            ring_bytes,
            next_offset: 0,
        }
    }

    /// Append `bytes`/`stream`/`ts` as a new line, returning the
    /// fully-populated [`CapturedLine`] (with monotonic offset).
    ///
    /// Drops the oldest entries until the new line fits, even if
    /// `bytes.len() > ring_bytes` (in which case the buffer is left
    /// empty and the line is still appended — callers expect the
    /// returned line to be enqueued so the SSE follow path can fan
    /// it out).
    pub fn push(&mut self, ts: DateTime<Utc>, stream: Stream, bytes: Bytes) -> CapturedLine {
        let len = bytes.len();
        // Bump the monotonic offset BEFORE inserting so the line's
        // recorded offset is the cursor at-or-after the last byte.
        self.next_offset = self.next_offset.saturating_add(len as u64);
        let line = CapturedLine {
            ts,
            stream,
            offset: self.next_offset,
            bytes,
        };
        // Evict until the new line fits.
        while !self.entries.is_empty() && self.total_bytes + len > self.ring_bytes {
            if let Some(dropped) = self.entries.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(dropped.bytes.len());
            } else {
                break;
            }
        }
        // If `len` alone exceeds `ring_bytes`, we still record the line
        // (the writer + broadcaster path is the source of truth; the
        // ring just won't carry oversized history).
        self.entries.push_back(line.clone());
        self.total_bytes = self.total_bytes.saturating_add(len);
        line
    }

    /// Retrieve all entries with `offset > since` (i.e. lines whose end
    /// cursor sits after the caller's cursor). When `since` is older
    /// than the oldest retained entry's offset, returns
    /// `(entries, Some(<oldest_dropped_until>))` so the SSE handler
    /// can emit a `gap` event.
    pub fn since(&self, since: u64) -> (Vec<CapturedLine>, Option<u64>) {
        if self.entries.is_empty() {
            return (Vec::new(), None);
        }
        // The earliest still-retrievable cursor is the offset of the
        // oldest entry MINUS that entry's length (i.e. the start
        // cursor). If `since` is below that, we lost lines.
        let oldest = self.entries.front().expect("non-empty");
        let oldest_start = oldest.offset.saturating_sub(oldest.bytes.len() as u64);
        let gap = if since < oldest_start {
            Some(oldest_start)
        } else {
            None
        };
        let entries = self
            .entries
            .iter()
            .filter(|e| e.offset > since)
            .cloned()
            .collect();
        (entries, gap)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Clone-able façade around a shared [`RingBuffer`] plus a broadcast
/// channel that fans new lines out to any active SSE subscriber.
#[derive(Clone)]
pub struct RingBufferHandle {
    inner: Arc<Mutex<RingBuffer>>,
    broadcast_tx: broadcast::Sender<CapturedLine>,
}

impl RingBufferHandle {
    pub fn new(ring_bytes: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        Self {
            inner: Arc::new(Mutex::new(RingBuffer::new(ring_bytes))),
            broadcast_tx,
        }
    }

    /// Append a line to the ring buffer and fan it out to every active
    /// subscriber. Subscribers that have lagged are dropped by the
    /// channel; the broadcast `send` result is intentionally ignored.
    pub async fn push(&self, ts: DateTime<Utc>, stream: Stream, bytes: Bytes) -> CapturedLine {
        let mut guard = self.inner.lock().await;
        let line = guard.push(ts, stream, bytes);
        drop(guard);
        let _ = self.broadcast_tx.send(line.clone());
        line
    }

    /// Snapshot of all entries with `offset > since`, plus an optional
    /// `gap` cursor (set when `since` lies before the ring's earliest
    /// retained line). See [`RingBuffer::since`].
    pub async fn since(&self, since: u64) -> (Vec<CapturedLine>, Option<u64>) {
        let guard = self.inner.lock().await;
        guard.since(since)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CapturedLine> {
        self.broadcast_tx.subscribe()
    }

    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        let guard = self.inner.lock().await;
        guard.len()
    }

    #[allow(dead_code)]
    pub async fn is_empty(&self) -> bool {
        let guard = self.inner.lock().await;
        guard.is_empty()
    }
}

/// Append-only writer driven by an in-memory size counter. Rotates by
/// renaming files at the configured threshold. Never panics, never
/// propagates errors; rotation/IO failures emit a `tracing::error!`
/// and capture continues.
pub struct RotatingWriter {
    primary: PathBuf,
    max_size_bytes: u64,
    max_files: u8,
    file: Option<File>,
    current_size: u64,
}

impl RotatingWriter {
    /// Open (or create) `<dir>/.aciddog/daemon-runner.log` for append.
    /// Creates `<dir>/.aciddog/` if missing.
    pub async fn open(dir: &Path, policy: &CapturePolicy) -> std::io::Result<Self> {
        let inner = dir.join(".aciddog");
        tokio::fs::create_dir_all(&inner).await?;
        let primary = inner.join("daemon-runner.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&primary)
            .await?;
        // Best-effort current-size read so a manager restart doesn't
        // immediately rotate a half-full file.
        let current_size = match tokio::fs::metadata(&primary).await {
            Ok(meta) => meta.len(),
            Err(_) => 0,
        };
        Ok(Self {
            primary,
            max_size_bytes: policy.max_size_bytes,
            max_files: policy.max_files,
            file: Some(file),
            current_size,
        })
    }

    /// Append `<rfc3339> <stream> <line-bytes>\n` to the current file.
    /// All I/O errors are swallowed and logged.
    pub async fn write(&mut self, line: &CapturedLine) {
        let payload = format!(
            "{ts} {stream} {body}\n",
            ts = line.ts.to_rfc3339(),
            stream = line.stream.as_str(),
            body = String::from_utf8_lossy(&line.bytes).trim_end_matches('\n'),
        );
        let bytes = payload.as_bytes();
        let len = bytes.len() as u64;

        if let Some(file) = self.file.as_mut() {
            if let Err(e) = file.write_all(bytes).await {
                tracing::error!(error = %e, path = %self.primary.display(), "rotating-writer: write failed");
                return;
            }
            self.current_size = self.current_size.saturating_add(len);
        } else {
            tracing::error!(path = %self.primary.display(), "rotating-writer: no file open, skipping write");
            return;
        }

        if self.current_size >= self.max_size_bytes {
            self.rotate().await;
        }
    }

    async fn rotate(&mut self) {
        // Close the current file before renaming so the OS sees the
        // final size and the rename is atomic w.r.t. the writer.
        if let Some(mut f) = self.file.take() {
            let _ = f.flush().await;
            let _ = f.shutdown().await;
        }

        // rename .log.(N-1) -> .log.N for N down to 1, skipping
        // missing entries.
        // We keep up to `max_files` historical files, so the highest
        // index we WRITE TO is `max_files`. Anything past that is
        // pruned.
        let max = self.max_files as usize;
        // Walk from max down to 1: rename .log.(N-1) -> .log.N.
        // Treat .log.0 as the primary (unrotated); skip that step.
        for n in (2..=max).rev() {
            let src = path_with_ext(&self.primary, n - 1);
            let dst = path_with_ext(&self.primary, n);
            match tokio::fs::rename(&src, &dst).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::error!(error = %e, src = %src.display(), dst = %dst.display(), "rotating-writer: rename failed");
                }
            }
        }

        if max >= 1 {
            // Promote .log -> .log.1
            let dst = path_with_ext(&self.primary, 1);
            match tokio::fs::rename(&self.primary, &dst).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::error!(error = %e, src = %self.primary.display(), dst = %dst.display(), "rotating-writer: rename primary failed");
                }
            }
        }

        // Best-effort prune anything past `max_files`.
        let mut n = max + 1;
        loop {
            let candidate = path_with_ext(&self.primary, n);
            match tokio::fs::remove_file(&candidate).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => {
                    tracing::error!(error = %e, path = %candidate.display(), "rotating-writer: prune failed");
                    break;
                }
            }
            n += 1;
            if n > max + 32 {
                // Hard backstop — we never want to loop forever if some
                // perms-edge keeps `remove_file` returning `Ok` somehow.
                break;
            }
        }

        // Re-open the primary fresh.
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.primary)
            .await
        {
            Ok(f) => {
                self.file = Some(f);
                self.current_size = 0;
            }
            Err(e) => {
                tracing::error!(error = %e, path = %self.primary.display(), "rotating-writer: re-open after rotate failed");
                self.file = None;
                // Reset the counter so we don't try to rotate again on
                // every subsequent write. A future write will retry the
                // open.
                self.current_size = 0;
            }
        }
    }
}

/// `<base>.<n>` (e.g. `daemon-runner.log.1`).
fn path_with_ext(base: &Path, n: usize) -> PathBuf {
    let mut s = base.as_os_str().to_owned();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

/// Spawn the per-child fan-out task. Takes ownership of the child's
/// stdout/stderr pipes (which `process::spawn_child` must have set up
/// via `Stdio::piped()`), runs both readers under a single
/// `tokio::select!` so a chatty stderr can never starve stdout, and
/// fans every line into the rotating writer + ring buffer
/// (broadcasting via the handle).
///
/// Returns:
///   * a clonable [`RingBufferHandle`] for the route layer,
///   * a `oneshot::Sender<()>` shutdown signal (held by the runner so
///     `Drop`/`stop` reliably ends capture even before the child's
///     pipes EOF), and
///   * the [`Child`] (with `stdout`/`stderr` taken) so the caller can
///     keep tracking lifecycle (`try_wait`, `wait`, etc.).
pub fn start_capture(
    work_dir: &Path,
    policy: CapturePolicy,
    mut child: Child,
) -> (RingBufferHandle, oneshot::Sender<()>, Child) {
    let handle = RingBufferHandle::new(policy.ring_bytes);
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let work_dir = work_dir.to_path_buf();
    let handle_for_task = handle.clone();
    let policy_for_task = policy.clone();

    tokio::spawn(async move {
        // Open the rotating writer up-front; if it fails, log and
        // continue (the broadcast + ring still serve SSE consumers).
        let mut writer = match RotatingWriter::open(&work_dir, &policy_for_task).await {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::error!(error = %e, work_dir = %work_dir.display(), "log_capture: open rotating writer failed");
                None
            }
        };

        let mut stdout_reader = stdout.map(BufReader::new);
        let mut stderr_reader = stderr.map(BufReader::new);
        let mut stdout_buf = Vec::with_capacity(256);
        let mut stderr_buf = Vec::with_capacity(256);
        let mut stdout_eof = stdout_reader.is_none();
        let mut stderr_eof = stderr_reader.is_none();

        loop {
            if stdout_eof && stderr_eof {
                break;
            }
            tokio::select! {
                _ = &mut shutdown_rx => {
                    break;
                }
                read = read_line(&mut stdout_reader, &mut stdout_buf), if !stdout_eof => {
                    match read {
                        Ok(0) => stdout_eof = true,
                        Ok(_) => {
                            let bytes = Bytes::copy_from_slice(&stdout_buf);
                            stdout_buf.clear();
                            let line = handle_for_task.push(Utc::now(), Stream::Stdout, bytes).await;
                            if let Some(w) = writer.as_mut() {
                                w.write(&line).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "log_capture: stdout read failed");
                            stdout_eof = true;
                        }
                    }
                }
                read = read_line(&mut stderr_reader, &mut stderr_buf), if !stderr_eof => {
                    match read {
                        Ok(0) => stderr_eof = true,
                        Ok(_) => {
                            let bytes = Bytes::copy_from_slice(&stderr_buf);
                            stderr_buf.clear();
                            let line = handle_for_task.push(Utc::now(), Stream::Stderr, bytes).await;
                            if let Some(w) = writer.as_mut() {
                                w.write(&line).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "log_capture: stderr read failed");
                            stderr_eof = true;
                        }
                    }
                }
            }
        }

        if let Some(mut w) = writer.take() {
            if let Some(f) = w.file.as_mut() {
                let _ = f.flush().await;
            }
        }
    });

    (handle, shutdown_tx, child)
}

/// Helper to drive `read_until('\n', ...)` on an `Option<BufReader<...>>`.
/// Returns `Ok(0)` on EOF (or when the reader is absent).
async fn read_line<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut Option<BufReader<R>>,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    match reader.as_mut() {
        Some(r) => r.read_until(b'\n', buf).await,
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn line_bytes(n: usize) -> Bytes {
        Bytes::from(vec![b'A'; n])
    }

    // --- Ring buffer ----------------------------------------------------

    #[test]
    fn ring_drops_oldest_when_full() {
        // 1MB ring, push 2MB of 100-byte lines — the buffer must cap at
        // <=1MB and the oldest lines must drop.
        let mut ring = RingBuffer::new(1024 * 1024);
        let line_size: usize = 100;
        let total: usize = 2 * 1024 * 1024;
        let count = total / line_size;
        let mut last_offset = 0u64;
        for _ in 0..count {
            let line = ring.push(now(), Stream::Stdout, line_bytes(line_size));
            assert!(
                line.offset > last_offset,
                "offsets must be monotonically increasing"
            );
            last_offset = line.offset;
        }
        assert!(
            ring.total_bytes <= 1024 * 1024,
            "total_bytes must cap at ring_bytes (got {})",
            ring.total_bytes
        );
        // Sanity: the buffer shouldn't be empty.
        assert!(!ring.is_empty());
    }

    #[test]
    fn ring_offset_resume_returns_only_new_entries() {
        // Push 5 lines, ask for entries strictly after line 3's
        // recorded offset. We expect lines 4 and 5 only.
        let mut ring = RingBuffer::new(1024 * 1024);
        let mut offsets = Vec::new();
        for _ in 0..5 {
            let l = ring.push(now(), Stream::Stdout, line_bytes(100));
            offsets.push(l.offset);
        }
        let cursor = offsets[2]; // offset AT END of line 3.
        let (entries, gap) = ring.since(cursor);
        assert_eq!(gap, None, "no gap when cursor is inside the ring");
        assert_eq!(entries.len(), 2, "expected lines 4 + 5 only");
        assert_eq!(entries[0].offset, offsets[3]);
        assert_eq!(entries[1].offset, offsets[4]);
    }

    #[test]
    fn ring_since_before_oldest_returns_gap_marker_and_oldest_onwards() {
        // Fill a ring, then push more so the oldest is evicted. since(0)
        // must return a gap + the surviving entries.
        let mut ring = RingBuffer::new(1024); // tiny so we evict fast
        for _ in 0..30 {
            ring.push(now(), Stream::Stdout, line_bytes(100));
        }
        let (entries, gap) = ring.since(0);
        let gap_offset = gap.expect("expected a gap marker");
        assert!(gap_offset > 0, "gap marker should sit past byte 0");
        assert!(!entries.is_empty(), "surviving entries must be returned");
        // Every returned entry's offset should be > 0 and the first
        // returned entry should be the oldest still-retained line.
        for e in &entries {
            assert!(e.offset > 0);
        }
    }

    // --- Rotation -------------------------------------------------------

    #[tokio::test]
    async fn rotation_triggers_at_size_threshold() {
        // max_files=3 keeps `.log` + 3 historical files
        // (`.log.1`..`.log.3`). 3 rotations therefore must produce
        // `.log` (freshly empty) + `.log.{1,2,3}`, and `.log.4`
        // must NOT exist.
        let dir = tempfile::tempdir().unwrap();
        let policy = CapturePolicy {
            max_size_bytes: 1024,
            max_files: 3,
            ring_bytes: 4096,
        };
        let mut writer = RotatingWriter::open(dir.path(), &policy).await.unwrap();
        for _ in 0..3 {
            let line = CapturedLine {
                ts: now(),
                stream: Stream::Stdout,
                offset: 0,
                bytes: line_bytes(1100),
            };
            writer.write(&line).await;
        }
        let primary = dir.path().join(".aciddog/daemon-runner.log");
        let log1 = dir.path().join(".aciddog/daemon-runner.log.1");
        let log2 = dir.path().join(".aciddog/daemon-runner.log.2");
        let log3 = dir.path().join(".aciddog/daemon-runner.log.3");
        let log4 = dir.path().join(".aciddog/daemon-runner.log.4");
        assert!(primary.exists(), "primary log must exist");
        assert!(log1.exists(), ".log.1 must exist after first rotation");
        assert!(log2.exists(), ".log.2 must exist after second rotation");
        assert!(log3.exists(), ".log.3 must exist after third rotation");
        assert!(
            !log4.exists(),
            ".log.4 must NOT exist (max_files=3 caps the highest historical index at 3)"
        );
    }

    #[tokio::test]
    async fn rotation_keeps_at_most_max_files() {
        let dir = tempfile::tempdir().unwrap();
        let policy = CapturePolicy {
            max_size_bytes: 1024,
            max_files: 2,
            ring_bytes: 4096,
        };
        let mut writer = RotatingWriter::open(dir.path(), &policy).await.unwrap();
        for _ in 0..5 {
            let line = CapturedLine {
                ts: now(),
                stream: Stream::Stderr,
                offset: 0,
                bytes: line_bytes(1100),
            };
            writer.write(&line).await;
        }
        let log3 = dir.path().join(".aciddog/daemon-runner.log.3");
        let log4 = dir.path().join(".aciddog/daemon-runner.log.4");
        assert!(
            !log3.exists(),
            "max_files=2 must cap the highest historical index at 2"
        );
        assert!(!log4.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rotation_survives_io_error_during_rename() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let policy = CapturePolicy {
            max_size_bytes: 1024,
            max_files: 3,
            ring_bytes: 4096,
        };
        let mut writer = RotatingWriter::open(dir.path(), &policy).await.unwrap();
        // First rotation establishes .log.1.
        let big = CapturedLine {
            ts: now(),
            stream: Stream::Stdout,
            offset: 0,
            bytes: line_bytes(1100),
        };
        writer.write(&big).await;
        // Now lock the inner directory r-x so subsequent renames fail.
        let inner = dir.path().join(".aciddog");
        let mut perms = std::fs::metadata(&inner).unwrap().permissions();
        let original = perms.mode();
        perms.set_mode(0o500);
        std::fs::set_permissions(&inner, perms).unwrap();

        // Force another rotation. The rename will fail; capture must
        // continue (the writer must not panic).
        for _ in 0..3 {
            writer.write(&big).await;
        }

        // Restore perms so tempdir cleanup succeeds.
        let mut restore = std::fs::metadata(&inner).unwrap().permissions();
        restore.set_mode(original);
        let _ = std::fs::set_permissions(&inner, restore);
    }
}
