import { Show, createResource, createSignal, type JSX } from "solid-js";
import { api } from "../api";
import type { AgentListResponse, HealthResponse, InstanceSummary } from "../types";
import {
  Badge,
  Button,
  Card,
  Empty,
  Eyebrow,
  Table,
  Toast,
} from "./ui";

interface Props {
  instance: InstanceSummary;
  agents: AgentListResponse | undefined;
  onRefreshAgents: () => void;
  onDeleted: () => void;
  onSelectAgent: (agentId: string) => void;
}

export default function InstanceDetail(props: Props): JSX.Element {
  const [health, { refetch: refreshHealth }] = createResource<
    HealthResponse,
    string
  >(
    () => props.instance.id,
    (id) => api.health(id),
  );
  const [deleting, setDeleting] = createSignal(false);
  const [deleteError, setDeleteError] = createSignal<string | null>(null);

  const handleDelete = async () => {
    if (!window.confirm(`Delete instance "${props.instance.name}"?`)) return;
    setDeleteError(null);
    setDeleting(true);
    try {
      await api.deleteInstance(props.instance.id);
      props.onDeleted();
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : String(err));
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "20px" }}>
      <header
        style={{
          display: "flex",
          "align-items": "flex-start",
          "justify-content": "space-between",
          gap: "16px",
        }}
      >
        <div>
          <h2
            style={{
              "font-size": "22px",
              "font-weight": "600",
              color: "var(--fg-0)",
              margin: "0",
              "letter-spacing": "-0.01em",
            }}
          >
            {props.instance.name}
          </h2>
          <p
            style={{
              color: "var(--fg-2)",
              "font-size": "13px",
              "font-family": "var(--font-mono)",
              margin: "4px 0 0",
            }}
          >
            {props.instance.transport === "unix"
              ? props.instance.socket_path
              : props.instance.url}
          </p>
        </div>
        <Button
          variant="danger"
          size="sm"
          icon="x"
          disabled={deleting()}
          onClick={handleDelete}
        >
          {deleting() ? "Deleting…" : "Delete"}
        </Button>
      </header>

      <Show when={deleteError()}>
        <Toast tone="danger">{deleteError()}</Toast>
      </Show>

      <Card
        title="Health"
        action={
          <Button
            variant="ghost"
            size="sm"
            icon="refresh"
            onClick={() => refreshHealth()}
          >
            Refresh
          </Button>
        }
      >
        <Show
          when={!health.loading}
          fallback={
            <span style={{ "font-size": "13px", color: "var(--fg-2)" }}>
              probing…
            </span>
          }
        >
          <Show
            when={health()}
            fallback={
              <Toast tone="danger">
                {health.error instanceof Error
                  ? health.error.message
                  : "unknown error"}
              </Toast>
            }
          >
            {(h) => (
              <Toast tone={h().ok ? "success" : "warning"}>
                HTTP {h().status} — {h().body || "(empty body)"}
              </Toast>
            )}
          </Show>
        </Show>
      </Card>

      <Card
        title="Agents"
        action={
          <Button
            variant="ghost"
            size="sm"
            icon="refresh"
            onClick={() => props.onRefreshAgents()}
          >
            Refresh
          </Button>
        }
        padded={false}
      >
        <Show
          when={props.agents}
          fallback={
            <div style={{ padding: "20px" }}>
              <Empty>Expand this instance in the tree to load agents.</Empty>
            </div>
          }
        >
          {(resp) => (
            <Show
              when={resp().ok}
              fallback={
                <div style={{ padding: "16px" }}>
                  <Toast tone="warning">{resp().error ?? "No agent data."}</Toast>
                </div>
              }
            >
              <Show
                when={resp().agents.length > 0}
                fallback={
                  <div style={{ padding: "20px" }}>
                    <Empty>No agents reported.</Empty>
                  </div>
                }
              >
                <Table
                  // Two-key stable sort: bucket by `group` in
                  // first-appearance order (named groups first,
                  // ungrouped last to mirror the TUI sidebar's
                  // `(ungrouped)` placement), then within each bucket
                  // pin leader=true rows first. Older daemons that
                  // emit the legacy 5/6-column table have group=null
                  // on every row — the bucketing collapses to a single
                  // (ungrouped) section and the leader sort still
                  // applies, so the layout degrades gracefully.
                  rows={(() => {
                    const order: (string | null)[] = [];
                    for (const a of resp().agents) {
                      const g = a.group ?? null;
                      if (!order.includes(g)) order.push(g);
                    }
                    if (!order.includes(null)) order.push(null);
                    const indexOfBucket = (g: string | null) => {
                      const i = order.indexOf(g);
                      return i === -1 ? order.length : i;
                    };
                    return [...resp().agents].sort((a, b) => {
                      const ag = a.group ?? null;
                      const bg = b.group ?? null;
                      const ai = indexOfBucket(ag);
                      const bi = indexOfBucket(bg);
                      if (ai !== bi) return ai - bi;
                      const aIsLeader = a.leader && a.parent === null;
                      const bIsLeader = b.leader && b.parent === null;
                      if (aIsLeader === bIsLeader) return 0;
                      return aIsLeader ? -1 : 1;
                    });
                  })()}
                  rowKey={(a) => a.id}
                  onRowClick={(a) => props.onSelectAgent(a.id)}
                  columns={[
                    {
                      key: "id",
                      header: <Eyebrow>ID</Eyebrow>,
                      render: (a) => (
                        <span style={{ color: "var(--accent-fg)", "font-weight": "500" }}>
                          <Show when={a.leader && a.parent === null}>
                            <span
                              title="leader"
                              aria-label="leader"
                              style={{ "margin-right": "0.35em" }}
                            >
                              👑
                            </span>
                          </Show>
                          {a.id}
                        </span>
                      ),
                    },
                    {
                      key: "group",
                      header: <Eyebrow>Group</Eyebrow>,
                      render: (a) => (
                        <span
                          style={{
                            color: a.group ? "var(--fg-1)" : "var(--fg-2)",
                            "font-style": a.group ? "normal" : "italic",
                          }}
                        >
                          {a.group ?? "(ungrouped)"}
                        </span>
                      ),
                    },
                    {
                      key: "kind",
                      header: <Eyebrow>Kind</Eyebrow>,
                      render: (a) => <Badge tone="neutral">{a.kind}</Badge>,
                    },
                    {
                      key: "status",
                      header: <Eyebrow>Status</Eyebrow>,
                      render: (a) => {
                        const tone =
                          a.status === "running"
                            ? "success"
                            : a.status === "failed"
                              ? "danger"
                              : "neutral";
                        return (
                          <Badge tone={tone} dot>
                            {a.status}
                          </Badge>
                        );
                      },
                    },
                    {
                      key: "parent",
                      header: <Eyebrow>Parent</Eyebrow>,
                      render: (a) => (
                        <span style={{ color: "var(--fg-2)" }}>
                          {a.parent ?? "—"}
                        </span>
                      ),
                    },
                    {
                      key: "ai_core",
                      header: <Eyebrow>AI core</Eyebrow>,
                      mono: true,
                      render: (a) => a.ai_core ?? "—",
                    },
                  ]}
                />
              </Show>
            </Show>
          )}
        </Show>
      </Card>
    </div>
  );
}
