import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";

import { api, instanceEventsUrl } from "../api";
import type { AgentRow, WireInboxMessage } from "../types";

interface Props {
  instanceId: string;
  /// Full agents list from the sidebar, used to build the Forward
  /// recipient dropdown. Same filter the TUI's `forward_candidates`
  /// applies: agents + automatons, workers excluded.
  agents: AgentRow[];
}

type RowMode =
  | { kind: "collapsed" }
  | { kind: "reply"; body: string }
  | { kind: "forward"; to: string; body: string };

export default function InboxView(props: Props): JSX.Element {
  const [messages, setMessages] = createSignal<WireInboxMessage[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [status, setStatus] = createSignal<
    "connecting" | "live" | "closed"
  >("connecting");

  // One per-row composer state. Keyed by message index so the composer
  // survives list refreshes as long as indices stay stable. Cleared
  // after a successful send/delete.
  const [rowModes, setRowModes] = createSignal<Record<number, RowMode>>({});
  const [busy, setBusy] = createSignal<number | null>(null);

  const loadInbox = async () => {
    setError(null);
    try {
      const resp = await api.listInbox(props.instanceId);
      if (!resp.ok) {
        setError(resp.error ?? "remote returned no data");
        setMessages([]);
      } else {
        setMessages(resp.messages);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  // (Re)load when instance changes and subscribe to the instance-wide
  // SSE stream. Any `user_message` frame triggers a refetch — that's
  // the same event the webservice emits when an agent sends to
  // "user", which is exactly what puts a message in the inbox.
  createEffect(() => {
    const id = props.instanceId;
    setMessages([]);
    setRowModes({});
    setStatus("connecting");
    void loadInbox();

    const es = new EventSource(instanceEventsUrl(id));
    es.onopen = () => setStatus("live");
    es.onmessage = (ev) => {
      try {
        const payload = JSON.parse(ev.data) as { kind?: string };
        if (payload.kind === "user_message") {
          void loadInbox();
        }
      } catch {
        // Ignore malformed frames.
      }
    };
    es.onerror = () => setStatus("closed");
    onCleanup(() => es.close());
  });

  // Agents eligible as forward recipients (TUI parity: agents +
  // automatons, workers excluded).
  const forwardCandidates = () =>
    props.agents.filter(
      (a) => a.kind === "agent" || a.kind === "automaton",
    );

  const openReply = (idx: number) => {
    setRowModes({ ...rowModes(), [idx]: { kind: "reply", body: "" } });
  };

  const openForward = (idx: number) => {
    const first = forwardCandidates()[0]?.id ?? "";
    setRowModes({
      ...rowModes(),
      [idx]: { kind: "forward", to: first, body: "" },
    });
  };

  const closeRow = (idx: number) => {
    const next = { ...rowModes() };
    delete next[idx];
    setRowModes(next);
  };

  const submitReply = async (idx: number) => {
    const mode = rowModes()[idx];
    if (!mode || mode.kind !== "reply") return;
    if (mode.body.trim() === "") return;
    setBusy(idx);
    setError(null);
    try {
      const resp = await api.replyInbox(props.instanceId, idx, mode.body);
      if (!resp.ok) {
        setError(resp.error ?? "reply failed");
      } else {
        closeRow(idx);
        await loadInbox();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  const submitForward = async (idx: number) => {
    const mode = rowModes()[idx];
    if (!mode || mode.kind !== "forward") return;
    if (mode.to === "" || mode.body.trim() === "") return;
    setBusy(idx);
    setError(null);
    try {
      const resp = await api.forwardInbox(
        props.instanceId,
        idx,
        mode.to,
        mode.body,
      );
      if (!resp.ok) {
        setError(resp.error ?? "forward failed");
      } else {
        closeRow(idx);
        await loadInbox();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  const submitDelete = async (idx: number) => {
    setBusy(idx);
    setError(null);
    try {
      const resp = await api.deleteInboxMessage(props.instanceId, idx);
      if (!resp.ok) {
        setError(resp.error ?? "delete failed");
      } else {
        await loadInbox();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div class="h-full overflow-y-auto p-4 space-y-3">
      <header class="flex items-center justify-between">
        <h2 class="text-lg font-semibold">Inbox</h2>
        <div class="flex items-center gap-2">
          <StreamBadge status={status()} />
          <button
            onClick={loadInbox}
            class="text-xs text-slate-400 hover:text-slate-100"
          >
            refresh
          </button>
        </div>
      </header>

      <Show when={error()}>
        <div class="rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <Show
        when={messages().length > 0}
        fallback={
          <div class="text-sm text-slate-500 italic">
            {error() ? "inbox unavailable." : "(no messages)"}
          </div>
        }
      >
        <For each={messages()}>
          {(msg, idx) => (
            <InboxRow
              msg={msg}
              index={idx()}
              mode={rowModes()[idx()]}
              busy={busy() === idx()}
              forwardCandidates={forwardCandidates()}
              onReplyClick={() => openReply(idx())}
              onForwardClick={() => openForward(idx())}
              onDelete={() => submitDelete(idx())}
              onCancel={() => closeRow(idx())}
              onReplyChange={(body) =>
                setRowModes({
                  ...rowModes(),
                  [idx()]: { kind: "reply", body },
                })
              }
              onForwardChange={(patch) =>
                setRowModes({
                  ...rowModes(),
                  [idx()]: {
                    kind: "forward",
                    to:
                      patch.to ??
                      (rowModes()[idx()] as { to: string } | undefined)?.to ??
                      "",
                    body:
                      patch.body ??
                      (rowModes()[idx()] as { body: string } | undefined)?.body ??
                      "",
                  },
                })
              }
              onSubmitReply={() => submitReply(idx())}
              onSubmitForward={() => submitForward(idx())}
            />
          )}
        </For>
      </Show>
    </div>
  );
}

function StreamBadge(props: {
  status: "connecting" | "live" | "closed";
}): JSX.Element {
  return (
    <span
      class="text-xs rounded px-1.5 py-0.5 border"
      classList={{
        "text-slate-400 border-slate-700": props.status === "connecting",
        "text-emerald-300 border-emerald-800 bg-emerald-900/30":
          props.status === "live",
        "text-red-300 border-red-800 bg-red-900/30":
          props.status === "closed",
      }}
    >
      {props.status}
    </span>
  );
}

function relativeTime(unixSecs: number): string {
  const deltaMs = Date.now() - unixSecs * 1000;
  const mins = Math.floor(deltaMs / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

interface RowProps {
  msg: WireInboxMessage;
  index: number;
  mode: RowMode | undefined;
  busy: boolean;
  forwardCandidates: AgentRow[];
  onReplyClick: () => void;
  onForwardClick: () => void;
  onDelete: () => void;
  onCancel: () => void;
  onReplyChange: (body: string) => void;
  onForwardChange: (patch: { to?: string; body?: string }) => void;
  onSubmitReply: () => void;
  onSubmitForward: () => void;
}

function InboxRow(props: RowProps): JSX.Element {
  const mode = () => props.mode ?? { kind: "collapsed" as const };

  return (
    <article class="rounded border border-slate-800 bg-slate-900/60">
      <header class="flex items-center justify-between px-3 py-2 border-b border-slate-800">
        <div>
          <span class="font-medium">{props.msg.from}</span>
          <span class="ml-2 text-xs text-slate-400">
            {relativeTime(props.msg.received_at)}
          </span>
        </div>
        <div class="flex items-center gap-2 text-sm">
          <button
            type="button"
            disabled={props.busy}
            onClick={props.onReplyClick}
            class="text-sky-300 hover:text-sky-200 disabled:opacity-50"
          >
            reply
          </button>
          <button
            type="button"
            disabled={props.busy}
            onClick={props.onForwardClick}
            class="text-sky-300 hover:text-sky-200 disabled:opacity-50"
          >
            forward
          </button>
          <button
            type="button"
            disabled={props.busy}
            onClick={props.onDelete}
            class="text-red-300 hover:text-red-200 disabled:opacity-50"
          >
            delete
          </button>
        </div>
      </header>
      <pre class="whitespace-pre-wrap break-words px-3 py-2 text-sm">
        {props.msg.body}
      </pre>
      <Show when={mode().kind === "reply"}>
        {(() => {
          const body = () => (props.mode as { body: string }).body;
          return (
            <div class="border-t border-slate-800 p-3 space-y-2 bg-slate-950/40">
              <div class="text-xs text-slate-400">
                Replying to <span class="font-mono">{props.msg.from}</span>
              </div>
              <textarea
                rows={3}
                value={body()}
                onInput={(e) => props.onReplyChange(e.currentTarget.value)}
                class="w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
                placeholder="Reply body"
              />
              <div class="flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={props.onCancel}
                  class="text-xs text-slate-400 hover:text-slate-100"
                >
                  cancel
                </button>
                <button
                  type="button"
                  onClick={props.onSubmitReply}
                  disabled={props.busy || body().trim() === ""}
                  class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
                >
                  {props.busy ? "Sending…" : "Send"}
                </button>
              </div>
            </div>
          );
        })()}
      </Show>
      <Show when={mode().kind === "forward"}>
        {(() => {
          const m = () => props.mode as { to: string; body: string };
          return (
            <div class="border-t border-slate-800 p-3 space-y-2 bg-slate-950/40">
              <label class="block text-xs text-slate-400">
                Forward to
                <select
                  value={m().to}
                  onInput={(e) =>
                    props.onForwardChange({ to: e.currentTarget.value })
                  }
                  class="mt-1 block w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
                >
                  <For each={props.forwardCandidates}>
                    {(a) => (
                      <option value={a.id}>
                        {a.id} ({a.kind})
                      </option>
                    )}
                  </For>
                </select>
              </label>
              <div class="text-xs text-slate-500">
                original from <span class="font-mono">{props.msg.from}</span>
              </div>
              <pre class="whitespace-pre-wrap break-words rounded bg-slate-950/60 px-2 py-1 text-xs text-slate-400 max-h-32 overflow-y-auto">
                {props.msg.body}
              </pre>
              <textarea
                rows={3}
                value={m().body}
                onInput={(e) =>
                  props.onForwardChange({ body: e.currentTarget.value })
                }
                class="w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm"
                placeholder="Add a note (the original is attached above; include it or quote inline if you want it delivered)."
              />
              <div class="flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={props.onCancel}
                  class="text-xs text-slate-400 hover:text-slate-100"
                >
                  cancel
                </button>
                <button
                  type="button"
                  onClick={props.onSubmitForward}
                  disabled={
                    props.busy || m().body.trim() === "" || m().to === ""
                  }
                  class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
                >
                  {props.busy ? "Forwarding…" : "Forward"}
                </button>
              </div>
            </div>
          );
        })()}
      </Show>
    </article>
  );
}
