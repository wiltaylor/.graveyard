import { Show, createResource, createSignal, type JSX } from "solid-js";
import AddInstanceForm from "./components/AddInstanceForm";
import InstanceDetail from "./components/InstanceDetail";
import InstanceTree from "./components/InstanceTree";
import { api } from "./api";
import type { AgentListResponse, InstanceSummary } from "./types";

export default function App(): JSX.Element {
  const [instances, { refetch: refetchInstances }] = createResource<
    InstanceSummary[]
  >(() => api.listInstances(), { initialValue: [] });

  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [agentsByInstance, setAgentsByInstance] = createSignal<
    Record<string, AgentListResponse>
  >({});

  const loadAgents = async (id: string) => {
    try {
      const resp = await api.listAgents(id);
      setAgentsByInstance({ ...agentsByInstance(), [id]: resp });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setAgentsByInstance({
        ...agentsByInstance(),
        [id]: {
          agents: [],
          ok: false,
          raw: null,
          error: message,
        },
      });
    }
  };

  const selectedInstance = () =>
    instances()?.find((i) => i.id === selectedId()) ?? null;

  const handleCreated = async (inst: InstanceSummary) => {
    await refetchInstances();
    setSelectedId(inst.id);
  };

  const handleDeleted = async () => {
    const deletedId = selectedId();
    setSelectedId(null);
    if (deletedId) {
      const rest = { ...agentsByInstance() };
      delete rest[deletedId];
      setAgentsByInstance(rest);
    }
    await refetchInstances();
  };

  return (
    <div class="min-h-screen grid grid-cols-[20rem_1fr] grid-rows-[auto_1fr]">
      <header class="col-span-2 border-b border-slate-800 bg-slate-900 px-4 py-3">
        <h1 class="text-lg font-semibold">aciddog manager</h1>
        <p class="text-xs text-slate-400">
          Registered aciddog :webservice instances.
        </p>
      </header>

      <aside class="border-r border-slate-800 bg-slate-900/60 p-4 overflow-y-auto">
        <section class="mb-6">
          <h2 class="text-xs uppercase tracking-wider text-slate-400 mb-2">
            Instances
          </h2>
          <Show
            when={!instances.loading}
            fallback={
              <div class="text-sm text-slate-500">Loading…</div>
            }
          >
            <InstanceTree
              instances={instances() ?? []}
              selectedId={selectedId()}
              onSelect={setSelectedId}
              agentsFor={(id) => agentsByInstance()[id]}
              onExpand={(id) => {
                if (!agentsByInstance()[id]) loadAgents(id);
              }}
            />
          </Show>
        </section>

        <section>
          <h2 class="text-xs uppercase tracking-wider text-slate-400 mb-2">
            Add instance
          </h2>
          <AddInstanceForm onCreated={handleCreated} />
        </section>
      </aside>

      <main class="p-6 overflow-y-auto">
        <Show
          when={selectedInstance()}
          fallback={
            <div class="text-slate-500">
              Select an instance from the left, or add a new one.
            </div>
          }
        >
          {(inst) => (
            <InstanceDetail
              instance={inst()}
              agents={agentsByInstance()[inst().id]}
              onRefreshAgents={() => loadAgents(inst().id)}
              onDeleted={handleDeleted}
            />
          )}
        </Show>
      </main>
    </div>
  );
}
