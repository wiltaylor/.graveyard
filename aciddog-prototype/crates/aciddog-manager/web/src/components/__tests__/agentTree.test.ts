import { describe, expect, it } from "vitest";

import { groupAgents } from "../agents/agentTree";
import type { AgentRow } from "../../types";

// Compact factory so each test reads as a near-table; defaults match
// the most common case (top-level agent, ready, no group, no leader).
function row(overrides: Partial<AgentRow> & { id: string }): AgentRow {
  return {
    id: overrides.id,
    kind: overrides.kind ?? "agent",
    status: overrides.status ?? "ready",
    parent: overrides.parent ?? null,
    ai_core: overrides.ai_core ?? null,
    leader: overrides.leader ?? false,
    group: overrides.group ?? null,
    automaton_backend: overrides.automaton_backend ?? "none",
  };
}

describe("groupAgents — nesting (case a)", () => {
  it("nests workers under their parent agent in a single group", () => {
    const rows: AgentRow[] = [
      row({ id: "lead", leader: true, group: "alpha" }),
      row({ id: "follower", group: "alpha" }),
      row({ id: "lead-w1", kind: "worker", parent: "lead", group: "alpha" }),
      row({ id: "lead-w2", kind: "worker", parent: "lead", group: "alpha" }),
      row({
        id: "follower-w1",
        kind: "worker",
        parent: "follower",
        group: "alpha",
      }),
      row({
        id: "follower-w2",
        kind: "worker",
        parent: "follower",
        group: "alpha",
      }),
    ];

    const groups = groupAgents(rows);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.name).toBe("alpha");
    expect(groups[0]!.agents.map((n) => n.row.id)).toEqual([
      "lead",
      "follower",
    ]);
    expect(groups[0]!.agents[0]!.children.map((c) => c.row.id)).toEqual([
      "lead-w1",
      "lead-w2",
    ]);
    expect(groups[0]!.agents[1]!.children.map((c) => c.row.id)).toEqual([
      "follower-w1",
      "follower-w2",
    ]);
    expect(groups[0]!.orphans).toEqual([]);
  });
});

describe("groupAgents — hibernated workers (cases b + c)", () => {
  const rows: AgentRow[] = [
    row({ id: "lead", leader: true }),
    row({ id: "snoozed", kind: "worker", parent: "lead", status: "hibernated" }),
    row({ id: "awake", kind: "worker", parent: "lead", status: "ready" }),
  ];

  it("filters hibernated workers by default", () => {
    const groups = groupAgents(rows);
    const kids = groups[0]!.agents[0]!.children.map((c) => c.row.id);
    expect(kids).toEqual(["awake"]);
  });

  it("keeps hibernated workers when showHibernated:true", () => {
    const groups = groupAgents(rows, { showHibernated: true });
    const kids = groups[0]!.agents[0]!.children.map((c) => c.row.id);
    expect(kids).toEqual(["snoozed", "awake"]);
  });

  it("never filters hibernated TOP-LEVEL agents", () => {
    // Top-level (parent === null) survives even with the hibernated
    // status — the renderer should still surface a hibernated agent.
    const tl: AgentRow[] = [
      row({ id: "tl-agent", status: "hibernated" }),
    ];
    const groups = groupAgents(tl);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.agents.map((n) => n.row.id)).toEqual(["tl-agent"]);
  });
});

describe("groupAgents — archived rows always dropped (case d)", () => {
  it("drops archived top-level agents AND archived workers", () => {
    const rows: AgentRow[] = [
      row({ id: "live-lead", leader: true }),
      // Cast through unknown so we can stash the not-yet-typed
      // `archived` field while keeping the helper's defensive read
      // honest.
      { ...row({ id: "ghost-agent" }), archived: true } as AgentRow,
      row({ id: "live-w", kind: "worker", parent: "live-lead" }),
      {
        ...row({ id: "ghost-w", kind: "worker", parent: "live-lead" }),
        archived: true,
      } as AgentRow,
    ];

    const groups = groupAgents(rows);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.agents.map((n) => n.row.id)).toEqual(["live-lead"]);
    expect(groups[0]!.agents[0]!.children.map((c) => c.row.id)).toEqual([
      "live-w",
    ]);
  });
});

