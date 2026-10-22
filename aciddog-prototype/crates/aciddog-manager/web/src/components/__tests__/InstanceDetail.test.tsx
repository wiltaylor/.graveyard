import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import InstanceDetail from "../InstanceDetail";
import type {
  AgentListResponse,
  AgentRow,
  HealthResponse,
  InstanceSummary,
} from "../../types";

const apiMock = vi.hoisted(() => ({
  health: vi.fn(),
  agents: vi.fn(),
  deleteInstance: vi.fn(),
}));

vi.mock("../../api", () => ({
  api: {
    health: apiMock.health,
    agents: apiMock.agents,
    deleteInstance: apiMock.deleteInstance,
  },
}));

const instance: InstanceSummary = {
  id: "inst-1",
  name: "fixture",
  transport: "https",
  url: "https://example.test",
  socket_path: null,
  tls_ca_path: null,
  has_api_key: false,
  created_at: "2026-05-03T01:00:00Z",
};

const health: HealthResponse = { ok: true, status: 200, body: "ok" };

// Six-row fixture mirroring the plan: ready leader agent, working
// agent, error agent, hibernated worker (parented to the leader),
// asleep automaton, ready automaton.
const agents: AgentRow[] = [
  {
    id: "lead-agent",
    kind: "agent",
    status: "ready",
    parent: null,
    ai_core: "core-a",
    leader: true,
    group: null,
    automaton_backend: "none",
  },
  {
    id: "busy-agent",
    kind: "agent",
    status: "working",
    parent: null,
    ai_core: "core-b",
    leader: false,
    group: null,
    automaton_backend: "none",
  },
  {
    id: "broken-agent",
    kind: "agent",
    status: "error",
    parent: null,
    ai_core: "core-c",
    leader: false,
    group: null,
    automaton_backend: "none",
  },
  {
    id: "snoozed-worker",
    kind: "worker",
    status: "hibernated",
    parent: "lead-agent",
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "none",
  },
  {
    id: "sleepy-bot",
    kind: "automaton",
    status: "asleep",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "script",
  },
  {
    id: "ready-bot",
    kind: "automaton",
    status: "ready",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "script",
  },
];

const agentResp: AgentListResponse = {
  agents,
  ok: true,
  raw: null,
  error: null,
};

// In the new tree layout each top-level agent lives in a container
// tagged with `data-ag-agent-id`; its children (workers) live under
// `data-ag-children`. Workers carry `data-ag-worker-id`. Helpers
// below resolve a row by id regardless of which kind it is so the
// per-row assertions stay close to the prior table-style intent.
function agentContainer(
  container: HTMLElement,
  agentId: string,
): HTMLElement {
  const el = container.querySelector<HTMLElement>(
    `[data-ag-agent-id="${agentId}"]`,
  );
  if (!el) throw new Error(`No agent container for id ${agentId}`);
  return el;
}

function workerRow(
  container: HTMLElement,
  workerId: string,
): HTMLElement {
  const el = container.querySelector<HTMLElement>(
    `[data-ag-worker-id="${workerId}"]`,
  );
  if (!el) throw new Error(`No worker row for id ${workerId}`);
  return el;
}

function rowFor(container: HTMLElement, agentId: string): HTMLElement {
  // Tries agent-container first; falls back to worker row. Used for
  // the original tone/glyph/leader assertions which don't care which
  // kind of row carries the badge.
  const ag = container.querySelector<HTMLElement>(
    `[data-ag-agent-id="${agentId}"]`,
  );
  if (ag) return ag;
  const w = container.querySelector<HTMLElement>(
    `[data-ag-worker-id="${agentId}"]`,
  );
  if (w) return w;
  throw new Error(`No row found for id ${agentId}`);
}

