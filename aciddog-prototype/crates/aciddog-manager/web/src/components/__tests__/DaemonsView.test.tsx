/*
Six cases covering the Daemons tab slice (task f5f708a1):
  1. renders_daemons_tab
  2. list_renders_two_rows_with_correct_pill_tones
  3. action_button_affordance_running_vs_stopped
  4. delete_confirm_flow
  5. scaffold_wizard_invalid_name_does_not_post
  6. scaffold_wizard_409_keeps_modal_open

Mock + render idiom mirrors PickerFilters.test.tsx — vi.hoisted() factory
plus vi.mock('../../api') so the module-level `api` import inside the
component closures resolves to the spy.
*/
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import AppShell from "../../AppShell";
import DaemonsView from "../daemons/DaemonsView";
import type {
  InstanceSummary,
  ManagedDaemonView,
} from "../../types";

const apiMock = vi.hoisted(() => ({
  listInstances: vi.fn(),
  listAgents: vi.fn(),
  listChannels: vi.fn(),
  health: vi.fn(),
  listManagedDaemons: vi.fn(),
  startManagedDaemon: vi.fn(),
  stopManagedDaemon: vi.fn(),
  reloadManagedDaemon: vi.fn(),
  deleteManagedDaemon: vi.fn(),
  createManagedDaemon: vi.fn(),
}));

vi.mock("../../api", () => ({
  api: {
    listInstances: apiMock.listInstances,
    listAgents: apiMock.listAgents,
    listChannels: apiMock.listChannels,
    health: apiMock.health,
    listManagedDaemons: apiMock.listManagedDaemons,
    startManagedDaemon: apiMock.startManagedDaemon,
    stopManagedDaemon: apiMock.stopManagedDaemon,
    reloadManagedDaemon: apiMock.reloadManagedDaemon,
    deleteManagedDaemon: apiMock.deleteManagedDaemon,
    createManagedDaemon: apiMock.createManagedDaemon,
  },
  request: vi.fn(),
  cronEventsUrl: (id: string) => `/events/${id}/cron`,
  instanceEventsUrl: (id: string) => `/events/${id}`,
  channelEventsUrl: (id: string, ch: string) => `/events/${id}/channels/${ch}`,
  chatEventsUrl: (id: string, agent: string) => `/events/${id}/agents/${agent}`,
  workflowEventsUrl: (id: string) => `/events/${id}/workflows`,
}));

vi.mock("../../auth", () => ({
  authApi: {
    whoami: vi.fn().mockResolvedValue({
      username: "tester",
      is_daemon_runner_enabled: true,
    }),
    logout: vi.fn().mockResolvedValue(undefined),
  },
}));

