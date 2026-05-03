import {
  Show,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";

import { api } from "../../api";
import type { ManagedDaemonView } from "../../types";
import { Button, Empty, Toast } from "../ui";
import DaemonDetailPane from "./DaemonDetailPane";
import DaemonsList from "./DaemonsList";
import ScaffoldWizard from "./ScaffoldWizard";

interface Props {
  /// Callback fired when a row's "Open in chat" button is clicked.
  /// AppShell wires this to setSelectedId(instanceId) + setActiveTab('agents')
  /// so we don't introduce any new Router code (decisions/2a5f6646).
  onOpenInChat: (instanceId: string) => void;
}

/// Top-level Daemons pane. Owns:
///   - the createResource for `api.listManagedDaemons` (5s polling)
///   - the ScaffoldWizard open/close signal
///   - the right-pane row selection
///   - top-of-pane warning Toast for polling errors (stale data preserved)
///
/// Action button click handlers (Start/Stop/Reload) call api.* and
/// force a refetch on success so the UI converges in <100ms instead
/// of waiting for the next 5s tick. 409s on reload (and any other
/// 4xx/5xx) surface verbatim in the same warning Toast — request<T>
/// already maps the server's error string into `err.message`.
///
/// NOTE: this component is only ever mounted under
/// `<Show when={isDaemonRunnerEnabled && activeTab === 'daemons'}>` in
/// AppShell. When the gate is off the polling timer never starts.
export default function DaemonsView(props: Props): JSX.Element {
  const [daemons, { refetch }] = createResource<ManagedDaemonView[]>(() =>
    api.listManagedDaemons(),
  );
  const [scaffoldOpen, setScaffoldOpen] = createSignal(false);
  const [selectedRowId, setSelectedRowId] = createSignal<string | null>(null);
  const [actionError, setActionError] = createSignal<string | null>(null);

  // 5s polling — see plan §"Polling cadence" / decisions/585d4b15.
  // Polling interval literal: 5000ms (5_000).
  let pollHandle: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    pollHandle = setInterval(() => {
      void refetch();
    }, 5_000);
  });
  onCleanup(() => {
    if (pollHandle !== undefined) clearInterval(pollHandle);
  });

  /// Last-known list. We DO NOT clear this on a polling error — stale
  /// data stays visible while the warning Toast surfaces the failure
  /// reason (per plan: "do NOT clear stale data — last successful
  /// response stays visible").
  const rows = createMemo<ManagedDaemonView[]>(() => daemons() ?? []);

  const pollError = () => {
    const e = daemons.error;
    if (!e) return null;
    return e instanceof Error ? e.message : String(e);
  };

  const selectedDaemon = createMemo<ManagedDaemonView | null>(() => {
    const id = selectedRowId();
    if (!id) return null;
    return rows().find((d) => d.id === id) ?? null;
  });

  const wrap = async (
    fn: (id: string) => Promise<unknown>,
    id: string,
  ): Promise<void> => {
    setActionError(null);
    try {
      await fn(id);
      // Force-refetch immediately so the UI converges before the next 5s tick.
      await refetch();
    } catch (err) {
      // 409 (reload not running) and any other 4xx/5xx land here with
      // the verbatim server error in `err.message`.
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleStart = (id: string) => wrap((x) => api.startManagedDaemon(x), id);
  const handleStop = (id: string) => wrap((x) => api.stopManagedDaemon(x), id);
  const handleReload = (id: string) =>
    wrap((x) => api.reloadManagedDaemon(x), id);

  const handleDelete = async (id: string): Promise<void> => {
    setActionError(null);
    try {
      await api.deleteManagedDaemon(id);
      // Clear selection if we just deleted the selected row.
      if (selectedRowId() === id) setSelectedRowId(null);
      await refetch();
    } catch (err) {
      // Re-throw so DaemonRow's confirm modal can render the inline
      // Toast and stay open.
      throw err instanceof Error ? err : new Error(String(err));
    }
  };

  const handleCreated = async (created: ManagedDaemonView) => {
    await refetch();
    setSelectedRowId(created.id);
  };

  return (
    <div
      class="daemons-view max-md:flex-col"
      data-test="daemons-view"
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        overflow: "hidden",
      }}
    >
      <header
        style={{
          display: "flex",
          "align-items": "center",
          gap: "12px",
          "border-bottom": "1px solid var(--border-subtle)",
          background: "var(--bg-1)",
          padding: "10px 24px",
        }}
      >
        <h2
          style={{
            "font-size": "14px",
            "font-weight": "600",
            color: "var(--fg-0)",
            margin: "0",
          }}
        >
          Daemons
        </h2>
        <span style={{ color: "var(--fg-2)", "font-size": "12px" }}>
          {rows().length} daemon{rows().length === 1 ? "" : "s"}
        </span>
        <div style={{ flex: "1" }} />
        <Button
          size="sm"
          variant="secondary"
          icon="refresh"
          onClick={() => void refetch()}
        >
          Refresh
        </Button>
        <Button
          size="sm"
          variant="primary"
          icon="plus"
          onClick={() => setScaffoldOpen(true)}
        >
          Scaffold daemon
        </Button>
      </header>

      <Show when={pollError()}>
        {(msg) => (
          <div style={{ padding: "12px 24px 0" }}>
            <Toast tone="warning">{msg()}</Toast>
          </div>
        )}
      </Show>

      <Show when={actionError()}>
        {(msg) => (
          <div style={{ padding: "12px 24px 0" }}>
            <Toast tone="warning">{msg()}</Toast>
          </div>
        )}
      </Show>

      <div
        class="daemons-body max-md:flex-col"
        style={{
          flex: "1",
          overflow: "hidden",
          display: "flex",
          gap: "16px",
          padding: "16px 24px",
          "min-height": "0",
        }}
      >
        <div
          style={{
            flex: "2 1 0",
            overflow: "auto",
            "min-width": "0",
          }}
        >
          <Show
            when={rows().length > 0}
            fallback={
              <Empty>
                {daemons.loading
                  ? "Loading daemons…"
                  : "No managed daemons yet. Click Scaffold daemon to create one."}
              </Empty>
            }
          >
            <DaemonsList
              daemons={rows()}
              selectedId={selectedRowId()}
              onSelect={setSelectedRowId}
              onStart={handleStart}
              onStop={handleStop}
              onReload={handleReload}
              onDelete={handleDelete}
              onOpenInChat={props.onOpenInChat}
            />
          </Show>
        </div>

        <div
          style={{
            flex: "1 1 0",
            overflow: "auto",
            "min-width": "0",
          }}
        >
          <DaemonDetailPane
            selectedDaemon={selectedDaemon()}
            onOpenInChat={props.onOpenInChat}
          />
        </div>
      </div>

      <ScaffoldWizard
        open={scaffoldOpen()}
        onClose={() => setScaffoldOpen(false)}
        onCreated={(d) => void handleCreated(d)}
      />
    </div>
  );
}
