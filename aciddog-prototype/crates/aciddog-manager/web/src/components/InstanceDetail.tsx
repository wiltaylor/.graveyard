import { Show, createResource, createSignal, type JSX } from "solid-js";
import { api } from "../api";
import type {
  AgentListResponse,
  HealthResponse,
  InstanceSummary,
} from "../types";
import { Button, Card, Empty, Toast } from "./ui";
import AgentTree from "./agents/AgentTree";

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
        scrollX
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
                {/*
                  All bucketing / leader-pin / orphan / hide-hibernated
                  logic lives in `groupAgents` (see
                  `agents/agentTree.ts`); the rendering side is a dumb
                  walk over the resulting tree (see
                  `agents/AgentTree.tsx`). Hibernated workers are
                  hidden by default — the daemon will still ship them
                  in the response, the renderer just drops them so the
                  card stays scannable.
                */}
                <AgentTree
                  rows={resp().agents}
                  onSelectAgent={(id) => props.onSelectAgent(id)}
                />
              </Show>
            </Show>
          )}
        </Show>
      </Card>
    </div>
  );
}
