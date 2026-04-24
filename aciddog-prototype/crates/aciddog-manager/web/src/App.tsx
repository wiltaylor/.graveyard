import { For, Show, createEffect, createResource, createSignal, type JSX } from "solid-js";
import AddInstanceForm from "./components/AddInstanceForm";
import ChannelView from "./components/ChannelView";
import ChatPane from "./components/ChatPane";
import InboxView from "./components/InboxView";
import InstanceDetail from "./components/InstanceDetail";
import InstanceTree from "./components/InstanceTree";
import MemoryView from "./components/MemoryView";
import TasksView from "./components/TasksView";
import TeamView from "./components/TeamView";
import WorkflowsView from "./components/WorkflowsView";
import { api } from "./api";
import type {
  AgentListResponse,
  ChannelListResponse,
  InstanceSummary,
} from "./types";

type TabId =
  | "team"
  | "agents"
  | "tasks"
  | "workflows"
  | "inbox"
  | "memory";

const TABS: { id: TabId; label: string }[] = [
  { id: "team", label: "Team" },
  { id: "agents", label: "Agents" },
  { id: "tasks", label: "Tasks" },
  { id: "workflows", label: "Workflows" },
  { id: "inbox", label: "Inbox" },
  { id: "memory", label: "Memory" },
];

export default function App(): JSX.Element {
  const [instances, { refetch: refetchInstances }] = createResource<
    InstanceSummary[]
  >(() => api.listInstances(), { initialValue: [] });

  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [selectedAgent, setSelectedAgent] = createSignal<string | null>(null);
  const [selectedChannel, setSelectedChannel] = createSignal<string | null>(
    null,
  );
  const [activeTab, setActiveTab] = createSignal<TabId>("team");
  const [agentsByInstance, setAgentsByInstance] = createSignal<
    Record<string, AgentListResponse>
  >({});
  const [channelsByInstance, setChannelsByInstance] = createSignal<
    Record<string, ChannelListResponse>
  >({});

  // Changing instances resets the agent/channel selection and drops
  // the user back on the Team overview tab.
  createEffect(() => {
    selectedId();
    setSelectedAgent(null);
    setSelectedChannel(null);
    setActiveTab("team");
  });

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

  const loadChannels = async (id: string) => {
    try {
      const resp = await api.listChannels(id);
      setChannelsByInstance({ ...channelsByInstance(), [id]: resp });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setChannelsByInstance({
        ...channelsByInstance(),
        [id]: {
          channels: [],
          ok: false,
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
      const restCh = { ...channelsByInstance() };
      delete restCh[deletedId];
      setChannelsByInstance(restCh);
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
              selectedAgent={selectedAgent()}
              selectedChannel={selectedChannel()}
              onSelect={setSelectedId}
              onSelectAgent={(instId, agentId) => {
                setSelectedId(instId);
                setSelectedAgent(agentId);
                setSelectedChannel(null);
                setActiveTab("agents");
              }}
              onSelectChannel={(instId, channelId) => {
                setSelectedId(instId);
                setSelectedChannel(channelId);
                setSelectedAgent(null);
                setActiveTab("agents");
              }}
              agentsFor={(id) => agentsByInstance()[id]}
              channelsFor={(id) => channelsByInstance()[id]}
              onExpand={(id) => {
                if (!agentsByInstance()[id]) loadAgents(id);
                if (!channelsByInstance()[id]) loadChannels(id);
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

      <main class="overflow-hidden flex flex-col">
        <Show
          when={selectedInstance()}
          fallback={
            <div class="p-6 text-slate-500">
              Select an instance from the left, or add a new one.
            </div>
          }
        >
          {(inst) => (
            <>
              <nav class="flex items-center gap-1 border-b border-slate-800 bg-slate-900/40 px-4 pt-2">
                <For each={TABS}>
                  {(tab) => (
                    <button
                      type="button"
                      onClick={() => {
                        setActiveTab(tab.id);
                        if (tab.id !== "agents") {
                          setSelectedAgent(null);
                          setSelectedChannel(null);
                        }
                      }}
                      class="px-3 py-1.5 text-sm border-b-2 -mb-px"
                      classList={{
                        "border-sky-500 text-slate-100":
                          activeTab() === tab.id,
                        "border-transparent text-slate-400 hover:text-slate-200":
                          activeTab() !== tab.id,
                      }}
                    >
                      {tab.label}
                    </button>
                  )}
                </For>
              </nav>

              <div class="flex-1 overflow-hidden">
                <Show when={activeTab() === "team"}>
                  <TeamView instanceId={inst().id} />
                </Show>

                <Show when={activeTab() === "agents"}>
                  <Show
                    when={selectedChannel()}
                    fallback={
                      <Show
                        when={selectedAgent()}
                        fallback={
                          <div class="p-6 overflow-y-auto h-full">
                            <InstanceDetail
                              instance={inst()}
                              agents={agentsByInstance()[inst().id]}
                              onRefreshAgents={() => loadAgents(inst().id)}
                              onDeleted={handleDeleted}
                              onSelectAgent={(agentId) =>
                                setSelectedAgent(agentId)
                              }
                            />
                          </div>
                        }
                      >
                        {(agent) => (
                          <ChatPane
                            instanceId={inst().id}
                            agent={agent()}
                            onClose={() => setSelectedAgent(null)}
                          />
                        )}
                      </Show>
                    }
                  >
                    {(channel) => (
                      <ChannelView
                        instanceId={inst().id}
                        channel={channel()}
                        info={channelsByInstance()[inst().id]?.channels.find(
                          (c) => c.id === channel(),
                        )}
                        onClose={() => setSelectedChannel(null)}
                      />
                    )}
                  </Show>
                </Show>

                <Show when={activeTab() === "tasks"}>
                  <TasksView
                    instanceId={inst().id}
                    agents={agentsByInstance()[inst().id]?.agents ?? []}
                  />
                </Show>

                <Show when={activeTab() === "workflows"}>
                  <WorkflowsView instanceId={inst().id} />
                </Show>

                <Show when={activeTab() === "inbox"}>
                  <InboxView
                    instanceId={inst().id}
                    agents={agentsByInstance()[inst().id]?.agents ?? []}
                  />
                </Show>

                <Show when={activeTab() === "memory"}>
                  <MemoryView instanceId={inst().id} />
                </Show>
              </div>
            </>
          )}
        </Show>
      </main>
    </div>
  );
}
