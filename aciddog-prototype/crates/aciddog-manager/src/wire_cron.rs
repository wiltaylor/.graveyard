//! Deserialize mirror of `aciddog::cron::{WireCronJob, CronPayload,
//! CronSchedule, CronTz, CronEvent}`.
//!
//! Same rationale as [`super::wire_tasks`] / [`super::wire_workflow`]:
//! the manager crate cannot depend on the `aciddog` crate because
//! both transitively link `sqlite3`. Shapes duplicated here must stay
//! in lockstep with `crates/aciddog/src/cron/mod.rs`.
//!
//! Unlike the canonical types in the daemon, these wire types don't
//! use `deny_unknown_fields` — a newer daemon adding a field must not
//! break an older manager that still only knows the fields it saw at
//! compile time.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireCronTz {
    Utc,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WireCronPayload {
    Message { body: String },
    SlashCommand { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WireCronSchedule {
    Recurring { expression: String },
    OneOff { run_at_local: NaiveDateTime },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireCronJob {
    pub id: Uuid,
    pub target_agent: String,
    pub payload: WireCronPayload,
    pub schedule: WireCronSchedule,
    pub timezone: WireCronTz,
    pub catchup_on_startup: bool,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
}

/// Live event fan-out shape. Matches `aciddog::cron::CronEvent` —
/// used by the `/v1/cron/events` SSE stream the manager forwards.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireCronEvent {
    Created { job: WireCronJob },
    Updated { job: WireCronJob },
    Removed { id: Uuid },
    Fired { id: Uuid, at: DateTime<Utc> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_created_roundtrips() {
        let wire = r#"{"kind":"removed","id":"00000000-0000-0000-0000-000000000000"}"#;
        let parsed: WireCronEvent = serde_json::from_str(wire).unwrap();
        assert!(matches!(parsed, WireCronEvent::Removed { .. }));
    }

    #[test]
    fn payload_tag_round_trips() {
        let p = WireCronPayload::SlashCommand {
            command: "/noop".into(),
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"kind\":\"slash_command\""), "got: {s}");
        let back: WireCronPayload = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn schedule_tag_round_trips() {
        let s = WireCronSchedule::Recurring {
            expression: "*/5 * * * *".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"kind\":\"recurring\""), "got: {j}");
        let back: WireCronSchedule = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }
}
