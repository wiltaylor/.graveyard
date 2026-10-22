//! `aciddog info`: a snapshot of "now" the orchestrator is willing to
//! commit to. Agents call this before scheduling a cron job so they
//! don't have to guess what `--tz local` resolves to.
//!
//! Returns a small multi-line table:
//!
//! ```text
//! local_time: 2026-04-22T14:35:07+10:00
//! utc_time:   2026-04-22T04:35:07Z
//! system_tz:  Australia/Brisbane
//! offset:     UTC+10:00
//! unix_epoch: 1776832507
//! ```
//!
//! When the IANA tz cannot be detected (rare on Linux/macOS) the
//! `system_tz` line falls back to `unknown` and only the offset stays.

use chrono::{DateTime, Local, Utc};

pub fn render(now_utc: DateTime<Utc>) -> String {
    let now_local = now_utc.with_timezone(&Local);
    let tz = iana_time_zone::get_timezone().unwrap_or_else(|_| "unknown".to_string());
    let mut out = String::new();
    out.push_str(&format!(
        "local_time: {}\n",
        now_local.format("%Y-%m-%dT%H:%M:%S%:z")
    ));
    out.push_str(&format!(
        "utc_time:   {}\n",
        now_utc.format("%Y-%m-%dT%H:%M:%SZ")
    ));
    out.push_str(&format!("system_tz:  {tz}\n"));
    out.push_str(&format!("offset:     UTC{}\n", now_local.format("%:z")));
    out.push_str(&format!("unix_epoch: {}\n", now_utc.timestamp()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn render_includes_local_and_utc_lines() {
        let now = Utc.with_ymd_and_hms(2026, 4, 22, 4, 35, 7).unwrap();
        let s = render(now);
        assert!(s.contains("local_time:"), "missing local_time: {s}");
        assert!(s.contains("utc_time:"), "missing utc_time: {s}");
        assert!(
            s.contains("2026-04-22T04:35:07Z"),
            "missing utc instant: {s}"
        );
        assert!(s.contains("unix_epoch:"), "missing unix_epoch: {s}");
    }

    #[test]
    fn render_includes_offset_line() {
        let now = Utc::now();
        let s = render(now);
        assert!(s.contains("offset:"), "missing offset: {s}");
        assert!(s.contains("system_tz:"), "missing system_tz: {s}");
    }
}
