import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";

import { api, channelEventsUrl } from "../api";
import type {
  ChannelMessageEvent,
  WireChannelInfo,
  WireChannelMessage,
} from "../types";

interface Props {
  instanceId: string;
  channel: string;
  /// Metadata from the sidebar's cached channel list — used for the
  /// header (description + member chips). Optional so the view still
  /// works if the parent hasn't loaded the list yet.
  info?: WireChannelInfo;
  onClose: () => void;
}

type StreamStatus = "connecting" | "live" | "lagged" | "closed";

export default function ChannelView(props: Props): JSX.Element {
  const [messages, setMessages] = createSignal<WireChannelMessage[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [status, setStatus] = createSignal<StreamStatus>("connecting");
  const [droppedCount, setDroppedCount] = createSignal(0);
  const [input, setInput] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [sendError, setSendError] = createSignal<string | null>(null);

  const loadHistory = async () => {
    setError(null);
    try {
      const resp = await api.channelHistory(props.instanceId, props.channel);
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

  createEffect(() => {
    const id = props.instanceId;
    const ch = props.channel;
    setMessages([]);
    setDroppedCount(0);
    setStatus("connecting");
    void loadHistory();

    const es = new EventSource(channelEventsUrl(id, ch));
    es.onopen = () => setStatus("live");
    es.onmessage = (ev) => {
      try {
        const parsed = JSON.parse(ev.data) as ChannelMessageEvent;
        setMessages((prev) => [
          ...prev,
          { sender: parsed.sender, body: parsed.body, is_welcome: false },
        ]);
      } catch {
        // Ignore malformed frames.
      }
    };
    es.addEventListener("lagged", (ev) => {
      setStatus("lagged");
      try {
        const parsed = JSON.parse((ev as MessageEvent).data) as {
          dropped?: number;
        };
        if (typeof parsed.dropped === "number") {
          setDroppedCount((n) => n + parsed.dropped!);
        }
      } catch {
        // Ignore payload parse errors.
      }
      void loadHistory();
    });
    es.onerror = () => setStatus("closed");
    onCleanup(() => es.close());
  });

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    const body = input().trim();
    if (!body) return;
    setSending(true);
    setSendError(null);
    try {
      const resp = await api.sendChannelMessage(
        props.instanceId,
        props.channel,
        body,
      );
      if (!resp.ok) {
        setSendError(resp.error ?? "send failed");
      } else {
        setInput("");
      }
    } catch (err) {
      setSendError(err instanceof Error ? err.message : String(err));
    } finally {
      setSending(false);
    }
  };

  return (
    <div class="flex flex-col h-[calc(100vh-6rem)]">
      <header class="border-b border-slate-800 px-4 py-2 bg-slate-900/60 space-y-1">
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-center gap-2 min-w-0">
            <button
              type="button"
              onClick={props.onClose}
              class="text-sm text-slate-400 hover:text-slate-100"
              title="Back to instance detail"
            >
              ← back
            </button>
            <h2 class="text-lg font-semibold truncate">#{props.channel}</h2>
            <StreamStatusBadge
              status={status()}
              dropped={droppedCount()}
            />
          </div>
          <button
            type="button"
            onClick={loadHistory}
            class="text-xs text-slate-400 hover:text-slate-100"
          >
            refresh
          </button>
        </div>
        <Show when={props.info?.description}>
          {(desc) => (
            <p class="text-xs text-slate-400">{desc()}</p>
          )}
        </Show>
        <Show when={props.info && props.info.members.length > 0}>
          <div class="flex items-center flex-wrap gap-1 text-xs">
            <span class="text-slate-500 uppercase tracking-wider">
              Members ({props.info!.members.length})
            </span>
            <For each={props.info!.members}>
              {(m) => (
                <span class="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-slate-300">
                  {m}
                </span>
              )}
            </For>
          </div>
        </Show>
      </header>

      <Show when={error()}>
        <div class="mx-4 mt-2 rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <div class="flex-1 overflow-y-auto px-4 py-3 space-y-2">
        <Show
          when={messages().length > 0}
          fallback={
            <div class="text-sm text-slate-500 italic">
              {error() ? "channel unavailable." : "(no messages yet)"}
            </div>
          }
        >
          <For each={messages()}>{(m) => <ChannelMessageRow msg={m} />}</For>
        </Show>
      </div>

      <form
        onSubmit={submit}
        class="border-t border-slate-800 p-3 bg-slate-900/60 space-y-2"
      >
        <Show when={sendError()}>
          <div class="rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
            {sendError()}
          </div>
        </Show>
        <div class="flex gap-2">
          <textarea
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                (e.currentTarget.form as HTMLFormElement).requestSubmit();
              }
            }}
            placeholder={`Post to #${props.channel} as the human operator. Enter to send, Shift+Enter for newline.`}
            rows={2}
            class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm resize-y"
          />
          <button
            type="submit"
            disabled={sending() || input().trim() === ""}
            class="self-end rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
          >
            {sending() ? "Sending…" : "Send"}
          </button>
        </div>
      </form>
    </div>
  );
}

function ChannelMessageRow(props: {
  msg: WireChannelMessage;
}): JSX.Element {
  return (
    <Show
      when={!props.msg.is_welcome}
      fallback={
        <div class="rounded border border-slate-800 bg-slate-900/30 px-2 py-1 text-xs text-slate-400 italic">
          {props.msg.body}
        </div>
      }
    >
      <div
        class="rounded border px-2 py-1 text-sm"
        classList={{
          "border-emerald-800 bg-emerald-950/30": props.msg.sender === "user",
          "border-indigo-800 bg-indigo-950/30": props.msg.sender !== "user",
        }}
      >
        <div class="text-xs font-mono text-slate-400 mb-0.5">
          {props.msg.sender}
        </div>
        <div class="whitespace-pre-wrap break-words">{props.msg.body}</div>
      </div>
    </Show>
  );
}

function StreamStatusBadge(props: {
  status: StreamStatus;
  dropped: number;
}): JSX.Element {
  return (
    <span
      class="text-xs rounded px-1.5 py-0.5 border"
      classList={{
        "text-slate-400 border-slate-700": props.status === "connecting",
        "text-emerald-300 border-emerald-800 bg-emerald-900/30":
          props.status === "live",
        "text-amber-300 border-amber-800 bg-amber-900/30":
          props.status === "lagged",
        "text-red-300 border-red-800 bg-red-900/30":
          props.status === "closed",
      }}
      title={
        props.status === "lagged" && props.dropped > 0
          ? `dropped ${props.dropped} event(s); history re-fetched`
          : undefined
      }
    >
      {props.status === "lagged" && props.dropped > 0
        ? `lagged · ${props.dropped} dropped`
        : props.status}
    </span>
  );
}
