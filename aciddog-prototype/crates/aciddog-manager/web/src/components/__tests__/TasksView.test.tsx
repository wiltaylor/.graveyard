import { cleanup, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import TasksView from "../TasksView";
import type { AgentRow, Task, TaskEvent } from "../../types";

/// Promise + external resolver pair, used to interleave SSE
/// dispatches between the moment `listTasks()` is invoked and the
/// moment its response actually lands. Mirrors the helper from
/// `ChatPane.test.tsx` — vitest's `mockResolvedValue` resolves on the
/// next tick, which is too eager to reproduce the initial-load race.
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

function makeTask(id: string, title: string): Task {
  return {
    id,
    title,
    status: "open",
    created_by: "alice",
    created_at_secs: 0,
    updated_at_secs: 0,
    notes: [],
    blocked_by: [],
    todos: [],
    comments: [],
  };
}

const apiMock = vi.hoisted(() => ({
  listTasks: vi.fn(),
}));

vi.mock("../../api", () => ({
  api: {
    listTasks: apiMock.listTasks,
  },
  tasksEventsUrl: (id: string) => `/events/${id}/tasks`,
}));

/// Minimal EventSource polyfill for jsdom — copied verbatim from
/// `ChatPane.test.tsx`'s polyfill. (Lifting it to a shared util would
/// reduce duplication; left as a follow-up to match test_lead's
/// existing convention.)
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

  dispatch(name: string, data: unknown): void {
    const set = this.namedListeners.get(name);
    if (!set) return;
    const ev = new MessageEvent(name, { data: JSON.stringify(data) });
    for (const listener of set) {
      listener(ev);
    }
  }

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

const NO_AGENTS: AgentRow[] = [];

/// `TasksView` renders the kanban grid (desktop) and the mobile list
/// in parallel, both bound to the same `tasks()` signal. Both surface
/// the task title verbatim, so `queryAllByText` returns one DOM node
/// per task per view. We assert "≥ 1 match" to mean "rendered at all"
/// rather than depending on the responsive viewport.
function expectTitlePresent(matches: HTMLElement[]) {
  expect(matches.length).toBeGreaterThan(0);
}

function expectTitleAbsent(matches: HTMLElement[]) {
  expect(matches.length).toBe(0);
}

describe("TasksView SSE subscription", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", MockEventSource);
    MockEventSource.reset();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    MockEventSource.reset();
  });

  /// Race-fix guard: a `created` event arriving BEFORE the initial
  /// listTasks() resolves must be buffered, then drained AFTER history
  /// applies. Removing `historyReady` / `pendingEvents` from the
  /// effect closure makes this case fail.
  it("buffers a live created event until initial history resolves", async () => {
    const taskA = makeTask("task-a", "history-task");
    const taskBuffered = makeTask("task-buf", "buffered-task");

    const initial = deferred<{ ok: true; tasks: Task[] }>();
    apiMock.listTasks.mockReturnValueOnce(initial.promise);

    const { queryAllByText } = render(() => (
      <TasksView instanceId="inst-1" agents={NO_AGENTS} />
    ));

    const es = await waitFor(() => MockEventSource.latest());

    // Live event BEFORE history lands — should be buffered.
    const ev: TaskEvent = { kind: "created", task: taskBuffered };
    es.dispatchMessage(ev);

    // Pre-resolve assertion: the buffered task must NOT be visible yet.
    expectTitleAbsent(queryAllByText(/buffered-task/));

    initial.resolve({ ok: true, tasks: [taskA] });

    await waitFor(() => {
      expectTitlePresent(queryAllByText(/history-task/));
      expectTitlePresent(queryAllByText(/buffered-task/));
    });
  });

  /// `updated` for a known id must replace the row in place — no
  /// duplication.
  it("applies updated events as in-place replace by id", async () => {
    const taskA = makeTask("task-a", "title-old");
    apiMock.listTasks.mockResolvedValueOnce({ ok: true, tasks: [taskA] });

    const { queryAllByText } = render(() => (
      <TasksView instanceId="inst-1" agents={NO_AGENTS} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    await waitFor(() => {
      expectTitlePresent(queryAllByText(/title-old/));
    });

    const renamed: Task = { ...taskA, title: "title-new" };
    const ev: TaskEvent = { kind: "updated", task: renamed };
    es.dispatchMessage(ev);

    await waitFor(() => {
      expectTitlePresent(queryAllByText(/title-new/));
    });
    // Old title is gone (replace, not duplicate).
    expectTitleAbsent(queryAllByText(/title-old/));
  });

  /// `removed` must splice the row out of local state.
  it("applies removed events by splicing the matching id", async () => {
    const taskA = makeTask("task-a", "alpha-row");
    const taskB = makeTask("task-b", "beta-row");
    apiMock.listTasks.mockResolvedValueOnce({
      ok: true,
      tasks: [taskA, taskB],
    });

    const { queryAllByText } = render(() => (
      <TasksView instanceId="inst-1" agents={NO_AGENTS} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    await waitFor(() => {
      expectTitlePresent(queryAllByText(/alpha-row/));
      expectTitlePresent(queryAllByText(/beta-row/));
    });

    const ev: TaskEvent = { kind: "removed", id: taskA.id };
    es.dispatchMessage(ev);

    await waitFor(() => {
      expectTitleAbsent(queryAllByText(/alpha-row/));
    });
    expectTitlePresent(queryAllByText(/beta-row/));
  });

  /// `lagged` clears the buffer AND triggers a fresh listTasks. The
  /// pre-lagged state must be replaced by whatever the REST endpoint
  /// returned on the resync.
  it("on lagged: clears the buffer and reloads from REST", async () => {
    const taskA = makeTask("task-a", "before-lagged");
    apiMock.listTasks.mockResolvedValueOnce({ ok: true, tasks: [taskA] });

    const second = deferred<{ ok: true; tasks: Task[] }>();
    apiMock.listTasks.mockReturnValueOnce(second.promise);

    const { queryAllByText } = render(() => (
      <TasksView instanceId="inst-1" agents={NO_AGENTS} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    await waitFor(() => {
      expect(apiMock.listTasks).toHaveBeenCalledTimes(1);
      expectTitlePresent(queryAllByText(/before-lagged/));
    });

    // `lagged` flips historyReady -> false and clears pendingEvents.
    es.dispatch("lagged", { dropped: 3 });

    // Resolve the second listTasks with a different fixture.
    const taskC = makeTask("task-c", "after-lagged");
    second.resolve({ ok: true, tasks: [taskC] });

    await waitFor(() => {
      expect(apiMock.listTasks).toHaveBeenCalledTimes(2);
      expectTitlePresent(queryAllByText(/after-lagged/));
    });
    // The pre-lagged state must be gone — the resync replaced the
    // entire list with what the REST endpoint just returned.
    expectTitleAbsent(queryAllByText(/before-lagged/));
  });

  /// Loud failure: an `updated` carrying an id we don't know about
  /// during the live phase should fire a console.warn and trigger a
  /// reload from REST. (decisions/59ff479f loud-failure ethos.)
  it("on updated for unknown id: warns once and triggers reload", async () => {
    const taskA = makeTask("task-a", "live-task");
    apiMock.listTasks.mockResolvedValueOnce({ ok: true, tasks: [taskA] });

    const second = deferred<{ ok: true; tasks: Task[] }>();
    apiMock.listTasks.mockReturnValueOnce(second.promise);

    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    const { queryAllByText } = render(() => (
      <TasksView instanceId="inst-1" agents={NO_AGENTS} />
    ));

    const es = await waitFor(() => MockEventSource.latest());
    await waitFor(() => {
      expect(apiMock.listTasks).toHaveBeenCalledTimes(1);
      expectTitlePresent(queryAllByText(/live-task/));
    });

    const stranger = makeTask("task-stranger", "stranger-task");
    const ev: TaskEvent = { kind: "updated", task: stranger };
    es.dispatchMessage(ev);

    // Reload must have been kicked off.
    await waitFor(() => {
      expect(apiMock.listTasks).toHaveBeenCalledTimes(2);
    });

    // Exactly one [TasksView] warn fired for the unknown id.
    const tasksViewWarns = warn.mock.calls.filter(
      (call) => typeof call[0] === "string" && call[0].includes("[TasksView]"),
    );
    expect(tasksViewWarns).toHaveLength(1);
    expect(tasksViewWarns[0][0]).toContain("updated event for unknown id");

    second.resolve({ ok: true, tasks: [taskA] });
    warn.mockRestore();
  });
});
