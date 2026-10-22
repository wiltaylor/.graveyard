import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import CronView from "../CronView";
import InboxView from "../InboxView";
import TasksView from "../TasksView";
import type { AgentRow } from "../../types";

const apiMock = vi.hoisted(() => ({
  listCron: vi.fn(),
  listInbox: vi.fn(),
  listTasks: vi.fn(),
}));

vi.mock("../../api", () => ({
  api: {
    listCron: apiMock.listCron,
    listInbox: apiMock.listInbox,
    listTasks: apiMock.listTasks,
  },
  cronEventsUrl: (id: string) => `/events/${id}/cron`,
  instanceEventsUrl: (id: string) => `/events/${id}`,
  tasksEventsUrl: (id: string) => `/events/${id}/tasks`,
}));

class MockEventSource {
  onopen: (() => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(readonly url: string) {}

  addEventListener(_type: string, _listener: EventListener): void {}

  removeEventListener(_type: string, _listener: EventListener): void {}

  close(): void {}
}

const agents: AgentRow[] = [
  {
    id: "agent-one",
    kind: "agent",
    status: "ready",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "none",
  },
  {
    id: "agent-two",
    kind: "agent",
    status: "ready",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "none",
  },
  {
    id: "script-auto",
    kind: "automaton",
    status: "ready",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "script",
  },
  {
    id: "web-auto",
    kind: "automaton",
    status: "ready",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "webservice",
  },
  {
    id: "remote-auto",
    kind: "automaton",
    status: "ready",
    parent: null,
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "remote",
  },
  {
    id: "worker-one",
    kind: "worker",
    status: "ready",
    parent: "agent-one",
    ai_core: null,
    leader: false,
    group: null,
    automaton_backend: "none",
  },
];

function expectOnlyEligibleMenuOptions(): void {
  const optionText = screen.getAllByRole("menuitem").map((el) => el.textContent);
  expect(optionText).toEqual(["agent-one", "agent-two", "script-auto", "remote-auto"]);
}

describe("target picker backend filters", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", MockEventSource);
    apiMock.listCron.mockResolvedValue({ ok: true, jobs: [] });
    apiMock.listInbox.mockResolvedValue({
      ok: true,
      messages: [{ from: "agent-one", body: "please route this", received_at: 1 }],
    });
    apiMock.listTasks.mockResolvedValue({ ok: true, tasks: [] });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("hides webservice automatons and workers from CronView target options", async () => {
    render(() => <CronView instanceId="inst-1" agents={agents} />);

    fireEvent.click(screen.getByRole("button", { name: "New cron job" }));
    fireEvent.click(screen.getByRole("button", { name: "Target agent" }));

    expectOnlyEligibleMenuOptions();
  });

  it("hides webservice automatons from InboxView forward options", async () => {
    render(() => <InboxView instanceId="inst-1" agents={agents} />);

    fireEvent.click(await screen.findByRole("button", { name: "Forward" }));
    fireEvent.click(screen.getByRole("button", { name: "Forward to" }));

    const optionText = screen.getAllByRole("menuitem").map((el) => el.textContent);
    expect(optionText).toEqual([
      "agent-one (agent)",
      "agent-two (agent)",
      "script-auto (automaton)",
      "remote-auto (automaton)",
    ]);
  });

  it("hides webservice automatons from TasksView assignee options", async () => {
    render(() => <TasksView instanceId="inst-1" agents={agents} />);

    fireEvent.click(screen.getByRole("button", { name: "New task" }));
    await waitFor(() => expect(screen.getByText("Assignee")).toBeTruthy());

    const select = screen.getByText("Assignee").parentElement?.querySelector("select");
    const options = Array.from(select?.querySelectorAll("option") ?? []).map(
      (el) => el.textContent,
    );
    expect(options).toContain("agent-one (agent)");
    expect(options).toContain("agent-two (agent)");
    expect(options).toContain("script-auto (automaton)");
    expect(options).toContain("remote-auto (automaton)");
    expect(options).not.toContain("web-auto (automaton)");
    expect(options).not.toContain("worker-one (worker)");
  });
});
