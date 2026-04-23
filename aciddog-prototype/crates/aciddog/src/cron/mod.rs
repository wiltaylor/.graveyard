//! Cron scheduling for the aciddog team.
//!
//! Agents schedule recurring or one-off payloads (chat messages or
//! slash commands) to be delivered to a target agent. Jobs persist to
//! `.aciddog/cron.json`. The scheduler is event-driven: the
//! orchestrator sleeps until the next due `next_run` (a
//! `DateTime<Utc>`) and a `tokio::sync::Notify` is woken whenever a
//! mutation could change the schedule.
//!
//! Time zones: each job carries [`CronTz`] (`Utc` or `Local`).
//! `Local` always means the timezone of the box running aciddog
//! (resolved via `chrono::Local`). `next_run` is always stored as
//! `Utc` so the scheduler is tz-agnostic.

use std::path::PathBuf;
use std::str::FromStr;

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CronTz {
    Utc,
    Local,
}

impl CronTz {
    pub fn parse(s: &str) -> Result<Self, CronError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "utc" => Ok(Self::Utc),
            "local" => Ok(Self::Local),
            other => Err(CronError::InvalidTz(other.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Utc => "utc",
            Self::Local => "local",
        }
    }
}

/// What gets delivered to the target agent when the job fires.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum CronPayload {
    /// Wrapped on the recipient side as `<Message from="cron:...">body</Message>`.
    Message { body: String },
    /// Run on the target chat exactly as if the user typed it.
    /// Includes the leading `/`. `/exit` and `/quit` are blocklisted.
    SlashCommand { command: String },
}

/// Recurring (5-field POSIX cron) or one-off (wall-clock instant).
/// The wall-clock fields are interpreted in the job's [`CronTz`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum CronSchedule {
    Recurring { expression: String },
    OneOff { run_at_local: NaiveDateTime },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CronJob {
    pub id: Uuid,
    pub target_agent: String,
    pub payload: CronPayload,
    pub schedule: CronSchedule,
    pub timezone: CronTz,
    pub catchup_on_startup: bool,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    /// Always stored as UTC. `None` means "will never fire" (disabled
    /// or unreachable schedule).
    pub next_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum CronError {
    #[error("cron job {0} not found")]
    NotFound(Uuid),

    #[error("invalid timezone `{0}` (expected `utc` or `local`)")]
    InvalidTz(String),

    #[error("invalid cron expression: {0}")]
    InvalidExpression(String),

    #[error("invalid one-off datetime: {0}")]
    InvalidDatetime(String),

    #[error("schedule will never fire")]
    UnreachableSchedule,

    #[error(
        "one-off datetime falls in a DST gap or is ambiguous; \
         use --tz utc to schedule by absolute instant"
    )]
    AmbiguousLocalDatetime,

    #[error("payload body must not be empty")]
    EmptyPayload,

    #[error("slash commands must start with `/`")]
    InvalidSlashCommand,

    #[error("`/exit`, `/quit`, and `/shutdown` are blocked as cron payloads")]
    BlockedSlashCommand,

    #[error("target agent must not be empty")]
    EmptyTarget,

    #[error("cron store I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("cron store parse error at {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Default, Clone)]
