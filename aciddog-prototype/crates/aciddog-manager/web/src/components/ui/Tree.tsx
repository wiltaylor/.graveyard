import { createEffect, createMemo, createSignal, For, Show, type Accessor, type JSX } from "solid-js";
import Icon, { type IconName } from "./Icon";

export interface TreeNode {
  id: string;
  label: JSX.Element;
  icon?: IconName | (string & {});
  meta?: JSX.Element;
  actions?: JSX.Element;
  children?: TreeNode[];
  hasChildren?: boolean;
}

interface TreeProps {
  nodes: TreeNode[];
  selectedId?: string | null;
  expanded: Set<string>;
  onToggle: (id: string) => void;
  onSelect?: (id: string) => void;
  emptyMessage?: JSX.Element;
}

interface RowProps {
  node: TreeNode;
  depth: number;
  selectedId?: string | null;
  expanded: Set<string>;
  focusedId: string | null;
  setFocusedId: (id: string) => void;
  getVisible: Accessor<VisibleRow[]>;
  onToggle: (id: string) => void;
  onSelect?: (id: string) => void;
}

interface VisibleRow {
  id: string;
  depth: number;
  parentId: string | null;
  hasKids: boolean;
  expanded: boolean;
}

function flattenVisible(nodes: TreeNode[], expanded: Set<string>): VisibleRow[] {
  const rows: VisibleRow[] = [];

  const visit = (items: TreeNode[], depth: number, parentId: string | null) => {
    for (const node of items) {
      const hasKids = node.hasChildren ?? Boolean(node.children && node.children.length > 0);
      const isOpen = expanded.has(node.id);
      rows.push({
        id: node.id,
        depth,
        parentId,
        hasKids,
        expanded: isOpen,
      });
      if (hasKids && isOpen && node.children) {
        visit(node.children, depth + 1, node.id);
      }
    }
  };

  visit(nodes, 0, null);
  return rows;
}