class MockEventSource {
  onopen: (() => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(readonly url: string) {}
  addEventListener(_t: string, _l: EventListener): void {}
  removeEventListener(_t: string, _l: EventListener): void {}
  close(): void {}
}

class MockResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

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

function makeDaemon(overrides: Partial<ManagedDaemonView>): ManagedDaemonView {
  return {
    id: "daemon-id-default",
    paired_instance_id: null,
    name: "default",
    work_dir: "/tmp/default",
    desired_state: "stopped",
    observed_state: "stopped",
    socket_path: "/tmp/default.sock",
    last_error: null,
    created_at: "2026-05-04T00:00:00Z",
    ...overrides,
  };
}

const RUNNING_DAEMON: ManagedDaemonView = makeDaemon({
  id: "d-running",
  name: "alpha",
  work_dir: "/srv/alpha",
  desired_state: "running",
  observed_state: "running",
  socket_path: "/tmp/alpha.sock",
  paired_instance_id: "inst-paired-alpha",
});

const STOPPED_DAEMON: ManagedDaemonView = makeDaemon({
  id: "d-stopped",
  name: "bravo",
  work_dir: "/srv/bravo",
  desired_state: "stopped",
  observed_state: "stopped",
  socket_path: "/tmp/bravo.sock",
});

beforeEach(() => {
  vi.stubGlobal("EventSource", MockEventSource);
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
  Element.prototype.scrollIntoView = vi.fn();

  apiMock.listInstances.mockResolvedValue([instance]);
  apiMock.listAgents.mockResolvedValue({
    ok: true,
    agents: [],
    raw: null,
    error: null,
  });
  apiMock.listChannels.mockResolvedValue({
    ok: true,
    channels: [],
    error: null,
  });
  apiMock.health.mockResolvedValue({ ok: true, status: 200, body: "ok" });
  apiMock.listManagedDaemons.mockResolvedValue([]);
  apiMock.startManagedDaemon.mockResolvedValue(makeDaemon({}));
  apiMock.stopManagedDaemon.mockResolvedValue(makeDaemon({}));
  apiMock.reloadManagedDaemon.mockResolvedValue(makeDaemon({}));
  apiMock.deleteManagedDaemon.mockResolvedValue(undefined);
  apiMock.createManagedDaemon.mockResolvedValue(makeDaemon({}));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// Click the instance label inside the InstanceTree so AppShell flips
// `selectedId` and renders the per-instance tab strip. Returns when
// the tab strip is on screen.
async function selectFixtureInstance(): Promise<void> {
  // The InstanceTree label "fixture" is rendered as plain text inside
  // a clickable row. Look it up by text and click it.
  const row = await screen.findByText("fixture");
  fireEvent.click(row);
  // Wait for at least one tab to appear (the strip only mounts under
  // <Show when={selectedInstance()}>).
  await waitFor(() => {
    expect(screen.getByRole("tab", { name: "Team" })).toBeTruthy();
  });
}

describe("Daemons tab rendering", () => {
  it("renders_daemons_tab", async () => {
    render(() => <AppShell username="tester" onLoggedOut={() => {}} />);

    await selectFixtureInstance();

    expect(screen.getByRole("tab", { name: /Daemons/i })).toBeTruthy();
  });
});

describe("Daemons list rendering + action affordance", () => {
  it("list_renders_two_rows_with_correct_pill_tones", async () => {
    apiMock.listManagedDaemons.mockResolvedValue([
      RUNNING_DAEMON,
      STOPPED_DAEMON,
    ]);

    const { container } = render(() => (
      <DaemonsView onOpenInChat={() => {}} />
    ));

    // Two table rows.
    await waitFor(() => {
      expect(
        container.querySelectorAll("tbody [data-daemon-id]"),
      ).toHaveLength(2);
    });

    // Names + work_dirs are visible.
    expect(screen.getByText("alpha")).toBeTruthy();
    expect(screen.getByText("bravo")).toBeTruthy();
    expect(screen.getByText("/srv/alpha")).toBeTruthy();
    expect(screen.getByText("/srv/bravo")).toBeTruthy();

    // The running row's observed pill carries fbadge-success; the
    // stopped row's observed pill carries fbadge-neutral.
    const runRow = container.querySelector<HTMLElement>(
      '[data-daemon-id="d-running"]',
    );
    const stopRow = container.querySelector<HTMLElement>(
      '[data-daemon-id="d-stopped"]',
    );
    expect(runRow).toBeTruthy();
    expect(stopRow).toBeTruthy();
    const runBadgeClasses = Array.from(
      runRow!.querySelectorAll<HTMLElement>(".fbadge"),
    ).map((el) => el.className);
    const stopBadgeClasses = Array.from(
      stopRow!.querySelectorAll<HTMLElement>(".fbadge"),
    ).map((el) => el.className);
    expect(runBadgeClasses.some((c) => c.includes("fbadge-success"))).toBe(true);
    expect(runBadgeClasses.some((c) => c.includes("fbadge-info"))).toBe(true);
    expect(stopBadgeClasses.some((c) => c.includes("fbadge-neutral"))).toBe(
      true,
    );
  });

  it("action_button_affordance_running_vs_stopped", async () => {
    apiMock.listManagedDaemons.mockResolvedValue([
      RUNNING_DAEMON,
      STOPPED_DAEMON,
    ]);

    render(() => <DaemonsView onOpenInChat={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("alpha")).toBeTruthy();
      expect(screen.getByText("bravo")).toBeTruthy();
    });

    // Running row (alpha) — Stop + Reload visible, Start absent.
    expect(
      screen.getByLabelText("Stop daemon alpha"),
    ).toBeTruthy();
    expect(
      screen.getByLabelText("Reload daemon alpha"),
    ).toBeTruthy();
    expect(screen.queryByLabelText("Start daemon alpha")).toBeNull();

    // Stopped row (bravo) — Start visible, Stop + Reload absent.
    expect(screen.getByLabelText("Start daemon bravo")).toBeTruthy();
    expect(screen.queryByLabelText("Stop daemon bravo")).toBeNull();
    expect(screen.queryByLabelText("Reload daemon bravo")).toBeNull();
  });
});

describe("Daemons delete-confirm flow", () => {
  it("delete_confirm_flow", async () => {
    apiMock.listManagedDaemons.mockResolvedValue([RUNNING_DAEMON]);

    render(() => <DaemonsView onOpenInChat={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("alpha")).toBeTruthy();
    });

    // Open the confirm modal.
    fireEvent.click(screen.getByLabelText("Delete daemon alpha"));
    await waitFor(() => {
      expect(
        screen.getByRole("dialog"),
      ).toBeTruthy();
    });
    // Clicking Cancel closes the modal without invoking the API.
    fireEvent.click(
      screen.getByLabelText("Cancel deleting daemon alpha"),
    );
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(apiMock.deleteManagedDaemon).not.toHaveBeenCalled();

    // Re-open and confirm.
    fireEvent.click(screen.getByLabelText("Delete daemon alpha"));
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeTruthy();
    });

    // Body copy verbatim.
    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain(
      "This stops the daemon (SIGTERM, 5s grace, SIGKILL fallback)",
    );
    expect(dialog.textContent).toContain("This cannot be undone.");

    // Clear the listManagedDaemons call count from the polling path
    // before the refetch trigger.
    apiMock.listManagedDaemons.mockClear();
    apiMock.listManagedDaemons.mockResolvedValue([]);

    fireEvent.click(
      screen.getByLabelText("Confirm delete daemon alpha"),
    );

    await waitFor(() => {
      expect(apiMock.deleteManagedDaemon).toHaveBeenCalledTimes(1);
    });
    expect(apiMock.deleteManagedDaemon).toHaveBeenCalledWith("d-running");

    // The post-delete force-refetch should fire.
    await waitFor(() => {
      expect(apiMock.listManagedDaemons).toHaveBeenCalled();
    });
  });
});

