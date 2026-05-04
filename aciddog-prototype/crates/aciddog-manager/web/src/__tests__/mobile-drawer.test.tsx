/*
Skip gates:
- Drawer behavior: ec979f18-db77-4ee2-b75b-9b1dfa64c62b
- Breakpoint contract: 40aa10ce-86b7-4e17-8c0c-1fd9bfacc1c6
*/
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import AppShell from "../AppShell";

const apiMock = vi.hoisted(() => ({
  listInstances: vi.fn(),
  listAgents: vi.fn(),
  listChannels: vi.fn(),
}));

const authMock = vi.hoisted(() => ({
  logout: vi.fn(),
}));

vi.mock("../api", () => ({
  api: {
    listInstances: apiMock.listInstances,
    listAgents: apiMock.listAgents,
    listChannels: apiMock.listChannels,
  },
}));

vi.mock("../auth", () => ({
  authApi: {
    logout: authMock.logout,
  },
}));

function stubMobileMatchMedia() {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("max-width: 768px"),
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

describe("mobile drawer ARIA and focus", () => {
  beforeEach(() => {
    stubMobileMatchMedia();
    apiMock.listInstances.mockResolvedValue([]);
    apiMock.listAgents.mockResolvedValue({ agents: [], ok: true, raw: null, error: null });
    apiMock.listChannels.mockResolvedValue({ channels: [], ok: true, error: null });
    authMock.logout.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("toggles aria-controls and aria-expanded on the hamburger", async () => {
    render(() => (
      <AppShell username="operator" onLoggedOut={vi.fn()} />
    ));

    const trigger = screen.getByRole("button", { name: "Open navigation" });
    expect(trigger.getAttribute("aria-controls")).toBe("app-sidebar");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(trigger);

    await waitFor(() => {
      expect(
        screen
          .getByRole("button", { name: "Close navigation" })
          .getAttribute("aria-expanded"),
      ).toBe("true");
    });
  });

  it("restores focus to the hamburger after backdrop dismissal", async () => {
    render(() => (
      <AppShell username="operator" onLoggedOut={vi.fn()} />
    ));

    const trigger = screen.getByRole("button", { name: "Open navigation" });
    fireEvent.click(trigger);

    const backdrop = document.querySelector<HTMLElement>(".fsidebar-drawer-backdrop");
    expect(backdrop).toBeTruthy();
    fireEvent.click(backdrop!);

    await waitFor(() => {
      expect(document.activeElement).toBe(
        screen.getByRole("button", { name: "Open navigation" }),
      );
    });
  });
});
