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
import ReposView from "./components/ReposView";
import SettingsTree, {
  type SettingsNodeId,
} from "./components/SettingsTree";
import SkillsView from "./components/skills/SkillsView";
import TasksView from "./components/TasksView";
import TeamView from "./components/TeamView";
import UsersView from "./components/UsersView";
import WorkflowsView from "./components/WorkflowsView";
import {
  Button,
  Drawer,
  Empty,
  Icon,
  Modal,
  Spinner,
  Tabs,
  ToastHost,
} from "./components/ui";
import Menu from "./components/ui/Menu";
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
  | "repos"
  | "cron"
  | "inbox"
  | "memory";

const TABS: TabItem<TabId>[] = [
  { id: "team", label: "Team", icon: "users" },
  { id: "agents", label: "Agents", icon: "user" },
  { id: "tasks", label: "Tasks", icon: "check" },
  { id: "workflows", label: "Workflows", icon: "git-branch" },
  { id: "repos", label: "Repos", icon: "git-branch" },
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
  const [showAddInstance, setShowAddInstance] = createSignal(false);
  const [drawerOpen, setDrawerOpen] = createSignal(false);
  let triggerRef: HTMLButtonElement | undefined;
  const closeDrawer = () => setDrawerOpen(false);

  const selectInstance = (id: string | null) => {
    if (id !== null) setSettingsSelection(null);
    setSelectedId(id);
    closeDrawer();
  };

  const selectSettings = (id: SettingsNodeId) => {
    setSelectedId(null);
    setSelectedAgent(null);
    setSelectedChannel(null);
    setSettingsSelection(id);
    closeDrawer();
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
    setShowAddInstance(false);
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
    <div class="app-shell">
      {/* Skip-link must remain OUTSIDE T2 drawer portal so Tab from <body> hits it before any focus trap */}
      <a class="skip-link" href="#app-main-content">
        Skip to main content
      </a>
      <header class="ftopbar">
        <button
          ref={(el) => (triggerRef = el)}
          type="button"
          class="ftopbar-hamburger"
          aria-controls="app-sidebar"
          aria-expanded={drawerOpen()}
          aria-label={drawerOpen() ? "Close navigation" : "Open navigation"}
          onClick={() => setDrawerOpen((v) => !v)}
        >
          <Icon name="menu" size={18} />
        </button>
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
        <div class="ftopbar-account">
          <span class="ftopbar-account-label">
            Signed in as{" "}
            <span style={{ color: "var(--fg-0)" }}>{props.username}</span>
          </span>
          <Menu
            align="end"
            sections={[
              {
                title: `Signed in as ${props.username}`,
                items: [
                  {
                    id: "logout",
                    label: "Log out",
                    icon: "log-out",
                    danger: true,
                    onSelect: handleLogout,
                  },
                ],
              },
            ]}
            trigger={(api) => (
              <button
                type="button"
                class="ftopbar-avatar-btn"
                aria-label={`Account menu — ${props.username}`}
                aria-haspopup="menu"
                aria-expanded={api.isOpen()}
                onClick={api.toggle}
              >
                {(props.username?.[0] ?? "").toUpperCase() || (
                  <Icon name="user" size={16} />
                )}
              </button>
            )}
          />
        </div>
      </header>

      <Drawer
        open={drawerOpen()}
        onClose={closeDrawer}
        triggerRef={() => triggerRef ?? null}
        labelledBy="app-sidebar-label"
      >
        <aside
          id="app-sidebar"
          classList={{ fsidebar: true, "is-drawer-open": drawerOpen() }}
          style={{ overflow: "auto" }}
        >
          <h2 id="app-sidebar-label" class="sr-only">
            Navigation
          </h2>
          <div
            class="fsidebar-section"
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              padding: "14px 6px 6px 10px",
            }}
          >
            <h2 class="fsidebar-section-title">Instances</h2>
            <button
              type="button"
              onClick={() => setShowAddInstance(true)}
              aria-label="Add instance"
              title="Add instance"
              class="ftopbar-icon-btn"
            >
              <Icon name="plus" size={14} />
            </button>
          </div>
          <Show
            when={!instances.loading}
            fallback={
              <div class="fsidebar-loading">
                <Spinner size={14} label="Loading instances" />
                <span>Loading…</span>
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
                setSettingsSelection(null);
                setSelectedId(instId);
                setSelectedAgent(agentId);
                setSelectedChannel(null);
                setActiveTab("agents");
                closeDrawer();
              }}
              onSelectChannel={(instId, channelId) => {
                setSettingsSelection(null);
                setSelectedId(instId);
                setSelectedChannel(channelId);
                setSelectedAgent(null);
                setActiveTab("agents");
                closeDrawer();
              }}
              agentsFor={(id) => agentsByInstance()[id]}
              channelsFor={(id) => channelsByInstance()[id]}
              onExpand={(id) => {
                if (!agentsByInstance()[id]) loadAgents(id);
                if (!channelsByInstance()[id]) loadChannels(id);
              }}
            />
          </Show>

          <h2 class="fsidebar-section">Settings</h2>
          <SettingsTree
            selected={settingsSelection()}
            onSelect={selectSettings}
          />
        </aside>
      </Drawer>

      <Modal
        open={showAddInstance()}
        onClose={() => setShowAddInstance(false)}
        title="Add instance"
        width={480}
      >
        <AddInstanceForm onCreated={handleCreated} />
      </Modal>

      <main
        id="app-main-content"
        class="app-main"
        tabindex={-1}
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
        <Show when={settingsSelection() === "skills"}>
          <div style={{ flex: "1", overflow: "auto", padding: "20px 24px" }}>
            <SkillsView />
          </div>
        </Show>
        <Show
          when={!settingsSelection() && selectedInstance()}
          fallback={
            <Show when={!settingsSelection()}>
              <div style={{ padding: "32px" }}>
                <Empty
                  title="No instance selected"
                  action={
                    <>
                      <span class="empty-action-mobile">
                        <Button
                          variant="primary"
                          icon="menu"
                          onClick={() => setDrawerOpen(true)}
                        >
                          Show instances
                        </Button>
                      </span>
                      <span class="empty-action-desktop">
                        <Button
                          variant="secondary"
                          icon="plus"
                          onClick={() => setShowAddInstance(true)}
                        >
                          Add instance
                        </Button>
                      </span>
                    </>
                  }
                >
                  Select an instance, or add a new one.
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
                  iconOnlyOnMobile
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
                  <TeamView
                    instanceId={inst().id}
                    instanceName={inst().name}
                    onRemoved={handleDeleted}
                  />
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

                <Show when={activeTab() === "repos"}>
                  <ReposView instanceId={inst().id} />
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
