import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Tree } from "../ui";
import type { TreeNode } from "../ui";

const nodes: TreeNode[] = [
  {
    id: "root",
    label: "Root",
    children: [
      { id: "child-a", label: "Child A" },
      {
        id: "branch",
        label: "Branch",
        children: [{ id: "grandchild", label: "Grandchild" }],
      },
    ],
  },
  { id: "sibling", label: "Sibling" },
];

function setup(options: { expanded?: string[]; selectedId?: string | null } = {}) {
  const [expanded, setExpanded] = createSignal(new Set(options.expanded ?? ["root"]));
  const onSelect = vi.fn();
  const onToggle = vi.fn((id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  });

  const result = render(() => (
    <Tree
      nodes={nodes}
      expanded={expanded()}
      selectedId={options.selectedId ?? null}
      onToggle={onToggle}
      onSelect={onSelect}
    />
  ));

  return { ...result, onSelect, onToggle };
}

function row(container: HTMLElement, id: string): HTMLElement {
  const el = container.querySelector<HTMLElement>(`[data-id="${id}"]`);
  if (!el) throw new Error(`Missing row ${id}`);
  return el;
}

function tabStops(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>('[tabindex="0"]'));
}

describe("Tree", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("keeps a single tab stop on the focused row", async () => {
    const { container } = setup({ selectedId: "child-a" });

    await waitFor(() => expect(tabStops(container).map((el) => el.dataset.id)).toEqual(["child-a"]));

    fireEvent.click(row(container, "sibling"));

    await waitFor(() => expect(tabStops(container).map((el) => el.dataset.id)).toEqual(["sibling"]));
  });

  it("walks visible rows with ArrowDown and ArrowUp", async () => {
    const { container } = setup();

    await waitFor(() => expect(document.activeElement).toBe(row(container, "root")));

    fireEvent.keyDown(row(container, "root"), { key: "ArrowDown" });
    await waitFor(() => expect(document.activeElement).toBe(row(container, "child-a")));

    fireEvent.keyDown(row(container, "child-a"), { key: "ArrowUp" });
    await waitFor(() => expect(document.activeElement).toBe(row(container, "root")));
  });

  it("expands collapsed parents with ArrowRight, then focuses the first child", async () => {
    const { container, onToggle } = setup({ expanded: [], selectedId: "root" });

    fireEvent.keyDown(row(container, "root"), { key: "ArrowRight" });

    await waitFor(() => expect(onToggle).toHaveBeenCalledWith("root"));
    await waitFor(() => expect(row(container, "child-a")).toBeTruthy());
    expect(document.activeElement).toBe(row(container, "root"));

    fireEvent.keyDown(row(container, "root"), { key: "ArrowRight" });

    await waitFor(() => expect(document.activeElement).toBe(row(container, "child-a")));
  });

  it("collapses expanded parents with ArrowLeft, then jumps to the parent", async () => {
    const { container, onToggle } = setup({ expanded: ["root"], selectedId: "root" });

    fireEvent.keyDown(row(container, "root"), { key: "ArrowLeft" });

    await waitFor(() => expect(onToggle).toHaveBeenCalledWith("root"));
    await waitFor(() => expect(container.querySelector('[data-id="child-a"]')).toBeNull());
    expect(document.activeElement).toBe(row(container, "root"));

    fireEvent.keyDown(row(container, "root"), { key: "ArrowRight" });
    await waitFor(() => expect(row(container, "child-a")).toBeTruthy());
    fireEvent.keyDown(row(container, "root"), { key: "ArrowRight" });
    await waitFor(() => expect(document.activeElement).toBe(row(container, "child-a")));

    fireEvent.keyDown(row(container, "child-a"), { key: "ArrowLeft" });

    await waitFor(() => expect(document.activeElement).toBe(row(container, "root")));
  });

  it("selects the focused row with Enter and Space", () => {
    const { container, onSelect } = setup({ selectedId: "child-a" });

    fireEvent.keyDown(row(container, "child-a"), { key: "Enter" });
    fireEvent.keyDown(row(container, "child-a"), { key: " " });

    expect(onSelect).toHaveBeenNthCalledWith(1, "child-a");
    expect(onSelect).toHaveBeenNthCalledWith(2, "child-a");
  });

  it("jumps to the first and last visible rows with Home and End", async () => {
    const { container } = setup({ expanded: ["root"], selectedId: "child-a" });

    await waitFor(() => expect(document.activeElement).toBe(row(container, "child-a")));

    fireEvent.keyDown(row(container, "child-a"), { key: "End" });
    await waitFor(() => expect(document.activeElement).toBe(row(container, "sibling")));

    fireEvent.keyDown(row(container, "sibling"), { key: "Home" });
    await waitFor(() => expect(document.activeElement).toBe(row(container, "root")));
  });

  it("renders treeitem roles and ARIA state only where appropriate", () => {
    const { container } = setup({ expanded: ["root"], selectedId: "child-a" });
    const items = screen.getAllByRole("treeitem");

    expect(items.map((item) => item.dataset.id)).toEqual(["root", "child-a", "branch", "sibling"]);
    expect(row(container, "root").getAttribute("aria-level")).toBe("1");
    expect(row(container, "child-a").getAttribute("aria-level")).toBe("2");
    expect(row(container, "child-a").getAttribute("aria-selected")).toBe("true");
    expect(row(container, "sibling").getAttribute("aria-selected")).toBe("false");
    expect(row(container, "root").getAttribute("aria-expanded")).toBe("true");
    expect(row(container, "branch").getAttribute("aria-expanded")).toBe("false");
    expect(row(container, "child-a").hasAttribute("aria-expanded")).toBe(false);
    expect(row(container, "sibling").hasAttribute("aria-expanded")).toBe(false);
  });
});
