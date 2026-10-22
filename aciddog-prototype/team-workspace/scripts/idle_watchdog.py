#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Idle watchdog automaton.

Triggered every 5 minutes by an aciddog cron job (`cron add --target
idle_watchdog --kind message --payload "tick" --schedule "*/5 * * * *"`).
The inbound message envelope on stdin is ignored — the tick itself is
the trigger; everything we need we read back via the `aciddog` CLI.

Behaviour, per the operator's spec:

  1. Idle agent + assigned tasks
     For each agent with status=ready:
       - collect non-terminal tasks where assignee == agent.id;
       - if the agent is a leader, also collect tasks where
         assignee == agent.group;
       - skip if any worker parented to the agent is currently
         status=working (the agent is indirectly busy);
       - if the surviving task list is non-empty, DM the agent with a
         compact summary so they pick the work back up.

  2. Idle agent + idle workers (no tasks)
     For each agent with status=ready that wasn't nudged in (1) and
     has at least one parented worker in status=hibernated, DM the
     agent to either drive the worker(s) forward or archive them.
     This covers the orphan-coordination case (workers left behind
     after a stalled hand-off).

  3. Failures
     Any agent OR worker in status=error gets reported to the
     `aciddog` config consultant in a single batched message so it
     can investigate.

Exclusions per spec:
  - Tasks assigned to the reserved `user` id are skipped — the
    operator gets to them in their own time, no follow-up needed.
  - The watchdog never nudges itself or `aciddog` (the latter would
    create a loop on top of the failure-report channel it already
    receives).

Authentication: aciddog injects ACIDDOG_AGENT_ID into the child
process when an automaton's script runs, so this script's CLI calls
authenticate as `idle_watchdog` and inherit the `sysadmin = true` /
`supervisor = true` rights declared on the automaton block.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Iterable

ACIDDOG = os.environ.get("ACIDDOG_BIN", "/home/wil/.cargo/bin/aciddog")

SELF_ID = "idle_watchdog"
CONSULTANT_ID = "aciddog"

# Statuses a watchdog should NEVER nudge into.
SKIP_ASSIGNEES: set[str] = {"user"}
SKIP_AGENT_NUDGES: set[str] = {SELF_ID, CONSULTANT_ID}

# Task statuses that count as "live work waiting on the assignee".
# `done` and `cancelled` are excluded; `planning` tasks are typically
# assigned to the user (and thus filtered by SKIP_ASSIGNEES) but a
# planner-owned planning task would be a legitimate nudge target too.
LIVE_TASK_STATUSES: tuple[str, ...] = (
    "open",
    "assigned",
    "approved",
    "in_progress",
    "review",
    "planning",
)


# ── data classes ────────────────────────────────────────────────────


@dataclass
class FleetMember:
    id: str
    kind: str  # "agent" | "worker" | "automaton"
    status: str
    parent: str | None
    ai_core: str
    leader: bool
    group: str | None


@dataclass
class Task:
    id: str
    status: str
    assignee: str | None
    deps: str
    title: str


@dataclass
class AgentSnapshot:
    member: FleetMember
    workers: list[FleetMember] = field(default_factory=list)
    tasks: list[Task] = field(default_factory=list)

    @property
    def has_busy_worker(self) -> bool:
        return any(w.status == "working" for w in self.workers)

    @property
    def hibernated_workers(self) -> list[FleetMember]:
        return [w for w in self.workers if w.status == "hibernated"]


# ── shell-out helpers ───────────────────────────────────────────────