describe("ScaffoldWizard validation + 409 handling", () => {
  it("scaffold_wizard_invalid_name_does_not_post", async () => {
    apiMock.listManagedDaemons.mockResolvedValue([]);

    render(() => <DaemonsView onOpenInChat={() => {}} />);

    // Open the wizard.
    fireEvent.click(screen.getByRole("button", { name: /Scaffold daemon/i }));
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeTruthy();
    });

    // Type a clearly invalid name.
    const dialog = screen.getByRole("dialog");
    const nameInput = dialog.querySelector<HTMLInputElement>(
      'input[placeholder="my-daemon"]',
    );
    expect(nameInput, "name input present").toBeTruthy();
    fireEvent.input(nameInput!, { target: { value: "BadName!" } });

    // Click submit.
    const submitButtons = Array.from(
      dialog.querySelectorAll<HTMLButtonElement>("button"),
    ).filter((b) => /Scaffold daemon/i.test(b.textContent ?? ""));
    expect(submitButtons.length).toBeGreaterThan(0);
    fireEvent.click(submitButtons[0]);

    // No POST should have been issued.
    expect(apiMock.createManagedDaemon).not.toHaveBeenCalled();

    // Inline error renders under the name field — Input renders the
    // help text in a `.ffield-help.is-error` span when `error` is set.
    await waitFor(() => {
      expect(
        dialog.querySelector(".ffield-help.is-error"),
      ).toBeTruthy();
    });
  });

  it("scaffold_wizard_409_keeps_modal_open", async () => {
    apiMock.listManagedDaemons.mockResolvedValue([]);
    apiMock.createManagedDaemon.mockRejectedValueOnce(
      new Error('daemon name "foo" already exists'),
    );

    render(() => <DaemonsView onOpenInChat={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: /Scaffold daemon/i }));
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeTruthy();
    });

    const dialog = screen.getByRole("dialog");
    const nameInput = dialog.querySelector<HTMLInputElement>(
      'input[placeholder="my-daemon"]',
    );
    fireEvent.input(nameInput!, { target: { value: "foo" } });

    const submitButtons = Array.from(
      dialog.querySelectorAll<HTMLButtonElement>("button"),
    ).filter((b) => /Scaffold daemon/i.test(b.textContent ?? ""));
    fireEvent.click(submitButtons[0]);

    await waitFor(() => {
      expect(apiMock.createManagedDaemon).toHaveBeenCalledTimes(1);
    });

    // Modal stays open.
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeTruthy();
    });

    // The error string renders under the name field.
    await waitFor(() => {
      const helpEl = dialog.querySelector<HTMLElement>(
        ".ffield-help.is-error",
      );
      expect(helpEl).toBeTruthy();
      expect(helpEl?.textContent).toContain('daemon name "foo" already exists');
    });

    // Input value preserved.
    expect(nameInput?.value).toBe("foo");
  });
});
