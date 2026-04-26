import {
  Show,
  createEffect,
  createResource,
  createSignal,
  type JSX,
} from "solid-js";
import AddInstanceForm from "./components/AddInstanceForm";
import ChannelView from "./components/ChannelView";
import ChatPane from "./components/ChatPane";
import CronView from "./components/CronView";
import InboxView from "./components/InboxView";
import InstanceDetail from "./components/InstanceDetail";
import InstanceTree from "./components/InstanceTree";
import MemoryView from "./components/MemoryView";
import SettingsTree, {
  type SettingsNodeId,
} from "./components/SettingsTree";
import TasksView from "./components/TasksView";
import TeamView from "./components/TeamView";
import UsersView from "./components/UsersView";
import WorkflowsView from "./components/WorkflowsView";
import { Button, Empty, Tabs, ToastHost } from "./components/ui";
import type { TabItem } from "./components/ui";
import { api } from "./api";
import { authApi } from "./auth";
import type {
  AgentListResponse,
  ChannelListResponse,
  InstanceSummary,
} from "./types";

interface Props {
  username: string;
  onLoggedOut: () => void;
}

type TabId =
  | "team"
  | "agents"
  | "tasks"
  | "workflows"
  | "cron"
  | "inbox"
  | "memory";

const TABS: TabItem<TabId>[] = [
  { id: "team", label: "Team", icon: "users" },
  { id: "agents", label: "Agents", icon: "user" },
  { id: "tasks", label: "Tasks", icon: "check" },
  { id: "workflows", label: "Workflows", icon: "git-branch" },
  { id: "cron", label: "Cron", icon: "clock" },
  { id: "inbox", label: "Inbox", icon: "bell" },
  { id: "memory", label: "Memory", icon: "database" },
];

export default function AppShell(props: Props): JSX.Element {
  const [instances, { refetch: refetchInstances }] = createResource<
    InstanceSummary[]
  >(() => api.listInstances(), { initialValue: [] });

  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [selectedAgent, setSelectedAgent] = createSignal<string | null>(null);
  const [selectedChannel, setSelectedChannel] = createSignal<string | null>(
    null,
  );
  const [settingsSelection, setSettingsSelection] =
    createSignal<SettingsNodeId | null>(null);
  const [activeTab, setActiveTab] = createSignal<TabId>("team");

  const selectInstance = (id: string | null) => {
    if (id !== null) setSettingsSelection(null);
    setSelectedId(id);
  };

  const selectSettings = (id: SettingsNodeId) => {
    setSelectedId(null);
    setSelectedAgent(null);
    setSelectedChannel(null);
    setSettingsSelection(id);
  };
  const [agentsByInstance, setAgentsByInstance] = createSignal<
    Record<string, AgentListResponse>
  >({});
  const [channelsByInstance, setChannelsByInstance] = createSignal<
    Record<string, ChannelListResponse>
  >({});

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

  const handleLogout = async () => {
    try {
      await authApi.logout();
    } catch {
      // best-effort
    }
    props.onLoggedOut();
  };

  return (
    <div
      class="app-shell"
      style={{
        "grid-template-columns": "280px 1fr",
        "grid-template-rows": "48px 1fr",
      }}
    >
      <header class="ftopbar">
        <div class="ftopbar-brand">
          <img src="/logo-mark.svg" alt="" style={{ height: "20px" }} />
          <span style={{ "font-weight": "600", "font-size": "14px" }}>
            aciddog manager
          </span>
        </div>
        <div class="ftopbar-crumbs">
          <Show when={selectedInstance()}>
            {(inst) => (
              <>
                <span class="sep">/</span>
                <span>{inst().name}</span>
              </>
            )}
          </Show>
          <Show when={settingsSelection()}>
            <span class="sep">/</span>
            <span>Settings · {settingsSelection()}</span>
          </Show>
        </div>
        <div
          style={{
            "margin-left": "auto",
            display: "flex",
            "align-items": "center",
            gap: "12px",
          }}
        >
          <span style={{ "font-size": "12px", color: "var(--fg-2)" }}>
            Signed in as{" "}
            <span style={{ color: "var(--fg-0)" }}>{props.username}</span>
          </span>
          <Button
            size="sm"
            variant="secondary"
            icon="log-out"
            onClick={handleLogout}
          >
            Log out
          </Button>
        </div>
      </header>

      <aside class="fsidebar" style={{ overflow: "auto" }}>
        <div class="fsidebar-section">Instances</div>
        <Show
          when={!instances.loading}
          fallback={
            <div
              style={{
                padding: "8px 10px",
                "font-size": "12px",
                color: "var(--fg-2)",
              }}
            >
              Loading…
            </div>
          }
        >
          <InstanceTree
            instances={instances() ?? []}
            selectedId={selectedId()}
            selectedAgent={selectedAgent()}
            selectedChannel={selectedChannel()}
            onSelect={selectInstance}
            onSelectAgent={(instId, agentId) => {
              selectInstance(instId);
              setSelectedAgent(agentId);
              setSelectedChannel(null);
              setActiveTab("agents");
            }}
            onSelectChannel={(instId, channelId) => {
              selectInstance(instId);
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

        <div class="fsidebar-section">Settings</div>
        <SettingsTree
          selected={settingsSelection()}
          onSelect={selectSettings}
        />

        <div class="fsidebar-section">Add instance</div>
        <div style={{ padding: "0 8px" }}>
          <AddInstanceForm onCreated={handleCreated} />
        </div>
      </aside>

      <main
        class="app-main"
        style={{
          padding: "0",
          display: "flex",
          "flex-direction": "column",
          "min-height": "0",
          overflow: "hidden",
        }}
      >
        <Show when={settingsSelection() === "users"}>
          <div style={{ flex: "1", overflow: "auto", padding: "20px 24px" }}>
            <UsersView currentUsername={props.username} />
          </div>
        </Show>
        <Show
          when={!settingsSelection() && selectedInstance()}
          fallback={
            <Show when={!settingsSelection()}>
              <div style={{ padding: "32px" }}>
                <Empty title="No instance selected">
                  Select an instance from the left, or add a new one.
                </Empty>
              </div>
            </Show>
          }
        >
          {(inst) => (
            <>
              <div
                style={{
                  "border-bottom": "1px solid var(--border-subtle)",
                  background: "var(--bg-0)",
                }}
              >
                <Tabs
                  items={TABS}
                  active={activeTab()}
                  onChange={(id) => {
                    setActiveTab(id);
                    if (id !== "agents") {
                      setSelectedAgent(null);
                      setSelectedChannel(null);
                    }
                  }}
                />
              </div>

              <div
                style={{
                  flex: "1",
                  overflow: "hidden",
                  "min-height": "0",
                  display: "flex",
                  "flex-direction": "column",
                }}
              >
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
                          <div
                            style={{
                              padding: "20px 24px",
                              overflow: "auto",
                              flex: "1",
                            }}
                          >
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

                <Show when={activeTab() === "cron"}>
                  <CronView
                    instanceId={inst().id}
                    agents={agentsByInstance()[inst().id]?.agents ?? []}
                  />
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
      <ToastHost />
    </div>
  );
}
