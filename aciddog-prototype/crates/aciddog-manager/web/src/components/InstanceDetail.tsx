import { Show, createResource, createSignal, type JSX } from "solid-js";
import { api } from "../api";
import type { AgentListResponse, HealthResponse, InstanceSummary } from "../types";

interface Props {
  instance: InstanceSummary;
  agents: AgentListResponse | undefined;
  onRefreshAgents: () => void;
  onDeleted: () => void;
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
    <div class="space-y-6">
      <header class="flex items-start justify-between gap-4">
        <div>
          <h2 class="text-xl font-semibold">{props.instance.name}</h2>
          <p class="text-sm text-slate-400">
            {props.instance.transport === "unix"
              ? props.instance.socket_path
              : props.instance.url}
          </p>
        </div>
        <button
          onClick={handleDelete}
          disabled={deleting()}
          class="rounded border border-red-800 bg-red-900/40 hover:bg-red-800 disabled:opacity-50 px-3 py-1 text-sm"
        >
          {deleting() ? "Deleting…" : "Delete"}
        </button>
      </header>

      <Show when={deleteError()}>
        <div class="rounded bg-red-900/40 border border-red-800 px-2 py-1 text-sm text-red-200">
          {deleteError()}
        </div>
      </Show>

      <section>
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm uppercase tracking-wider text-slate-400">
            Health
          </h3>
          <button
            onClick={() => refreshHealth()}
            class="text-xs text-slate-400 hover:text-slate-100"
          >
            refresh
          </button>
        </div>
        <Show
          when={!health.loading}
          fallback={<div class="text-sm text-slate-500">probing…</div>}
        >
          <Show
            when={health()}
            fallback={
              <div class="text-sm text-red-300">
                {health.error instanceof Error
                  ? health.error.message
                  : "unknown error"}
              </div>
            }
          >
            {(h) => (
              <div
                class="rounded px-2 py-1 text-sm"
                classList={{
                  "bg-emerald-900/40 text-emerald-200 border border-emerald-800":
                    h().ok,
                  "bg-amber-900/40 text-amber-200 border border-amber-800":
                    !h().ok,
                }}
              >
                HTTP {h().status} — {h().body || "(empty body)"}
              </div>
            )}
          </Show>
        </Show>
      </section>

      <section>
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-sm uppercase tracking-wider text-slate-400">
            Agents
          </h3>
          <button
            onClick={() => props.onRefreshAgents()}
            class="text-xs text-slate-400 hover:text-slate-100"
          >
            refresh
          </button>
        </div>
        <Show
          when={props.agents}
          fallback={
            <div class="text-sm text-slate-500">
              Expand this instance in the tree to load agents.
            </div>
          }
        >
          {(resp) => (
            <Show
              when={resp().ok}
              fallback={
                <div class="rounded bg-amber-900/40 border border-amber-800 px-2 py-1 text-sm text-amber-200">
                  {resp().error ?? "No agent data."}
                </div>
              }
            >
              <Show
                when={resp().agents.length > 0}
                fallback={
                  <div class="text-sm text-slate-500">No agents reported.</div>
                }
              >
                <table class="w-full text-sm">
                  <thead class="text-left text-slate-400 uppercase text-xs tracking-wider">
                    <tr>
                      <th class="py-1 pr-4">ID</th>
                      <th class="py-1 pr-4">Kind</th>
                      <th class="py-1 pr-4">Status</th>
                      <th class="py-1 pr-4">Parent</th>
                      <th class="py-1">AI core</th>
                    </tr>
                  </thead>
                  <tbody>
                    {resp().agents.map((a) => (
                      <tr class="border-t border-slate-800">
                        <td class="py-1 pr-4 font-medium">{a.id}</td>
                        <td class="py-1 pr-4">{a.kind}</td>
                        <td class="py-1 pr-4">{a.status}</td>
                        <td class="py-1 pr-4 text-slate-400">
                          {a.parent ?? "—"}
                        </td>
                        <td class="py-1 text-slate-400">{a.ai_core ?? "—"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Show>
            </Show>
          )}
        </Show>
      </section>
    </div>
  );
}
