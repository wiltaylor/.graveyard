//! OSC 52 clipboard writer.
//!
//! Pushes a payload to the system clipboard via the terminal emulator, so it
//! works over ssh and inside tmux. Each terminal needs it enabled:
//! - tmux:     `set -g set-clipboard on`
//! - xterm:    `*allowWindowOps: true`
//! - kitty:    `clipboard_control write-clipboard write-primary`
//! - alacritty / wezterm / foot / ghostty: on by default

use std::io::{self, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Write an OSC 52 sequence copying `payload` to the system clipboard.
pub fn osc52_copy(payload: &str) -> io::Result<()> {
    let seq = osc52_sequence(payload);
    let mut stdout = io::stdout().lock();
    stdout.write_all(seq.as_bytes())?;
    stdout.flush()
}

/// Build the raw OSC 52 escape sequence. Split out for unit testing.
pub fn osc52_sequence(payload: &str) -> String {
    let b64 = STANDARD.encode(payload.as_bytes());
    format!("\x1b]52;c;{b64}\x07")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_wraps_base64_payload() {
        let seq = osc52_sequence("hello");
        assert!(seq.starts_with("\x1b]52;c;"));
        assert!(seq.ends_with('\x07'));
        let b64 = &seq[7..seq.len() - 1];
        let decoded = STANDARD.decode(b64).unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn empty_payload_is_valid() {
        let seq = osc52_sequence("");
        assert_eq!(seq, "\x1b]52;c;\x07");
    }

    #[test]
    fn unicode_payload_round_trips() {
        let seq = osc52_sequence("héllo → world");
        let b64 = &seq[7..seq.len() - 1];
        let decoded = STANDARD.decode(b64).unwrap();
        assert_eq!(decoded, "héllo → world".as_bytes());
    }
}