function cssEscape(value: string): string {
  if (globalThis.CSS?.escape) return globalThis.CSS.escape(value);
  return value.replace(/["\\]/g, "\\$&");
}

function TreeRow(props: RowProps): JSX.Element {
  const hasKids = () =>
    props.node.hasChildren ?? Boolean(props.node.children && props.node.children.length > 0);
  const isOpen = () => props.expanded.has(props.node.id);

  const handleRowClick = () => {
    props.setFocusedId(props.node.id);
    props.onSelect?.(props.node.id);
  };
  const handleChevron = (ev: MouseEvent) => {
    ev.stopPropagation();
    if (hasKids()) props.onToggle(props.node.id);
  };
  const handleKeyDown = (ev: KeyboardEvent) => {
    const visible = props.getVisible();
    const index = visible.findIndex((row) => row.id === props.node.id);
    const current = visible[index];
    const focusRow = (id: string | null | undefined) => {
      if (id) props.setFocusedId(id);
    };

    // TODO: typeahead first-letter jump — out of scope for v1, see follow-up ticket.
    switch (ev.key) {
      case "Enter":
      case " ":
      case "Spacebar":
        ev.preventDefault();
        props.onSelect?.(props.node.id);
        break;
      case "ArrowDown":
        ev.preventDefault();
        if (current) focusRow(visible[index + 1]?.id ?? current.id);
        break;
      case "ArrowUp":
        ev.preventDefault();
        if (current) focusRow(visible[index - 1]?.id ?? current.id);
        break;
      case "ArrowRight":
        ev.preventDefault();
        if (hasKids() && !isOpen()) {
          props.onToggle(props.node.id);
        } else if (isOpen()) {
          focusRow(visible.find((row) => row.parentId === props.node.id)?.id);
        }
        break;
      case "ArrowLeft":
        ev.preventDefault();
        if (isOpen()) {
          props.onToggle(props.node.id);
        } else {
          focusRow(current?.parentId);
        }
        break;
      case "Home":
        ev.preventDefault();
        focusRow(visible[0]?.id);
        break;
      case "End":
        ev.preventDefault();
        focusRow(visible[visible.length - 1]?.id);
        break;
    }
  };

  return (
    <>
      <div
        class="tn"
        data-selected={props.selectedId === props.node.id ? "true" : "false"}
        data-id={props.node.id}
        role="treeitem"
        aria-selected={props.selectedId === props.node.id ? "true" : "false"}
        aria-level={props.depth + 1}
        aria-expanded={hasKids() ? (isOpen() ? "true" : "false") : undefined}
        tabIndex={props.focusedId === props.node.id ? 0 : -1}
        style={{ "padding-left": `${4 + props.depth * 14}px` }}
        onClick={handleRowClick}
        onDblClick={() => hasKids() && props.onToggle(props.node.id)}
        onKeyDown={handleKeyDown}
      >
        <span class="tn-chev" onClick={handleChevron} aria-hidden="true">
          <Show when={hasKids()}>
            <Icon
              name={isOpen() ? "chevron-down" : "chevron-right"}
              size={14}
              style={{ color: "var(--fg-2)" }}
            />
          </Show>
        </span>
        <span class="tn-icon">
          <Show when={props.node.icon}>
            {(n) => <Icon name={n()} size={14} />}
          </Show>
        </span>
        <span
          class="tn-label"
          style={{
            "white-space": "nowrap",
            overflow: "hidden",
            "text-overflow": "ellipsis",
          }}
        >
          {props.node.label}
        </span>
        <span class="tn-actions">
          <Show when={props.node.meta}>
            <span style={{ color: "var(--fg-2)", "font-size": "11px" }}>
              {props.node.meta}
            </span>
          </Show>
          <Show when={props.node.actions}>{props.node.actions}</Show>
        </span>
      </div>
      <Show when={isOpen() && props.node.children}>
        <For each={props.node.children}>
          {(child) => (
            <TreeRow
              node={child}
              depth={props.depth + 1}
              selectedId={props.selectedId}
              expanded={props.expanded}
              focusedId={props.focusedId}
              setFocusedId={props.setFocusedId}
              getVisible={props.getVisible}
              onToggle={props.onToggle}
              onSelect={props.onSelect}
            />
          )}
        </For>
      </Show>
    </>
  );
}

export default function Tree(props: TreeProps): JSX.Element {
  let treeEl!: HTMLDivElement;
  const visibleRows = createMemo(() => flattenVisible(props.nodes, props.expanded));
  const [focusedId, setFocusedId] = createSignal<string | null>(null);

  createEffect(() => {
    const visible = visibleRows();
    const focused = focusedId();
    if (focused && visible.some((row) => row.id === focused)) return;

    const selected = props.selectedId;
    const next =
      selected && visible.some((row) => row.id === selected) ? selected : visible[0]?.id ?? null;
    setFocusedId(next);
  });

  createEffect(() => {
    const id = focusedId();
    if (!id) return;

    queueMicrotask(() => {
      const root = treeEl;
      if (!root?.isConnected) return;
      const row = root.querySelector<HTMLElement>(`[data-id="${cssEscape(id)}"]`);
      if (!row) return;
      row.focus({ preventScroll: true });
      // scrollIntoView is missing in some test environments (e.g. jsdom);
      // guard so headless test suites don't see uncaught exceptions.
      if (typeof row.scrollIntoView === "function") {
        row.scrollIntoView({ block: "nearest" });
      }
    });
  });

  return (
    <div class="tree" role="tree" ref={treeEl}>
      <Show
        when={props.nodes.length > 0}
        fallback={
          <div
            style={{
              padding: "16px",
              "font-size": "12px",
              color: "var(--fg-2)",
            }}
          >
            {props.emptyMessage ?? "No items"}
          </div>
        }
      >
        <For each={props.nodes}>
          {(node) => (
            <TreeRow
              node={node}
              depth={0}
              selectedId={props.selectedId}
              expanded={props.expanded}
              focusedId={focusedId()}
              setFocusedId={setFocusedId}
              getVisible={visibleRows}
              onToggle={props.onToggle}
              onSelect={props.onSelect}
            />
          )}
        </For>
      </Show>
    </div>
  );
}
