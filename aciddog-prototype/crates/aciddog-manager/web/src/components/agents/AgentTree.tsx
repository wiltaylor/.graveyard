// Renders the Agents card body: a flat <Table> is replaced with a
// nested tree where workers visually live inside their parent agent's
// container (via a left-border + indent). Group buckets get an
// italic header; null/undefined group renders as "(ungrouped)" and
// always sorts last (the helper handles that — see `agentTree.ts`).
//
// All bucketing/filtering/orphan logic lives in `groupAgents`. This
// component is intentionally dumb — it owns only the chevron-collapse
// state (ephemeral, lost on navigation) and the click wiring.

import { For, Show, createSignal, type JSX } from "solid-js";

import type { AgentRow } from "../../types";
import { Badge } from "../ui";
import { agentStatusVisual } from "./agentStatus";
import {
  groupAgents,
  type AgentChild,
  type AgentNode,
  type GroupAgentsOptions,
} from "./agentTree";

interface Props {
  rows: AgentRow[];
  onSelectAgent: (agentId: string) => void;
  /** Forwarded straight into `groupAgents`. False by default mirrors
   *  the plan's hide-hibernated-by-default behaviour. */
  showHibernated?: boolean;
}

// Shared row geometry. Five columns: ID (with chevron / leader / ↳),
// status badge, group, parent, ai_core. Single source of truth so
// agent / worker / orphan rows align cleanly under each other.
const ROW_GRID: JSX.CSSProperties = {
  display: "grid",
  "grid-template-columns": "minmax(220px, 2fr) minmax(140px, 1fr) minmax(120px, 1fr) minmax(140px, 1fr) minmax(120px, 1fr)",
  "column-gap": "16px",
  "align-items": "center",
  padding: "8px 16px",
  "border-bottom": "1px solid var(--bg-3)",
  cursor: "pointer",
};

const CELL_TEXT: JSX.CSSProperties = {
  "font-size": "13px",
  color: "var(--fg-1)",
  "min-width": "0",
  "overflow-wrap": "anywhere",
};

const CELL_MUTED: JSX.CSSProperties = {
  "font-size": "13px",
  color: "var(--fg-2)",
  "min-width": "0",
  "overflow-wrap": "anywhere",
};

function StatusBadge(props: { row: AgentRow }): JSX.Element {
  const v = () => agentStatusVisual(props.row.kind, props.row.status);
  return (
    <span style={{ "min-width": "0" }}>
      <Badge tone={v().tone} dot>
        {v().glyph} {v().label}
      </Badge>
    </span>
  );
}

function WorkerRow(props: {
  child: AgentChild;
  /** The parent agent's row, when known. We render its id+group in
   *  the parent/group columns for visual parity with the prior table.
   *  `null` for orphans (we fall back to the worker's own group). */
  parentRow: AgentRow | null;
  onSelect: (id: string) => void;
}): JSX.Element {
  const row = props.child.row;
  const groupText = () =>
    (props.parentRow?.group ?? row.group) ?? "—";
  const parentText = () => props.parentRow?.id ?? row.parent ?? "—";

  return (
    <div
      role="button"
      tabIndex={0}
      data-ag-worker-id={row.id}
      data-ag-orphan={props.child.orphaned ? "true" : undefined}
      style={ROW_GRID}
      onClick={() => props.onSelect(row.id)}
    >
      <span style={{ ...CELL_TEXT, color: "var(--accent-fg)", "font-weight": "500" }}>
        <span aria-hidden="true" style={{ "margin-right": "0.15em" }}>↳ </span>
        {row.id}
        <span
          style={{ color: "var(--fg-2)", "margin-left": "0.4em" }}
        >
          [worker]
        </span>
        <Show when={props.child.orphaned}>
          <span
            style={{
              color: "var(--fg-2)",
              "font-style": "italic",
              "margin-left": "0.4em",
            }}
          >
            (orphaned)
          </span>
        </Show>
      </span>
      <StatusBadge row={row} />
      <span style={CELL_MUTED}>{groupText()}</span>
      <span style={CELL_MUTED}>{parentText()}</span>
      <span style={{ ...CELL_TEXT, "font-family": "var(--font-mono)" }}>
        {row.ai_core ?? "—"}
      </span>
    </div>
  );
}

