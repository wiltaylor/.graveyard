/*
Skip gates:
- Tab ARIA and active-state behavior: e48778e1-5142-4910-900f-85f3e2da1b9a
*/
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import Tabs, { type TabItem } from "../components/ui/Tabs";

class MockResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

const items: TabItem[] = [
  { id: "team", label: "Team", icon: "users" },
  { id: "tasks", label: "Tasks", icon: "check" },
  { id: "cron", label: "Cron", icon: "clock" },
];

function Harness() {
  const [active, setActive] = createSignal("team");
  return <Tabs items={items} active={active()} onChange={setActive} />;
}

describe("Tabs ARIA invariants", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", MockResizeObserver);
    Element.prototype.scrollIntoView = vi.fn();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("renders a tablist with tab roles", () => {
    render(() => <Harness />);

    expect(screen.getByRole("tablist")).toBeTruthy();
    expect(screen.getAllByRole("tab")).toHaveLength(items.length);
  });

  it("marks only the active tab as selected", () => {
    render(() => <Harness />);

    expect(screen.getByRole("tab", { name: "Team" }).getAttribute("aria-selected")).toBe(
      "true",
    );
    expect(screen.getByRole("tab", { name: "Tasks" }).getAttribute("aria-selected")).toBe(
      "false",
    );

    fireEvent.click(screen.getByRole("tab", { name: "Tasks" }));

    expect(screen.getByRole("tab", { name: "Team" }).getAttribute("aria-selected")).toBe(
      "false",
    );
    expect(screen.getByRole("tab", { name: "Tasks" }).getAttribute("aria-selected")).toBe(
      "true",
    );
  });
});
