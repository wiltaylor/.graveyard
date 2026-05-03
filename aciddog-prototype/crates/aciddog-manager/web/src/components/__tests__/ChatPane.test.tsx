import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ChatPane from "../ChatPane";

const apiMock = vi.hoisted(() => ({
  chatHistory: vi.fn(),
  sendAsUser: vi.fn(),
}));

vi.mock("../../api", () => ({
  api: {
    chatHistory: apiMock.chatHistory,
    sendAsUser: apiMock.sendAsUser,
  },
  chatEventsUrl: (id: string, agent: string) =>
    `/events/${id}/agents/${agent}`,
}));

/// Minimal EventSource polyfill for jsdom: registers handlers and lets
/// the test dispatch named events (`status`, `lagged`, default) by
/// invoking `dispatch(name, data)` from the most recent instance.
class MockEventSource {
  static instances: MockEventSource[] = [];

  onopen: (() => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;

  private namedListeners = new Map<string, Set<(ev: MessageEvent) => void>>();
  closed = false;

  constructor(readonly url: string) {
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: EventListener): void {
    let set = this.namedListeners.get(type);
    if (!set) {
      set = new Set();
      this.namedListeners.set(type, set);
    }
    set.add(listener as (ev: MessageEvent) => void);
  }

  removeEventListener(type: string, listener: EventListener): void {
    this.namedListeners.get(type)?.delete(listener as (ev: MessageEvent) => void);
  }

  close(): void {
    this.closed = true;
  }

  /// Fire a named SSE event (`status`, `lagged`) at every registered
  /// listener for that name. Default-named events should be delivered
  /// via the EventSource `onmessage` slot — call `dispatchMessage` for
  /// those.
  dispatch(name: string, data: unknown): void {
    const set = this.namedListeners.get(name);
    if (!set) return;
    const ev = new MessageEvent(name, { data: JSON.stringify(data) });
    for (const listener of set) {
      listener(ev);
    }
  }

  static latest(): MockEventSource {
    const last = MockEventSource.instances[MockEventSource.instances.length - 1];
    if (!last) throw new Error("no EventSource instance constructed");
    return last;
  }

  static reset(): void {
    MockEventSource.instances = [];
  }
}

describe("ChatPane animated thinking indicator", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", MockEventSource);
    MockEventSource.reset();
    apiMock.chatHistory.mockResolvedValue({ ok: true, blocks: [] });
    apiMock.sendAsUser.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    MockEventSource.reset();
  });

  it("starts hidden — no `thinking…` indicator on initial mount", async () => {
    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    // Wait for the EventSource subscription to fire so the createEffect
    // has settled before we assert absence.
    await waitFor(() => MockEventSource.latest());
    expect(queryByText(/thinking…/)).toBeNull();
  });

  it("shows the indicator when a `status: working` event arrives for the current agent", async () => {
    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    es.dispatch("status", { agent: "planner", status: "working" });

    await waitFor(() => {
      expect(queryByText(/thinking…/)).not.toBeNull();
    });
  });

  it("hides the indicator when a subsequent `status: ready` event arrives", async () => {
    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    es.dispatch("status", { agent: "planner", status: "working" });
    await waitFor(() => {
      expect(queryByText(/thinking…/)).not.toBeNull();
    });

    es.dispatch("status", { agent: "planner", status: "ready" });
    await waitFor(() => {
      expect(queryByText(/thinking…/)).toBeNull();
    });
  });

  it("ignores `status` events for a different agent (no leak across panes)", async () => {
    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    // Foreign-agent status frame: must NOT toggle the indicator.
    es.dispatch("status", { agent: "other-agent", status: "working" });

    // Allow Solid a tick to re-render before asserting absence.
    await new Promise((r) => setTimeout(r, 10));
    expect(queryByText(/thinking…/)).toBeNull();
  });
});