function AgentRowView(props: {
  agent: AgentNode;
  expanded: boolean;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
}): JSX.Element {
  const row = props.agent.row;

  return (
    <div class="ag-agent" data-ag-agent-id={row.id}>
      <div
        role="button"
        tabIndex={0}
        data-ag-row="agent"
        style={ROW_GRID}
        onClick={() => props.onSelect(row.id)}
      >
        <span style={{ ...CELL_TEXT, color: "var(--accent-fg)", "font-weight": "500" }}>
          <button
            type="button"
            data-ag-chevron={row.id}
            aria-label={props.expanded ? "collapse" : "expand"}
            onClick={(e) => {
              e.stopPropagation();
              props.onToggle(row.id);
            }}
            style={{
              background: "transparent",
              border: "0",
              color: "var(--fg-2)",
              cursor: "pointer",
              padding: "0 6px 0 0",
              "font-size": "11px",
              "line-height": "1",
            }}
          >
            {props.expanded ? "▼" : "▶"}
          </button>
          <Show when={row.leader && row.parent === null}>
            <span
              data-ag-leader="true"
              title="leader"
              aria-label="leader"
              style={{ "margin-right": "0.35em" }}
            >
              👑
            </span>
          </Show>
          {row.id}
        </span>
        <StatusBadge row={row} />
        <span
          style={{
            ...CELL_TEXT,
            color: row.group ? "var(--fg-1)" : "var(--fg-2)",
            "font-style": row.group ? "normal" : "italic",
          }}
        >
          {row.group ?? "(ungrouped)"}
        </span>
        <span style={CELL_MUTED}>{row.parent ?? "—"}</span>
        <span style={{ ...CELL_TEXT, "font-family": "var(--font-mono)" }}>
          {row.ai_core ?? "—"}
        </span>
      </div>
      <Show when={props.expanded && props.agent.children.length > 0}>
        <div
          data-ag-children={row.id}
          style={{
            "border-left": "1px solid var(--fg-3)",
            "padding-left": "12px",
            "margin-left": "12px",
          }}
        >
          <For each={props.agent.children}>
            {(child) => (
              <WorkerRow
                child={child}
                parentRow={row}
                onSelect={props.onSelect}
              />
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

export default function AgentTree(props: Props): JSX.Element {
  // Ephemeral collapse state — a missing entry means EXPANDED. Lost
  // on navigation per the plan.
  const [expanded, setExpanded] = createSignal(new Map<string, boolean>());
  const isExpanded = (id: string): boolean => {
    const v = expanded().get(id);
    return v === undefined ? true : v;
  };
  const toggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Map(prev);
      next.set(id, !isExpanded(id));
      return next;
    });
  };

  const groups = (): ReturnType<typeof groupAgents> => {
    const opts: GroupAgentsOptions = props.showHibernated
      ? { showHibernated: true }
      : {};
    return groupAgents(props.rows, opts);
  };

  return (
    <div data-ag-tree="true">
      <For each={groups()}>
        {(group) => (
          <section
            data-ag-group={group.name ?? "__null__"}
            style={{ "padding-bottom": "4px" }}
          >
            <div
              class="ag-group-header"
              data-ag-group-header={group.name ?? "__null__"}
              style={{
                padding: "12px 16px 6px",
                "font-style": "italic",
                color: "var(--fg-2)",
                "font-size": "12px",
                "letter-spacing": "0.04em",
              }}
            >
              ── {group.name ?? "ungrouped"} ──
            </div>
            <For each={group.agents}>
              {(agent) => (
                <AgentRowView
                  agent={agent}
                  expanded={isExpanded(agent.row.id)}
                  onToggle={toggle}
                  onSelect={props.onSelectAgent}
                />
              )}
            </For>
            <For each={group.orphans}>
              {(orphan) => (
                <WorkerRow
                  child={orphan}
                  parentRow={null}
                  onSelect={props.onSelectAgent}
                />
              )}
            </For>
          </section>
        )}
      </For>
    </div>
  );
}
