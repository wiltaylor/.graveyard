import { cleanup, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import CronView from "../CronView";
import type { WireCronJob } from "../../types";

const apiMock = vi.hoisted(() => ({
  listCron: vi.fn(),
}));

vi.mock("../../api", () => ({
  api: {
    listCron: apiMock.listCron,
  },
  cronEventsUrl: (id: string) => `/events/${id}/cron`,
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

const wclV5JobId = "fcd78ecf-3103-5533-8f3a-5384ca69bf63";

function cronJob(id: string): WireCronJob {
  return {
    id,
    target_agent: "nightly-maintainer",
    payload: { kind: "slash_command", command: "/maintain --dry-run" },
    schedule: { kind: "recurring", expression: "15 3 * * *" },
    timezone: "utc",
    catchup_on_startup: false,
    enabled: true,
    created_by: "operator",
    created_at: "2026-05-03T01:02:03Z",
    last_run: "2026-05-03T03:15:00Z",
    next_run: null,
  };
}

describe("CronView mixed UUID rendering", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", MockEventSource);
    apiMock.listCron.mockResolvedValue({
      ok: true,
      jobs: [cronJob(wclV5JobId), cronJob(crypto.randomUUID())],
    });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("cron_view_renders_wcl_v5_and_imperative_v4_jobs_with_identical_markup", async () => {
    const { container } = render(() => <CronView instanceId="inst-1" />);

    await waitFor(() => {
      expect(container.querySelectorAll("tbody tr")).toHaveLength(2);
    });

    expect(screen.getAllByText("nightly-maintainer")).toHaveLength(2);
    expect(screen.getAllByText("every 15 3 * * *")).toHaveLength(2);
    expect(screen.getAllByText("/maintain --dry-run")).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Edit" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Delete" })).toHaveLength(2);

    const rows = Array.from(container.querySelectorAll("tbody tr"));
    expect(rows).toHaveLength(2);
    expect(rows[0].outerHTML).toEqual(rows[1].outerHTML);
  });
});