def _run(args: list[str], *, check: bool = True) -> str:
    """Invoke aciddog with `args`. Return stdout (text). Raise on non-zero
    when `check`, else return whatever stdout we got."""
    result = subprocess.run(
        [ACIDDOG, *args],
        capture_output=True,
        text=True,
        check=False,
    )
    if check and result.returncode != 0:
        raise RuntimeError(
            f"aciddog {' '.join(args)} failed (rc={result.returncode}): "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout


def fetch_fleet() -> list[FleetMember]:
    """Parse `aciddog agent list` (TSV with header)."""
    out = _run(["agent", "list"])
    lines = [ln for ln in out.splitlines() if ln.strip()]
    if not lines:
        return []
    headers = lines[0].split("\t")
    rows: list[FleetMember] = []
    for line in lines[1:]:
        cols = line.split("\t")
        if len(cols) != len(headers):
            continue
        row = dict(zip(headers, cols))
        rows.append(
            FleetMember(
                id=row.get("id", "").strip(),
                kind=row.get("kind", "").strip(),
                status=row.get("status", "").strip(),
                parent=(row.get("parent") or "").strip() or None
                if (row.get("parent") or "").strip() != "-"
                else None,
                ai_core=row.get("ai_core", "").strip(),
                leader=row.get("leader", "false").strip().lower() == "true",
                group=(row.get("group") or "").strip() or None,
            )
        )
    return rows


def fetch_tasks_in_status(status: str) -> list[Task]:
    """Parse `aciddog task list --status <status>` (whitespace-aligned table)."""
    try:
        out = _run(["task", "list", "--status", status])
    except RuntimeError:
        return []
    rows: list[Task] = []
    for line in out.splitlines():
        line = line.rstrip()
        if not line or line.startswith("(no tasks") or line.startswith("id "):
            continue
        # uuid is 36 chars; columns after are whitespace-padded.
        # split with maxsplit=4 -> [id, status, assignee, deps, title]
        parts = line.split(None, 4)
        if len(parts) < 4:
            continue
        if len(parts) == 4:
            parts.append("")  # empty title
        task_id, st, assignee, deps, title = parts
        # uuid sanity check (cheap)
        if len(task_id) != 36 or task_id[8] != "-":
            continue
        rows.append(
            Task(
                id=task_id,
                status=st,
                assignee=None if assignee in {"-", ""} else assignee,
                deps=deps,
                title=title.strip(),
            )
        )
    return rows


def fetch_live_tasks() -> list[Task]:
    """Union of all non-terminal task statuses, deduplicated by id."""
    seen: dict[str, Task] = {}
    for status in LIVE_TASK_STATUSES:
        for task in fetch_tasks_in_status(status):
            seen.setdefault(task.id, task)
    return list(seen.values())


def send_dm(target: str, body: str) -> None:
    """Best-effort `aciddog send`. Errors are printed (logged to chat) but
    do not abort the rest of the sweep."""
    try:
        _run(["send", "--to", target, "--body", body])
    except RuntimeError as exc:
        print(f"[idle_watchdog] failed to DM {target}: {exc}", file=sys.stderr)


# ── correlation ─────────────────────────────────────────────────────


def build_snapshots(
    fleet: list[FleetMember], tasks: list[Task]
) -> tuple[list[AgentSnapshot], list[FleetMember]]:
    """Return (agent snapshots indexed by id, errored members)."""
    agents = [m for m in fleet if m.kind == "agent"]
    workers = [m for m in fleet if m.kind == "worker"]
    errored = [m for m in fleet if m.status == "error"]

    workers_by_parent: dict[str, list[FleetMember]] = defaultdict(list)
    for w in workers:
        if w.parent:
            workers_by_parent[w.parent].append(w)

    snapshots: list[AgentSnapshot] = []
    for agent in agents:
        snap = AgentSnapshot(member=agent)
        snap.workers = workers_by_parent.get(agent.id, [])
        # Tasks assigned directly to the agent.
        direct = [
            t
            for t in tasks
            if t.assignee == agent.id and t.assignee not in SKIP_ASSIGNEES
        ]
        # Tasks assigned to the agent's group iff the agent is a leader.
        group_tasks: list[Task] = []
        if agent.leader and agent.group:
            group_tasks = [
                t
                for t in tasks
                if t.assignee == agent.group and t.assignee not in SKIP_ASSIGNEES
            ]
        # Dedup by task id (a task could in theory match both lists if
        # an agent's id and group label were the same — they can't per
        # validation rules, but be defensive).
        snap.tasks = list({t.id: t for t in (direct + group_tasks)}.values())
        snapshots.append(snap)
    return snapshots, errored


# ── nudge composers ─────────────────────────────────────────────────


def _format_task_lines(tasks: Iterable[Task], limit: int = 8) -> str:
    items = list(tasks)
    head = items[:limit]
    lines = [
        f"  - [{t.status}] {t.id[:8]} — {t.title or '(no title)'}" for t in head
    ]
    if len(items) > limit:
        lines.append(f"  - … and {len(items) - limit} more")
    return "\n".join(lines)


def nudge_agent_with_tasks(snap: AgentSnapshot) -> str:
    return (
        "Idle-watchdog ping: you are idle but the following task(s) are "
        f"assigned to you (or to your group `{snap.member.group}`) and waiting "
        "for movement. Pick one up — `aciddog task show --id <uuid>` to read it,"
        " then `task update --status in_progress` (or whatever your workflow "
        "step is) and start work. If a task is blocked, leave a `task comment` "
        "explaining why and reassign it.\n\n"
        f"Open work ({len(snap.tasks)}):\n"
        + _format_task_lines(snap.tasks)
        + "\n\nIf you genuinely have nothing actionable here, reply to me "
        "explaining what's blocking and I'll route it. (Auto-message; ignored "
        "if you are about to act.)"
    )


def nudge_agent_with_idle_workers(snap: AgentSnapshot) -> str:
    worker_ids = ", ".join(w.id for w in snap.hibernated_workers)
    return (
        "Idle-watchdog ping: you have no assigned tasks but you own "
        f"{len(snap.hibernated_workers)} hibernated worker(s) — "
        f"{worker_ids} — that aren't being driven. Either send them their "
        "next instruction (`aciddog send --to <worker-id> --body \"…\"`) or "
        "archive them (`aciddog worker archive --id <worker-id>`). Leaving "
        "hibernated workers around clutters supervision and chews session "
        "state. (Auto-message; ignore if you're about to act.)"
    )


def report_failures_to_consultant(errored: list[FleetMember]) -> str:
    lines = []
    for m in errored:
        parent = f", parent={m.parent}" if m.parent else ""
        lines.append(f"  - {m.id} ({m.kind}, ai_core={m.ai_core}{parent})")
    return (
        "Idle-watchdog report: the following fleet member(s) are in "
        "`error` state and need investigation. Use `aciddog agent history "
        "--id <id> --kind system_error` to see why, then either "
        "`worker force-archive` (workers) or `agent stop`/`agent wake` "
        "(agents) as appropriate.\n\n"
        f"Errored ({len(errored)}):\n" + "\n".join(lines)
    )


# ── main sweep ──────────────────────────────────────────────────────


def main() -> int:
    # Drain stdin so the parent doesn't see a SIGPIPE; payload is unused.
    try:
        sys.stdin.read()
    except Exception:
        pass

    try:
        fleet = fetch_fleet()
        tasks = fetch_live_tasks()
    except RuntimeError as exc:
        print(f"[idle_watchdog] fleet/task fetch failed: {exc}", file=sys.stderr)
        return 1

    snapshots, errored = build_snapshots(fleet, tasks)

    nudged_for_tasks: list[str] = []
    nudged_for_workers: list[str] = []

    for snap in snapshots:
        agent = snap.member
        if agent.id in SKIP_AGENT_NUDGES:
            continue
        if agent.status != "ready":
            # Busy / asleep / hibernated agents do not need nudging.
            continue
        if snap.has_busy_worker:
            # A child worker is currently running on the agent's behalf
            # — the agent is effectively engaged.
            continue

        if snap.tasks:
            send_dm(agent.id, nudge_agent_with_tasks(snap))
            nudged_for_tasks.append(agent.id)
        elif snap.hibernated_workers:
            send_dm(agent.id, nudge_agent_with_idle_workers(snap))
            nudged_for_workers.append(agent.id)

    # Failure report (single batched message to the config consultant).
    if errored:
        send_dm(CONSULTANT_ID, report_failures_to_consultant(errored))

    # Sweep summary on stdout — surfaces in the watchdog's own chat pane.
    summary = {
        "fleet_total": len(fleet),
        "tasks_live": len(tasks),
        "nudged_for_tasks": nudged_for_tasks,
        "nudged_for_idle_workers": nudged_for_workers,
        "errored_reported": [m.id for m in errored],
    }
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
