import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ChatPane from "../ChatPane";
import type { ChatBlockEvent, WireChatBlock } from "../../types";

/// Promise + external resolver pair, used to interleave SSE
/// dispatches between the moment `chatHistory()` is invoked and the
/// moment its response actually lands. Vitest's `mockResolvedValue`
/// resolves on the next tick, which is too eager to reproduce the
/// initial-load race.
function deferred<T>(): {
  promise: Promise<T>;
  resolve: (v: T) => void;
  reject: (e: unknown) => void;
} {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/// Tiny factory: a `user_text` block renders as a chat message whose
/// `text` shows up verbatim in the DOM, making it trivially queryable.
function userText(text: string): WireChatBlock {
  return { kind: "user_text", text };
}

/// Wrap a block as a `ChatBlockEvent` with the provided `seq`.
function blockEvent(agent: string, seq: number, block: WireChatBlock): ChatBlockEvent {
  return { agent, block, ts: new Date().toISOString(), seq };
}

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

  /// Fire a default-named (`message`) SSE event at the `onmessage`
  /// slot. Mirrors the wire shape: a single JSON-serialised payload
  /// in `MessageEvent.data`. Pairs with `dispatch` for named events.
  dispatchMessage(data: unknown): void {
    const ev = new MessageEvent("message", { data: JSON.stringify(data) });
    this.onmessage?.(ev);
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

describe("ChatPane initial-load / lagged-resync race", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", MockEventSource);
    MockEventSource.reset();
    apiMock.sendAsUser.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    MockEventSource.reset();
  });

  it("buffers live events until initial history resolves", async () => {
    // chatHistory hangs until we explicitly resolve. This reproduces
    // the window where the SSE subscription is live but the initial
    // /history fetch is still in flight — a live event in this gap
    // used to be appended at the wrong index and clobbered by the
    // upcoming setBlocks(resp.blocks).
    const initial = deferred<{ ok: true; blocks: WireChatBlock[] }>();
    apiMock.chatHistory.mockReturnValueOnce(initial.promise);

    const history: WireChatBlock[] = [
      userText("history-zero"),
      userText("history-one"),
    ];

    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    const es = await waitFor(() => MockEventSource.latest());

    // Live event arriving BEFORE history lands. seq === eventual
    // history length, so once history applies the buffered event
    // appends at slot 2.
    es.dispatchMessage(blockEvent("planner", history.length, userText("live-buffered")));

    // Pre-resolve assertion: nothing rendered yet (blocks were
    // initialised to `[]` and the live event is buffered, not
    // applied — so it must NOT be visible).
    expect(queryByText(/live-buffered/)).toBeNull();

    initial.resolve({ ok: true, blocks: history });

    await waitFor(() => {
      expect(queryByText(/history-zero/)).not.toBeNull();
      expect(queryByText(/history-one/)).not.toBeNull();
      expect(queryByText(/live-buffered/)).not.toBeNull();
    });
  });

  it("detects a gap and triggers a re-sync", async () => {
    // First chatHistory resolves immediately with N=2 blocks.
    const history0: WireChatBlock[] = [
      userText("h0"),
      userText("h1"),
    ];
    // After the gap re-sync, the daemon catches us up with N+3
    // blocks — including the missed gap-block at slot 2 and a
    // placeholder at slot 4 that the buffered live event will then
    // replace via the pendingEvents drain. Sized so the buffered
    // event's seq (4) is within bounds (length 5) → REPLACE branch,
    // not another gap loop.
    const history1: WireChatBlock[] = [
      userText("h0"),
      userText("h1"),
      userText("gap-block"),
      userText("filler-3"),
      userText("placeholder-4"),
    ];

    // First call: immediate resolve. Second call: deferred — we want
    // to verify the buffered live event survives the resync drain.
    const second = deferred<{ ok: true; blocks: WireChatBlock[] }>();
    apiMock.chatHistory
      .mockResolvedValueOnce({ ok: true, blocks: history0 })
      .mockReturnValueOnce(second.promise);

    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    const es = await waitFor(() => MockEventSource.latest());

    // Wait for the initial history to render.
    await waitFor(() => {
      expect(queryByText(/h0/)).not.toBeNull();
      expect(queryByText(/h1/)).not.toBeNull();
    });

    // Real gap: seq = N + 2 (length is 2, so we'd expect seq 2 next).
    const gapSeq = history0.length + 2;
    es.dispatchMessage(blockEvent("planner", gapSeq, userText("future-live")));

    // Gap path must call chatHistory a second time (re-sync) and
    // log a loud warning (decisions/59ff479f loud-failure ethos).
    await waitFor(() => {
      expect(apiMock.chatHistory).toHaveBeenCalledTimes(2);
    });
    expect(warn).toHaveBeenCalledWith(
      "[ChatPane] gap detected",
      expect.objectContaining({ agent: "planner", seq: gapSeq, length: 2 }),
    );

    // Second history brings the gap-block plus a placeholder at the
    // buffered event's seq. After history lands, the pendingEvents
    // drain re-applies the buffered future-live block at slot 4
    // (REPLACE), and the placeholder is gone.
    second.resolve({ ok: true, blocks: history1 });

    await waitFor(() => {
      expect(queryByText(/gap-block/)).not.toBeNull();
      expect(queryByText(/future-live/)).not.toBeNull();
    });
    // The placeholder at slot 4 must have been replaced by the
    // drained live event — confirms the drain actually fired.
    expect(queryByText(/placeholder-4/)).toBeNull();

    warn.mockRestore();
  });

  it("buffers live events across a lagged re-sync", async () => {
    // Initial history is empty; the lagged re-sync arrives later.
    const second = deferred<{ ok: true; blocks: WireChatBlock[] }>();
    apiMock.chatHistory
      .mockResolvedValueOnce({ ok: true, blocks: [] })
      .mockReturnValueOnce(second.promise);

    const { queryByText } = render(() => (
      <ChatPane instanceId="inst-1" agent="planner" onClose={() => {}} />
    ));

    const es = await waitFor(() => MockEventSource.latest());

    // Let initial history settle (historyReady flips true).
    await waitFor(() => {
      expect(apiMock.chatHistory).toHaveBeenCalledTimes(1);
    });

    // Lagged → triggers a second chatHistory call. historyReady flips
    // back to false; pendingEvents is cleared.
    es.dispatch("lagged", { dropped: 3 });
    await waitFor(() => {
      expect(apiMock.chatHistory).toHaveBeenCalledTimes(2);
    });

    // Live event arriving DURING the resync window — must be
    // buffered (not applied at the wrong index) so that when the
    // post-lagged history lands, it doesn't clobber the live event.
    es.dispatchMessage(blockEvent("planner", 1, userText("post-lagged-live")));

    expect(queryByText(/post-lagged-live/)).toBeNull();

    second.resolve({ ok: true, blocks: [userText("after-lagged")] });

    await waitFor(() => {
      expect(queryByText(/after-lagged/)).not.toBeNull();
      expect(queryByText(/post-lagged-live/)).not.toBeNull();
    });
  });
});