describe("groupAgents — orphan workers (case e)", () => {
  it("returns a worker with an unresolved parent as a top-level orphan", () => {
    const rows: AgentRow[] = [
      row({ id: "real-lead", leader: true, group: "alpha" }),
      row({
        id: "ghosted-w",
        kind: "worker",
        parent: "missing-parent",
        group: "alpha",
      }),
    ];

    const groups = groupAgents(rows);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.name).toBe("alpha");
    expect(groups[0]!.orphans).toHaveLength(1);
    expect(groups[0]!.orphans[0]!.row.id).toBe("ghosted-w");
    expect(groups[0]!.orphans[0]!.orphaned).toBe(true);
    // The orphan must NOT have leaked into any agent's children.
    expect(groups[0]!.agents[0]!.children).toEqual([]);
  });

  it("mints a brand-new bucket from an orphan's own group when no agent created it", () => {
    const rows: AgentRow[] = [
      row({ id: "lead-a", group: "alpha" }),
      // The orphan's parent doesn't resolve AND no agent has group
      // "beta" — the helper should mint a "beta" bucket.
      row({
        id: "lonely-w",
        kind: "worker",
        parent: "missing",
        group: "beta",
      }),
    ];
    const groups = groupAgents(rows);
    expect(groups.map((g) => g.name)).toEqual(["alpha", "beta"]);
    const beta = groups.find((g) => g.name === "beta")!;
    expect(beta.agents).toEqual([]);
    expect(beta.orphans.map((o) => o.row.id)).toEqual(["lonely-w"]);
    expect(beta.orphans[0]!.orphaned).toBe(true);
  });
});

describe("groupAgents — bucket order (case f)", () => {
  it("first-appearance for named groups, null bucket pinned LAST", () => {
    const rows: AgentRow[] = [
      // First null-grouped row appears second — it must still come
      // last in the output.
      row({ id: "a-1", group: "alpha" }),
      row({ id: "u-1" }), // null bucket
      row({ id: "b-1", group: "beta" }),
      row({ id: "a-2", group: "alpha" }),
      row({ id: "u-2" }),
    ];
    const groups = groupAgents(rows);
    expect(groups.map((g) => g.name)).toEqual(["alpha", "beta", null]);
    // The null bucket has its data preserved with name=null (renderer
    // is the one that prints "(ungrouped)").
    expect(groups[2]!.name).toBeNull();
    expect(groups[2]!.agents.map((n) => n.row.id)).toEqual(["u-1", "u-2"]);
  });

  it("does not invent a null bucket when no row has null group", () => {
    const rows: AgentRow[] = [
      row({ id: "a-1", group: "alpha" }),
      row({ id: "b-1", group: "beta" }),
    ];
    const groups = groupAgents(rows);
    expect(groups.map((g) => g.name)).toEqual(["alpha", "beta"]);
  });
});

describe("groupAgents — leader pin (case g)", () => {
  it("pins leader === true && parent === null first; preserves first-appearance otherwise", () => {
    const rows: AgentRow[] = [
      // First-appearance order: x, y, lead — but lead must sort first.
      row({ id: "x" }),
      row({ id: "y" }),
      row({ id: "lead", leader: true }),
    ];
    const groups = groupAgents(rows);
    expect(groups[0]!.agents.map((n) => n.row.id)).toEqual(["lead", "x", "y"]);
  });

  it("does NOT pin a 'leader' worker (leader=true but parent !== null)", () => {
    const rows: AgentRow[] = [
      row({ id: "tl-a" }),
      row({ id: "tl-b", leader: true }),
      // A worker that happens to carry `leader: true` should still
      // nest under its parent — never get pinned at the top.
      row({
        id: "leaderly-w",
        kind: "worker",
        parent: "tl-a",
        leader: true,
      }),
    ];
    const groups = groupAgents(rows);
    expect(groups[0]!.agents.map((n) => n.row.id)).toEqual(["tl-b", "tl-a"]);
    // The "leader" worker is in tl-a's children, not the agents list.
    const tlA = groups[0]!.agents.find((n) => n.row.id === "tl-a")!;
    expect(tlA.children.map((c) => c.row.id)).toEqual(["leaderly-w"]);
  });

  it("multiple leaders preserve first-appearance amongst themselves", () => {
    const rows: AgentRow[] = [
      row({ id: "non-lead" }),
      row({ id: "lead-1", leader: true }),
      row({ id: "lead-2", leader: true }),
    ];
    const groups = groupAgents(rows);
    expect(groups[0]!.agents.map((n) => n.row.id)).toEqual([
      "lead-1",
      "lead-2",
      "non-lead",
    ]);
  });
});
