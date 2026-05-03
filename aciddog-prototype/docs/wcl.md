# WCL Reference

## Cron Blocks

Declare scheduled jobs with `cron <id> { ... }` blocks. Field semantics match
`aciddog cron add`: `target` names a configured agent or automaton, `kind` is
`:message` or `:command`, `payload` is the delivered message body or slash
command, `schedule` is either a 5-field POSIX cron expression or a one-off
`YYYY-MM-DDTHH:MM[:SS]` timestamp when `oneoff = true`, and `tz` is `:local`
or `:utc` (`:local` by default).

```wcl
cron idle_watchdog {
  target = "idle_watchdog"
  kind = :message
  payload = "tick"
  schedule = "*/5 * * * *"
}
```

Optional booleans default to `false`:

- `oneoff`
- `catchup_on_startup` (imperative cron jobs only)
- `disabled`

WCL cron blocks are reapplied on startup and `/reload`; you can still add/remove
jobs imperatively via `aciddog cron add` and they live alongside WCL jobs
uninterrupted.

A WCL cron block with both `oneoff = true` and `catchup_on_startup = true` is
rejected at config load. WCL is reapplied on every `/reload`, which is
incompatible with the prune-after-fire one-off lifecycle that
`catchup_on_startup` expects. For exactly-once-ever catchup semantics, use the
imperative path: `aciddog cron add --oneoff --catchup-on-startup ...`.

Live SSE subscribers receive `CronEvent::{Created, Updated, Removed}` for
WCL-driven cron mutations on boot and `/reload`, so open Cron tabs in the
manager portal stay in sync without manual refresh.
