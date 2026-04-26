import { For, Show, type JSX } from "solid-js";
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
  onToggle: (id: string) => void;
  onSelect?: (id: string) => void;
}

function TreeRow(props: RowProps): JSX.Element {
  const hasKids = () =>
    props.node.hasChildren ?? Boolean(props.node.children && props.node.children.length > 0);
  const isOpen = () => props.expanded.has(props.node.id);

  const handleRowClick = () => {
    props.onSelect?.(props.node.id);
  };
  const handleChevron = (ev: MouseEvent) => {
    ev.stopPropagation();
    if (hasKids()) props.onToggle(props.node.id);
  };

  return (
    <>
      <div
        class="tn"
        data-selected={props.selectedId === props.node.id ? "true" : "false"}
        style={{ "padding-left": `${4 + props.depth * 14}px` }}
        onClick={handleRowClick}
        onDblClick={() => hasKids() && props.onToggle(props.node.id)}
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
  return (
    <div class="tree" role="tree">
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
              onToggle={props.onToggle}
              onSelect={props.onSelect}
            />
          )}
        </For>
      </Show>
    </div>
  );
}
