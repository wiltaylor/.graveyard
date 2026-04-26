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
import { Badge, Button, Eyebrow, Textarea, Toast } from "./ui";

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
      <header class="border-b border-border-subtle px-6 py-3 bg-bg-1 space-y-2">
        <div class="flex items-center justify-between gap-4">
          <div class="flex items-center gap-3 min-w-0">
            <Button size="sm" variant="ghost" icon="arrow-right" onClick={props.onClose}>
              Back
            </Button>
            <h2 class="text-base font-semibold text-fg-0 truncate">#{props.channel}</h2>
            <StreamStatusBadge
              status={status()}
              dropped={droppedCount()}
            />
          </div>
          <Button size="sm" variant="ghost" icon="refresh" onClick={loadHistory}>
            Refresh
          </Button>
        </div>
        <Show when={props.info?.description}>
          {(desc) => <p class="text-xs text-fg-2 m-0">{desc()}</p>}
        </Show>
        <Show when={props.info && props.info.members.length > 0}>
          <div class="flex items-center flex-wrap gap-1 text-xs">
            <Eyebrow>Members ({props.info!.members.length})</Eyebrow>
            <For each={props.info!.members}>
              {(m) => <Badge tone="neutral">{m}</Badge>}
            </For>
          </div>
        </Show>
      </header>

      <Show when={error()}>
        <div class="mx-6 mt-3">
          <Toast tone="danger">{error()}</Toast>
        </div>
      </Show>

      <div class="flex-1 overflow-y-auto px-4 py-3 space-y-2">
        <Show
          when={messages().length > 0}
          fallback={
            <div class="text-sm text-fg-3 italic">
              {error() ? "channel unavailable." : "(no messages yet)"}
            </div>
          }
        >
          <For each={messages()}>{(m) => <ChannelMessageRow msg={m} />}</For>
        </Show>
      </div>

      <form
        onSubmit={submit}
        class="border-t border-border-subtle p-3 bg-bg-1 space-y-2"
      >
        <Show when={sendError()}>
          <Toast tone="danger">{sendError()}</Toast>
        </Show>
        <div class="flex gap-2 items-end">
          <div class="flex-1">
            <Textarea
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
            />
          </div>
          <Button
            type="submit"
            variant="primary"
            icon="arrow-right"
            disabled={sending() || input().trim() === ""}
          >
            {sending() ? "Sending…" : "Send"}
          </Button>
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
        <div class="rounded border border-border-subtle bg-bg-1/30 px-2 py-1 text-xs text-fg-2 italic">
          {props.msg.body}
        </div>
      }
    >
      <div
        class="rounded border px-2 py-1 text-sm"
        classList={{
          "border-success bg-success-bg": props.msg.sender === "user",
          "border-accent bg-accent-bg": props.msg.sender !== "user",
        }}
      >
        <div class="text-xs font-mono text-fg-2 mb-0.5">
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
  const tone = () =>
    props.status === "live"
      ? "success"
      : props.status === "lagged"
        ? "warning"
        : props.status === "closed"
          ? "danger"
          : "neutral";
  const label = () =>
    props.status === "lagged" && props.dropped > 0
      ? `lagged · ${props.dropped} dropped`
      : props.status;
  return (
    <Badge tone={tone()} dot>
      {label()}
    </Badge>
  );
}