describe("InstanceDetail agent rows mirror TUI palette", () => {
  beforeEach(() => {
    apiMock.health.mockResolvedValue(health);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("renders the matching badge tone + glyph for every visible agent kind/status", async () => {
    const { container } = render(() => (
      <InstanceDetail
        instance={instance}
        agents={agentResp}
        onRefreshAgents={() => {}}
        onDeleted={() => {}}
        onSelectAgent={() => {}}
      />
    ));

    await waitFor(() => {
      // Five top-level rows survive the default hide-hibernated
      // filter (the lead, busy, broken, sleepy-bot, ready-bot
      // entries). The hibernated worker is filtered out.
      expect(
        container.querySelectorAll("[data-ag-agent-id]"),
      ).toHaveLength(5);
    });

    // Status column: each visible row gets the matching tone class +
    // glyph. The hibernated worker is excluded from the new layout
    // (filtered by `groupAgents` — covered separately).
    const expectations: Array<{
      id: string;
      toneClass: string;
      glyph: string;
      label: string;
    }> = [
      {
        id: "lead-agent",
        toneClass: "fbadge-success",
        glyph: "🟢",
        label: "ready",
      },
      {
        id: "busy-agent",
        toneClass: "fbadge-warning",
        glyph: "🟡",
        label: "working",
      },
      {
        id: "broken-agent",
        toneClass: "fbadge-danger",
        glyph: "🔴",
        label: "error",
      },
      {
        id: "sleepy-bot",
        toneClass: "fbadge-info",
        glyph: "🟦",
        label: "asleep",
      },
      {
        id: "ready-bot",
        toneClass: "fbadge-success",
        glyph: "🟩",
        label: "ready",
      },
    ];

    for (const { id, toneClass, glyph, label } of expectations) {
      const row = rowFor(container, id);
      const dotBadge = row.querySelector<HTMLElement>(".fbadge-dot")
        ?.parentElement;
      expect(dotBadge, `dotted badge for ${id}`).toBeTruthy();
      expect(dotBadge?.className).toContain(toneClass);
      expect(dotBadge?.textContent).toContain(glyph);
      expect(dotBadge?.textContent).toContain(label);
    }
  });

  it("indents the worker row with ↳ and a [worker] suffix when shown", async () => {
    // The default hide-hibernated filter would drop our only worker
    // — re-render with a non-hibernated worker so the [worker] +
    // ↳ decoration assertion still applies on the new layout.
    const liveAgents: AgentRow[] = [
      ...agents.slice(0, 3),
      {
        id: "live-worker",
        kind: "worker",
        status: "ready",
        parent: "lead-agent",
        ai_core: null,
        leader: false,
        group: null,
        automaton_backend: "none",
      },
      ...agents.slice(4),
    ];
    const liveResp: AgentListResponse = {
      agents: liveAgents,
      ok: true,
      raw: null,
      error: null,
    };

    const { container } = render(() => (
      <InstanceDetail
        instance={instance}
        agents={liveResp}
        onRefreshAgents={() => {}}
        onDeleted={() => {}}
        onSelectAgent={() => {}}
      />
    ));

    await waitFor(() => {
      expect(workerRow(container, "live-worker")).toBeTruthy();
    });

    const row = workerRow(container, "live-worker");
    expect(row.textContent).toContain("↳");
    expect(row.textContent).toContain("[worker]");
  });

  it("keeps the 👑 leader decoration only on leader && parent === null", async () => {
    const { container } = render(() => (
      <InstanceDetail
        instance={instance}
        agents={agentResp}
        onRefreshAgents={() => {}}
        onDeleted={() => {}}
        onSelectAgent={() => {}}
      />
    ));

    await waitFor(() => {
      expect(
        container.querySelectorAll("[data-ag-agent-id]"),
      ).toHaveLength(5);
    });

    const leaderRow = rowFor(container, "lead-agent");
    expect(leaderRow.querySelector('[data-ag-leader="true"]')).toBeTruthy();
    expect(leaderRow.textContent).toContain("👑");

    // None of the other visible rows (followers + automatons) carry
    // the crown. The hibernated worker is filtered out of the
    // default render, so the assertion only covers what the user
    // would actually see.
    const otherIds = ["busy-agent", "broken-agent", "sleepy-bot", "ready-bot"];
    for (const id of otherIds) {
      const row = rowFor(container, id);
      expect(row.querySelector('[data-ag-leader="true"]')).toBeNull();
    }
  });
});

// ---------------------------------------------------------------------------
// Structural nesting fixture: 2 groups, 3 agents, 4 workers (one
// hibernated, one orphan whose parent doesn't resolve).
// ---------------------------------------------------------------------------

const nestedAgents: AgentRow[] = [
  // alpha bucket — appears first.
  {
    id: "alpha-lead",
    kind: "agent",
    status: "ready",
    parent: null,
    ai_core: "core-1",
    leader: true,
    group: "alpha",
    automaton_backend: "none",
  },
  {
    id: "alpha-follower",
    kind: "agent",
    status: "working",
    parent: null,
    ai_core: "core-2",
    leader: false,
    group: "alpha",
    automaton_backend: "none",
  },
  // alpha workers — one ready (visible), one hibernated (filtered).
  {
    id: "alpha-w-live",
    kind: "worker",
    status: "ready",
    parent: "alpha-lead",
    ai_core: null,
    leader: false,
    group: "alpha",
    automaton_backend: "none",
  },
  {
    id: "alpha-w-snoozed",
    kind: "worker",
    status: "hibernated",
    parent: "alpha-lead",
    ai_core: null,
    leader: false,
    group: "alpha",
    automaton_backend: "none",
  },
  // beta bucket — appears second.
  {
    id: "beta-agent",
    kind: "agent",
    status: "ready",
    parent: null,
    ai_core: "core-3",
    leader: false,
    group: "beta",
    automaton_backend: "none",
  },
  {
    id: "beta-w-live",
    kind: "worker",
    status: "ready",
    parent: "beta-agent",
    ai_core: null,
    leader: false,
    group: "beta",
    automaton_backend: "none",
  },
  // Orphan worker — parent does not resolve. Carries group "beta"
  // so it lands in the beta bucket as a top-level orphan.
  {
    id: "ghost-w",
    kind: "worker",
    status: "ready",
    parent: "missing-parent",
    ai_core: null,
    leader: false,
    group: "beta",
    automaton_backend: "none",
  },
];

const nestedResp: AgentListResponse = {
  agents: nestedAgents,
  ok: true,
  raw: null,
  error: null,
};

describe("Agents card structural nesting", () => {
  beforeEach(() => {
    apiMock.health.mockResolvedValue(health);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  function renderNested() {
    return render(() => (
      <InstanceDetail
        instance={instance}
        agents={nestedResp}
        onRefreshAgents={() => {}}
        onDeleted={() => {}}
        onSelectAgent={() => {}}
      />
    ));
  }

  it("(a) lists group headers in first-appearance order", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(
        container.querySelectorAll("[data-ag-group-header]").length,
      ).toBeGreaterThan(0);
    });
    const headers = Array.from(
      container.querySelectorAll<HTMLElement>("[data-ag-group-header]"),
    ).map((el) => el.getAttribute("data-ag-group-header"));
    // No null bucket in this fixture — only alpha, beta in
    // first-appearance order.
    expect(headers).toEqual(["alpha", "beta"]);
  });

  it("(b) DOM-nests worker rows inside their parent agent's container", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(workerRow(container, "alpha-w-live")).toBeTruthy();
    });

    // .closest() walks up to the nearest `[data-ag-agent-id]`. For a
    // properly nested worker that should resolve to the parent
    // agent's container.
    const worker = workerRow(container, "alpha-w-live");
    const parent = worker.closest<HTMLElement>("[data-ag-agent-id]");
    expect(parent).toBeTruthy();
    expect(parent?.getAttribute("data-ag-agent-id")).toBe("alpha-lead");

    const betaWorker = workerRow(container, "beta-w-live");
    const betaParent = betaWorker.closest<HTMLElement>("[data-ag-agent-id]");
    expect(betaParent?.getAttribute("data-ag-agent-id")).toBe("beta-agent");
  });

  it("(c) hides hibernated workers from the rendered DOM by default", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(workerRow(container, "alpha-w-live")).toBeTruthy();
    });
    expect(
      container.querySelector('[data-ag-worker-id="alpha-w-snoozed"]'),
    ).toBeNull();
  });

  it("(d) renders the orphan worker top-level inside its own group bucket with an (orphaned) tag", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(workerRow(container, "ghost-w")).toBeTruthy();
    });
    const ghost = workerRow(container, "ghost-w");
    // Orphan must NOT live inside any agent container.
    expect(ghost.closest("[data-ag-agent-id]")).toBeNull();
    // Orphan carries the data flag and the visible (orphaned) tag.
    expect(ghost.getAttribute("data-ag-orphan")).toBe("true");
    expect(ghost.textContent).toContain("(orphaned)");
    // Lives inside the beta group bucket (not alpha).
    const groupSection = ghost.closest<HTMLElement>("[data-ag-group]");
    expect(groupSection?.getAttribute("data-ag-group")).toBe("beta");
  });

  it("(e) clicking the chevron removes the agent's children block from the DOM (re-clicking restores it)", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(workerRow(container, "alpha-w-live")).toBeTruthy();
    });

    const chevron = container.querySelector<HTMLElement>(
      '[data-ag-chevron="alpha-lead"]',
    );
    expect(chevron, "chevron present").toBeTruthy();

    // Initially expanded — children block visible.
    expect(
      container.querySelector('[data-ag-children="alpha-lead"]'),
    ).toBeTruthy();
    expect(workerRow(container, "alpha-w-live")).toBeTruthy();

    // Collapse.
    fireEvent.click(chevron!);
    expect(
      container.querySelector('[data-ag-children="alpha-lead"]'),
    ).toBeNull();
    expect(
      container.querySelector('[data-ag-worker-id="alpha-w-live"]'),
    ).toBeNull();

    // Re-expand.
    fireEvent.click(chevron!);
    expect(
      container.querySelector('[data-ag-children="alpha-lead"]'),
    ).toBeTruthy();
    expect(workerRow(container, "alpha-w-live")).toBeTruthy();
  });

  it("(f) places 👑 only on the leader row — never on followers, never on workers", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(workerRow(container, "alpha-w-live")).toBeTruthy();
    });

    const leader = agentContainer(container, "alpha-lead");
    expect(leader.querySelector('[data-ag-leader="true"]')).toBeTruthy();

    const follower = agentContainer(container, "alpha-follower");
    expect(follower.querySelector('[data-ag-leader="true"]')).toBeNull();

    const beta = agentContainer(container, "beta-agent");
    expect(beta.querySelector('[data-ag-leader="true"]')).toBeNull();

    // Workers never get the crown either.
    for (const id of ["alpha-w-live", "beta-w-live", "ghost-w"]) {
      const w = workerRow(container, id);
      expect(w.querySelector('[data-ag-leader="true"]')).toBeNull();
    }
  });

  it("(g) every visible worker (nested + orphan) carries the [worker] suffix", async () => {
    const { container } = renderNested();
    await waitFor(() => {
      expect(workerRow(container, "alpha-w-live")).toBeTruthy();
    });
    for (const id of ["alpha-w-live", "beta-w-live", "ghost-w"]) {
      const w = workerRow(container, id);
      expect(w.textContent).toContain("[worker]");
      expect(w.textContent).toContain("↳");
    }
  });
});