pub struct CronFilter {
    pub target: Option<String>,
    pub include_disabled: bool,
    pub created_by: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct CronPatch {
    pub target_agent: Option<String>,
    pub payload: Option<CronPayload>,
    pub schedule: Option<CronSchedule>,
    pub timezone: Option<CronTz>,
    pub catchup_on_startup: Option<bool>,
    pub enabled: Option<bool>,
}

/// Summary of mutations applied during [`CronStore::reconcile_on_startup`].
#[derive(Debug, Default, Clone)]
pub struct ReconcileReport {
    /// Past one-off jobs without `catchup_on_startup` — pruned.
    pub removed_one_offs: Vec<Uuid>,
    /// Past one-off jobs with `catchup_on_startup` — `next_run` set to now.
    pub queued_one_offs: Vec<Uuid>,
    /// Recurring jobs whose `next_run` was recomputed from now.
    pub recomputed_recurring: Vec<Uuid>,
    /// Recurring jobs whose schedule is unreachable — disabled.
    pub disabled_unreachable: Vec<Uuid>,
}

#[derive(Debug, Default)]
pub struct CronStore {
    jobs: Vec<CronJob>,
    path: Option<PathBuf>,
}

impl CronStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the store backed by `path`. Missing file → empty store.
    pub fn load_or_init(path: PathBuf) -> Result<Self, CronError> {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let jobs: Vec<CronJob> =
                    serde_json::from_slice(&bytes).map_err(|e| CronError::Parse {
                        path: path.display().to_string(),
                        source: e,
                    })?;
                Ok(Self {
                    jobs,
                    path: Some(path),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                jobs: Vec::new(),
                path: Some(path),
            }),
            Err(e) => Err(CronError::Io {
                path: path.display().to_string(),
                source: e,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        target_agent: String,
        payload: CronPayload,
        schedule: CronSchedule,
        timezone: CronTz,
        catchup_on_startup: bool,
        enabled: bool,
        created_by: String,
    ) -> Result<&CronJob, CronError> {
        let target = target_agent.trim().to_string();
        if target.is_empty() {
            return Err(CronError::EmptyTarget);
        }
        validate_payload(&payload)?;
        validate_schedule(&schedule)?;
        let now = Utc::now();
        let next_run = compute_next_run(&schedule, timezone, now)?;
        let job = CronJob {
            id: Uuid::new_v4(),
            target_agent: target,
            payload,
            schedule,
            timezone,
            catchup_on_startup,
            enabled,
            created_by,
            created_at: now,
            last_run: None,
            next_run: if enabled { Some(next_run) } else { None },
        };
        self.jobs.push(job);
        self.persist()?;
        Ok(self.jobs.last().unwrap())
    }

    pub fn get(&self, id: Uuid) -> Option<&CronJob> {
        self.jobs.iter().find(|j| j.id == id)
    }

    pub fn list(&self, filter: &CronFilter) -> Vec<&CronJob> {
        self.jobs
            .iter()
            .filter(|j| {
                if !filter.include_disabled && !j.enabled {
                    return false;
                }
                if let Some(t) = &filter.target {
                    if j.target_agent != *t {
                        return false;
                    }
                }
                if let Some(cb) = &filter.created_by {
                    if j.created_by != *cb {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn update(&mut self, id: Uuid, patch: CronPatch) -> Result<&CronJob, CronError> {
        let pos = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(CronError::NotFound(id))?;

        // Build the post-patch shape, validate, then apply.
        let mut next_target = self.jobs[pos].target_agent.clone();
        let mut next_payload = self.jobs[pos].payload.clone();
        let mut next_schedule = self.jobs[pos].schedule.clone();
        let mut next_tz = self.jobs[pos].timezone;
        let mut next_catchup = self.jobs[pos].catchup_on_startup;
        let mut next_enabled = self.jobs[pos].enabled;

        if let Some(t) = patch.target_agent {
            let t = t.trim().to_string();
            if t.is_empty() {
                return Err(CronError::EmptyTarget);
            }
            next_target = t;
        }
        if let Some(p) = patch.payload {
            validate_payload(&p)?;
            next_payload = p;
        }
        if let Some(s) = patch.schedule {
            validate_schedule(&s)?;
            next_schedule = s;
        }
        if let Some(tz) = patch.timezone {
            next_tz = tz;
        }
        if let Some(c) = patch.catchup_on_startup {
            next_catchup = c;
        }
        if let Some(en) = patch.enabled {
            next_enabled = en;
        }

        let next_run = if next_enabled {
            Some(compute_next_run(&next_schedule, next_tz, Utc::now())?)
        } else {
            None
        };

        let j = &mut self.jobs[pos];
        j.target_agent = next_target;
        j.payload = next_payload;
        j.schedule = next_schedule;
        j.timezone = next_tz;
        j.catchup_on_startup = next_catchup;
        j.enabled = next_enabled;
        j.next_run = next_run;
        self.persist()?;
        Ok(&self.jobs[pos])
    }

    pub fn remove(&mut self, id: Uuid) -> Result<CronJob, CronError> {
        let pos = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(CronError::NotFound(id))?;
        let removed = self.jobs.remove(pos);
        self.persist()?;
        Ok(removed)
    }

    /// Enabled jobs whose `next_run` is on or before `now`. Sorted
    /// (next_run, id) for deterministic firing order.
    pub fn due_at(&self, now: DateTime<Utc>) -> Vec<Uuid> {
        let mut due: Vec<(DateTime<Utc>, Uuid)> = self
            .jobs
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.next_run.map(|n| (n, j.id)))
            .filter(|(n, _)| *n <= now)
            .collect();
        due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        due.into_iter().map(|(_, id)| id).collect()
    }

    /// Earliest `next_run` across all enabled jobs.
    pub fn next_due_at(&self) -> Option<DateTime<Utc>> {
        self.jobs
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.next_run)
            .min()
    }

    /// Record a successful firing. Recurring → recompute `next_run`
    /// (or disable if unreachable). One-off → remove. Returns the
    /// post-fire job for recurring; `None` when a one-off was deleted.
    pub fn mark_fired(
        &mut self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<CronJob>, CronError> {
        let pos = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(CronError::NotFound(id))?;
        let is_oneoff = matches!(self.jobs[pos].schedule, CronSchedule::OneOff { .. });
        if is_oneoff {
            self.jobs.remove(pos);
            self.persist()?;
            return Ok(None);
        }
        let next = compute_next_run(&self.jobs[pos].schedule, self.jobs[pos].timezone, now);
        let j = &mut self.jobs[pos];
        j.last_run = Some(now);
        match next {
            Ok(n) => j.next_run = Some(n),
            Err(_) => {
                j.enabled = false;
                j.next_run = None;
            }
        }
        let snapshot = j.clone();
        self.persist()?;
        Ok(Some(snapshot))
    }

    /// Disable a job (used when a recurring job's target vanishes).
    pub fn disable(&mut self, id: Uuid) -> Result<(), CronError> {
        let pos = self
            .jobs
            .iter()
            .position(|j| j.id == id)
            .ok_or(CronError::NotFound(id))?;
        self.jobs[pos].enabled = false;
        self.jobs[pos].next_run = None;
        self.persist()?;
        Ok(())
    }

    /// Run the startup reconciliation pass.
    ///
    /// - Past one-off + `catchup_on_startup=false` → remove.
    /// - Past one-off + `catchup_on_startup=true`  → set `next_run = now`.
    /// - All recurring → recompute `next_run` from `now` (no catchup).
    /// - Recurring with unreachable schedule → disabled.
    pub fn reconcile_on_startup(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<ReconcileReport, CronError> {
        let mut report = ReconcileReport::default();
        let mut to_remove: Vec<Uuid> = Vec::new();
        let mut to_queue: Vec<Uuid> = Vec::new();
        let mut to_recompute: Vec<Uuid> = Vec::new();

        for j in &self.jobs {
            match &j.schedule {
                CronSchedule::OneOff { .. } => {
                    let scheduled = j
                        .next_run
                        .or_else(|| compute_next_run(&j.schedule, j.timezone, now).ok());
                    let past = scheduled.map(|s| s < now).unwrap_or(true);
                    if past {
                        if j.enabled && j.catchup_on_startup {
                            to_queue.push(j.id);
                        } else {
                            to_remove.push(j.id);
                        }
                    }
                }
                CronSchedule::Recurring { .. } => {
                    if j.enabled {
                        to_recompute.push(j.id);
                    }
                }
            }
        }

        for id in &to_remove {
            if let Some(pos) = self.jobs.iter().position(|j| j.id == *id) {
                self.jobs.remove(pos);
                report.removed_one_offs.push(*id);
            }
        }
        for id in &to_queue {
            if let Some(pos) = self.jobs.iter().position(|j| j.id == *id) {
                self.jobs[pos].next_run = Some(now);
                report.queued_one_offs.push(*id);
            }
        }
        for id in &to_recompute {
            if let Some(pos) = self.jobs.iter().position(|j| j.id == *id) {
                let sched = self.jobs[pos].schedule.clone();
                let tz = self.jobs[pos].timezone;
                match compute_next_run(&sched, tz, now) {
                    Ok(n) => {
                        self.jobs[pos].next_run = Some(n);
                        report.recomputed_recurring.push(*id);
                    }
                    Err(_) => {
                        self.jobs[pos].enabled = false;
                        self.jobs[pos].next_run = None;
                        report.disabled_unreachable.push(*id);
                    }
                }
            }
        }
        self.persist()?;
        Ok(report)
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub fn all(&self) -> &[CronJob] {
        &self.jobs
    }

    /// Atomic write: serialize to `<path>.tmp` then rename. No-op for
    /// ephemeral stores (`path.is_none()`).
    fn persist(&self) -> Result<(), CronError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CronError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&self.jobs).map_err(|e| CronError::Parse {
            path: path.display().to_string(),
            source: e,
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| CronError::Io {
            path: tmp.display().to_string(),
            source: e,
        })?;
        std::fs::rename(&tmp, path).map_err(|e| CronError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }
}

pub fn validate_payload(p: &CronPayload) -> Result<(), CronError> {
    match p {
        CronPayload::Message { body } => {
            if body.trim().is_empty() {
                return Err(CronError::EmptyPayload);
            }
        }
        CronPayload::SlashCommand { command } => {
            let cmd = command.trim();
            if cmd.is_empty() {
                return Err(CronError::EmptyPayload);
            }
            if !cmd.starts_with('/') {
                return Err(CronError::InvalidSlashCommand);
            }
            let name = cmd
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if name == "exit" || name == "quit" || name == "shutdown" {
                return Err(CronError::BlockedSlashCommand);
            }
        }
    }
    Ok(())
}

pub fn validate_schedule(s: &CronSchedule) -> Result<(), CronError> {
    match s {
        CronSchedule::Recurring { expression } => {
            Schedule::from_str(expression)
                .map_err(|e| CronError::InvalidExpression(e.to_string()))?;
        }
        CronSchedule::OneOff { .. } => {}
    }
    Ok(())
}

/// Parse a recurring expression string from the wire (CLI or tag).
///
/// Accepts both 5-field POSIX cron (`min hour dom mon dow`) and the
/// `cron` crate's native 6/7-field form. 5-field input is normalized
/// by prepending `0` for the seconds field, so a user can write
/// `0 9 * * *` (every day at 9:00) and it round-trips through the
/// scheduler. The normalized form is what gets stored.
pub fn parse_recurring(expression: &str) -> Result<CronSchedule, CronError> {
    let normalized = normalize_cron_expression(expression);
    Schedule::from_str(&normalized).map_err(|e| CronError::InvalidExpression(e.to_string()))?;
    Ok(CronSchedule::Recurring {
        expression: normalized,
    })
}

fn normalize_cron_expression(input: &str) -> String {
    let trimmed = input.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Parse a one-off wall-clock string. Accepts `YYYY-MM-DDTHH:MM[:SS]`
/// (no offset). Tz is supplied separately via [`CronTz`].
pub fn parse_oneoff(wallclock: &str) -> Result<CronSchedule, CronError> {
    let s = wallclock.trim();
    let parsed = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .map_err(|e| CronError::InvalidDatetime(e.to_string()))?;
    Ok(CronSchedule::OneOff {
        run_at_local: parsed,
    })
}

pub fn compute_next_run(
    sched: &CronSchedule,
    tz: CronTz,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, CronError> {
    match (sched, tz) {
        (CronSchedule::Recurring { expression }, CronTz::Utc) => {
            let s = Schedule::from_str(expression)
                .map_err(|e| CronError::InvalidExpression(e.to_string()))?;
            s.after(&after)
                .next()
                .map(|dt| dt.with_timezone(&Utc))
                .ok_or(CronError::UnreachableSchedule)
        }
        (CronSchedule::Recurring { expression }, CronTz::Local) => {
            let s = Schedule::from_str(expression)
                .map_err(|e| CronError::InvalidExpression(e.to_string()))?;
            let after_local = after.with_timezone(&Local);
            s.after(&after_local)
                .next()
                .map(|dt| dt.with_timezone(&Utc))
                .ok_or(CronError::UnreachableSchedule)
        }
        (CronSchedule::OneOff { run_at_local }, CronTz::Utc) => {
            Ok(Utc.from_utc_datetime(run_at_local))
        }
        (CronSchedule::OneOff { run_at_local }, CronTz::Local) => Local
            .from_local_datetime(run_at_local)
            .single()
            .map(|dt| dt.with_timezone(&Utc))
            .ok_or(CronError::AmbiguousLocalDatetime),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    fn mk_store() -> CronStore {
        CronStore::new()
    }

    fn msg(body: &str) -> CronPayload {
        CronPayload::Message {
            body: body.to_string(),
        }
    }

    fn cmd(command: &str) -> CronPayload {
        CronPayload::SlashCommand {
            command: command.to_string(),
        }
    }

    fn recur(expr: &str) -> CronSchedule {
        CronSchedule::Recurring {
            expression: expr.to_string(),
        }
    }

    fn oneoff_in(secs: i64) -> CronSchedule {
        // Build a one-off as `now + secs` in UTC wall-clock terms.
        let when = (Utc::now() + Duration::seconds(secs)).naive_utc();
        CronSchedule::OneOff { run_at_local: when }
    }

    // ─── CronTz ──────────────────────────────────────────────────────

    #[test]
    fn cron_tz_parse_accepts_utc_local_case_insensitive() {
        assert_eq!(CronTz::parse("utc").unwrap(), CronTz::Utc);
        assert_eq!(CronTz::parse("UTC").unwrap(), CronTz::Utc);
        assert_eq!(CronTz::parse("local").unwrap(), CronTz::Local);
        assert_eq!(CronTz::parse("LOCAL").unwrap(), CronTz::Local);
        assert!(matches!(
            CronTz::parse("Pacific/Auckland"),
            Err(CronError::InvalidTz(_))
        ));
    }

    // ─── validate_payload ────────────────────────────────────────────

    #[test]
    fn validate_payload_rejects_empty_message() {
        assert!(matches!(
            validate_payload(&msg("   ")),
            Err(CronError::EmptyPayload)
        ));
    }

    #[test]
    fn validate_payload_rejects_slash_without_leading_slash() {
        assert!(matches!(
            validate_payload(&cmd("version")),
            Err(CronError::InvalidSlashCommand)
        ));
    }

    #[test]
    fn validate_payload_blocks_exit_and_quit() {
        for s in [
            "/exit",
            "/EXIT",
            "/quit",
            "/Quit",
            "/exit now",
            "/shutdown",
            "/SHUTDOWN",
            "/shutdown now",
        ] {
            assert!(
                matches!(
                    validate_payload(&cmd(s)),
                    Err(CronError::BlockedSlashCommand)
                ),
                "expected `{s}` to be blocked"
            );
        }
    }

    #[test]
    fn validate_payload_allows_other_slash_commands() {
        for s in ["/version", "/foo bar", "/anything-goes"] {
            validate_payload(&cmd(s)).expect("allowed");
        }
    }

    // ─── compute_next_run ────────────────────────────────────────────

    #[test]
    fn compute_next_run_recurring_utc_advances() {
        // every minute at second 0
        let s = recur("0 * * * * *");
        let now = Utc.with_ymd_and_hms(2026, 4, 22, 12, 0, 30).unwrap();
        let next = compute_next_run(&s, CronTz::Utc, now).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 4, 22, 12, 1, 0).unwrap());
    }

    #[test]
    fn compute_next_run_recurring_local_returns_utc() {
        // We can't assert the exact value without knowing the system tz,
        // but it must be strictly after `now` and within 24h.
        let s = recur("0 0 9 * * *"); // 9am every day
        let now = Utc::now();
        let next = compute_next_run(&s, CronTz::Local, now).unwrap();
        assert!(next > now);
        assert!(next - now < Duration::hours(25));
    }

    #[test]
    fn compute_next_run_oneoff_utc_yields_exact_instant() {
        let when =
            NaiveDateTime::parse_from_str("2026-04-22T14:35:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let s = CronSchedule::OneOff { run_at_local: when };
        let got = compute_next_run(&s, CronTz::Utc, Utc::now()).unwrap();
        assert_eq!(got, Utc.from_utc_datetime(&when));
    }

    #[test]
    fn compute_next_run_rejects_invalid_recurring_expression() {
        let s = recur("not a cron expression");
        assert!(matches!(
            compute_next_run(&s, CronTz::Utc, Utc::now()),
            Err(CronError::InvalidExpression(_))
        ));
    }

    // ─── CronStore::add ──────────────────────────────────────────────

    #[test]
    fn add_assigns_uuid_and_computes_next_run() {
        let mut s = mk_store();
        let job = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .clone();
        assert_eq!(job.target_agent, "reviewer");
        assert!(job.next_run.is_some());
        assert_eq!(job.created_by, "alice");
        assert!(job.enabled);
    }

    #[test]
    fn add_rejects_empty_target() {
        let mut s = mk_store();
        let err = s
            .add(
                "  ".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap_err();
        assert!(matches!(err, CronError::EmptyTarget));
    }

    #[test]
    fn add_rejects_blocked_slash_payload() {
        let mut s = mk_store();
        let err = s
            .add(
                "self".into(),
                cmd("/exit"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap_err();
        assert!(matches!(err, CronError::BlockedSlashCommand));
    }

    #[test]
    fn add_rejects_invalid_recurring_expression() {
        let mut s = mk_store();
        let err = s
            .add(
                "self".into(),
                msg("ping"),
                recur("not a cron"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap_err();
        assert!(matches!(err, CronError::InvalidExpression(_)));
    }

    #[test]
    fn add_disabled_job_has_no_next_run() {
        let mut s = mk_store();
        let job = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                false,
                "alice".into(),
            )
            .unwrap()
            .clone();
        assert!(!job.enabled);
        assert!(job.next_run.is_none());
    }

    // ─── update / remove ─────────────────────────────────────────────

    #[test]
    fn update_changes_fields_and_recomputes_next_run() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        s.update(
            id,
            CronPatch {
                payload: Some(msg("revised")),
                schedule: Some(recur("0 0 * * * *")),
                ..Default::default()
            },
        )
        .unwrap();
        let j = s.get(id).unwrap();
        match &j.payload {
            CronPayload::Message { body } => assert_eq!(body, "revised"),
            other => panic!("expected Message, got {other:?}"),
        }
        assert!(j.next_run.is_some());
    }

    #[test]
    fn update_disabling_clears_next_run() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        s.update(
            id,
            CronPatch {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let j = s.get(id).unwrap();
        assert!(!j.enabled);
        assert!(j.next_run.is_none());
    }

    #[test]
    fn remove_returns_removed_job() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let r = s.remove(id).unwrap();
        assert_eq!(r.id, id);
        assert!(s.get(id).is_none());
        assert!(matches!(s.remove(id), Err(CronError::NotFound(_))));
    }

    // ─── due_at / next_due_at ────────────────────────────────────────

    #[test]
    fn due_at_returns_only_enabled_jobs_due_now() {
        let mut s = mk_store();
        let _past_disabled = s
            .add(
                "a".into(),
                msg("p"),
                oneoff_in(-60),
                CronTz::Utc,
                false,
                false, // disabled
                "alice".into(),
            )
            .unwrap()
            .id;
        let past_enabled = s
            .add(
                "b".into(),
                msg("p"),
                oneoff_in(-30),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let _future = s
            .add(
                "c".into(),
                msg("p"),
                oneoff_in(3600),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;

        let due = s.due_at(Utc::now());
        assert_eq!(due, vec![past_enabled]);
    }

    #[test]
    fn next_due_at_picks_earliest_enabled() {
        let mut s = mk_store();
        s.add(
            "a".into(),
            msg("p"),
            oneoff_in(3600),
            CronTz::Utc,
            false,
            true,
            "alice".into(),
        )
        .unwrap();
        let earlier = s
            .add(
                "b".into(),
                msg("p"),
                oneoff_in(60),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .next_run
            .unwrap();
        s.add(
            "c".into(),
            msg("p"),
            oneoff_in(7200),
            CronTz::Utc,
            false,
            false, // disabled — must be ignored
            "alice".into(),
        )
        .unwrap();
        assert_eq!(s.next_due_at(), Some(earlier));
    }

    // ─── mark_fired ──────────────────────────────────────────────────

    #[test]
    fn mark_fired_recurring_advances_next_run_and_sets_last_run() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let now = Utc::now();
        let updated = s.mark_fired(id, now).unwrap().unwrap();
        assert_eq!(updated.last_run, Some(now));
        assert!(updated.next_run.unwrap() > now);
    }

    #[test]
    fn mark_fired_oneoff_removes_job() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                oneoff_in(-30),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let res = s.mark_fired(id, Utc::now()).unwrap();
        assert!(res.is_none(), "one-off should be deleted on fire");
        assert!(s.get(id).is_none());
    }

    // ─── reconcile_on_startup ────────────────────────────────────────

    #[test]
    fn reconcile_removes_past_oneoff_without_catchup() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                oneoff_in(-60),
                CronTz::Utc,
                false, // catchup_on_startup = false
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let report = s.reconcile_on_startup(Utc::now()).unwrap();
        assert_eq!(report.removed_one_offs, vec![id]);
        assert!(s.get(id).is_none());
    }

    #[test]
    fn reconcile_queues_past_oneoff_with_catchup() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                oneoff_in(-60),
                CronTz::Utc,
                true, // catchup_on_startup = true
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let now = Utc::now();
        let report = s.reconcile_on_startup(now).unwrap();
        assert_eq!(report.queued_one_offs, vec![id]);
        // `next_run` should now equal `now` so the next tick picks it.
        let j = s.get(id).unwrap();
        assert_eq!(j.next_run, Some(now));
    }

    #[test]
    fn reconcile_recomputes_recurring_next_run_no_catchup() {
        let mut s = mk_store();
        let id = s
            .add(
                "reviewer".into(),
                msg("ping"),
                recur("0 * * * * *"),
                CronTz::Utc,
                false,
                true,
                "alice".into(),
            )
            .unwrap()
            .id;
        let now = Utc.with_ymd_and_hms(2026, 4, 22, 12, 0, 30).unwrap();
        let report = s.reconcile_on_startup(now).unwrap();
        assert_eq!(report.recomputed_recurring, vec![id]);
        let j = s.get(id).unwrap();
        // Next firing should be in the future, not a backlog of missed
        // minutes.
        assert!(j.next_run.unwrap() > now);
    }

    // ─── persistence ─────────────────────────────────────────────────

    #[test]
    fn persist_survives_reopen() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cron.json");
        let id;
        {
            let mut s = CronStore::load_or_init(path.clone()).unwrap();
            id = s
                .add(
                    "reviewer".into(),
                    msg("ping"),
                    recur("0 * * * * *"),
                    CronTz::Utc,
                    false,
                    true,
                    "alice".into(),
                )
                .unwrap()
                .id;
        }
        assert!(path.exists());
        let s2 = CronStore::load_or_init(path).unwrap();
        let j = s2.get(id).expect("job persisted");
        assert_eq!(j.target_agent, "reviewer");
        assert!(j.next_run.is_some());
    }

    #[test]
    fn load_or_init_handles_missing_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cron.json");
        let s = CronStore::load_or_init(path.clone()).unwrap();
        assert!(s.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn load_or_init_errors_on_malformed_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cron.json");
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(matches!(
            CronStore::load_or_init(path),
            Err(CronError::Parse { .. })
        ));
    }

    #[test]
    fn schema_strict_rejects_unknown_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cron.json");
        // Has an extra `mystery` field; deny_unknown_fields should reject.
        std::fs::write(
            &path,
            br#"[{
                "id":"00000000-0000-0000-0000-000000000001",
                "target_agent":"a",
                "payload":{"kind":"message","body":"hi"},
                "schedule":{"kind":"recurring","expression":"0 * * * * *"},
                "timezone":"utc",
                "catchup_on_startup":false,
                "enabled":true,
                "created_by":"alice",
                "created_at":"2026-04-22T12:00:00Z",
                "last_run":null,
                "next_run":null,
                "mystery":"value"
            }]"#,
        )
        .unwrap();
        assert!(matches!(
            CronStore::load_or_init(path),
            Err(CronError::Parse { .. })
        ));
    }

    // ─── parse_oneoff / parse_recurring ──────────────────────────────

    #[test]
    fn parse_oneoff_accepts_iso_with_and_without_seconds() {
        assert!(parse_oneoff("2026-04-22T14:35").is_ok());
        assert!(parse_oneoff("2026-04-22T14:35:07").is_ok());
        assert!(matches!(
            parse_oneoff("not a date"),
            Err(CronError::InvalidDatetime(_))
        ));
    }

    #[test]
    fn parse_recurring_round_trips_valid_expression() {
        let s = parse_recurring("0 0 9 * * *").unwrap();
        match s {
            CronSchedule::Recurring { expression } => assert_eq!(expression, "0 0 9 * * *"),
            other => panic!("expected Recurring, got {other:?}"),
        }
    }

    #[test]
    fn parse_recurring_normalizes_5_field_posix_cron() {
        // `0 9 * * *` (every day at 9am) → prepend `0 ` for seconds.
        let s = parse_recurring("0 9 * * *").unwrap();
        match s {
            CronSchedule::Recurring { expression } => assert_eq!(expression, "0 0 9 * * *"),
            other => panic!("expected Recurring, got {other:?}"),
        }
    }

    #[test]
    fn parse_recurring_passes_through_6_field_form() {
        let s = parse_recurring("30 0 9 * * *").unwrap();
        match s {
            CronSchedule::Recurring { expression } => assert_eq!(expression, "30 0 9 * * *"),
            other => panic!("expected Recurring, got {other:?}"),
        }
    }

    // ─── list / filter ───────────────────────────────────────────────

    #[test]
    fn list_filters_by_target_and_enabled() {
        let mut s = mk_store();
        s.add(
            "a".into(),
            msg("p"),
            recur("0 * * * * *"),
            CronTz::Utc,
            false,
            true,
            "alice".into(),
        )
        .unwrap();
        s.add(
            "b".into(),
            msg("p"),
            recur("0 * * * * *"),
            CronTz::Utc,
            false,
            false,
            "alice".into(),
        )
        .unwrap();

        let only_enabled = s.list(&CronFilter::default());
        assert_eq!(only_enabled.len(), 1);
        assert_eq!(only_enabled[0].target_agent, "a");

        let with_disabled = s.list(&CronFilter {
            include_disabled: true,
            ..Default::default()
        });
        assert_eq!(with_disabled.len(), 2);

        let just_b = s.list(&CronFilter {
            include_disabled: true,
            target: Some("b".into()),
            ..Default::default()
        });
        assert_eq!(just_b.len(), 1);
        assert_eq!(just_b[0].target_agent, "b");
    }
}
