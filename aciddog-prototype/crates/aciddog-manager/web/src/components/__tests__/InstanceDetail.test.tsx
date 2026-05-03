import { cleanup, render, waitFor } from "@solidjs/testing-library";
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

function rowFor(container: HTMLElement, agentId: string): HTMLElement {
  const tds = Array.from(container.querySelectorAll<HTMLElement>("tbody td"));
  for (const td of tds) {
    if (td.textContent?.includes(agentId)) {
      const tr = td.closest("tr");
      if (tr) return tr as HTMLElement;
    }
  }
  throw new Error(`No row found containing id ${agentId}`);
}

describe("InstanceDetail agent rows mirror TUI palette", () => {
  beforeEach(() => {
    apiMock.health.mockResolvedValue(health);
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("renders five distinct badge tones and the matching glyph for every agent kind/status", async () => {
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
      expect(container.querySelectorAll("tbody tr")).toHaveLength(6);
    });

    // Status column: each row gets the matching tone class + glyph.
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
        id: "snoozed-worker",
        toneClass: "fbadge-neutral",
        glyph: "💤",
        label: "hibernated",
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
      const dotBadge = row.querySelector<HTMLElement>(".fbadge-dot")?.parentElement;
      expect(dotBadge, `dotted badge for ${id}`).toBeTruthy();
      expect(dotBadge?.className).toContain(toneClass);
      expect(dotBadge?.textContent).toContain(glyph);
      expect(dotBadge?.textContent).toContain(label);
    }
  });

  it("indents the worker row with ↳ and a [worker] suffix", async () => {
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
      expect(container.querySelectorAll("tbody tr")).toHaveLength(6);
    });

    const workerRow = rowFor(container, "snoozed-worker");
    const idCell = workerRow.querySelector<HTMLElement>("td");
    expect(idCell?.textContent).toContain("↳");
    expect(idCell?.textContent).toContain("[worker]");
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
      expect(container.querySelectorAll("tbody tr")).toHaveLength(6);
    });

    const leaderRow = rowFor(container, "lead-agent");
    expect(leaderRow.textContent).toContain("👑");

    const otherIds = [
      "busy-agent",
      "broken-agent",
      "snoozed-worker",
      "sleepy-bot",
      "ready-bot",
    ];
    for (const id of otherIds) {
      const row = rowFor(container, id);
      expect(row.textContent ?? "").not.toContain("👑");
    }
  });
});
