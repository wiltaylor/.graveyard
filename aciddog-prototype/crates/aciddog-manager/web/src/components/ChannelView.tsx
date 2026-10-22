import {
  For,
  Show,
  createEffect,
  createMemo,
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
import {
  Badge,
  Button,
  Chat,
  ChatComposer,
  Eyebrow,
  Toast,
  type ChatItem,
} from "./ui";

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

  const submit = async () => {
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

  const items = createMemo<ChatItem[]>(() =>
    messages().map((m, i) => messageToItem(m, i)),
  );

  return (
    <div class="flex flex-col h-[calc(100vh-6rem)]">
      <Show when={props.info?.description || (props.info?.members.length ?? 0) > 0}>
        <div class="border-b border-border-subtle px-6 py-2 bg-bg-1 space-y-2">
          <Show when={props.info?.description}>
            {(desc) => <p class="text-xs text-fg-2 m-0">{desc()}</p>}
          </Show>
          <Show when={(props.info?.members.length ?? 0) > 0}>
            <div class="flex items-center flex-wrap gap-1 text-xs">
              <Eyebrow>Members ({props.info!.members.length})</Eyebrow>
              <For each={props.info!.members}>
                {(m) => <Badge tone="neutral">{m}</Badge>}
              </For>
            </div>
          </Show>
        </div>
      </Show>

      <Show when={error()}>
        <div class="mx-6 mt-3">
          <Toast tone="danger">{error()}</Toast>
        </div>
      </Show>

      <div class="flex-1 min-h-0 p-3">
        <Chat
          height="100%"
          title={
            <>
              <Button
                size="sm"
                variant="ghost"
                icon="arrow-right"
                onClick={props.onClose}
              >
                Back
              </Button>
              <span>#{props.channel}</span>
              <Show when={props.info?.session_hash}>
                {(hash) => (
                  <span
                    class="font-mono text-xs text-fg-3"
                    title={
                      props.info?.session_started
                        ? `session started ${props.info.session_started}`
                        : undefined
                    }
                  >
                    [{hash()}]
                  </span>
                )}
              </Show>
            </>
          }
          meta={
            <span class="inline-flex items-center gap-2">
              <StreamStatusBadge status={status()} dropped={droppedCount()} />
              <Button
                size="sm"
                variant="ghost"
                icon="refresh"
                onClick={loadHistory}
              >
                Refresh
              </Button>
            </span>
          }
          messages={items()}
          emptyMessage={
            error() ? "channel unavailable." : "(no messages yet)"
          }
          footer={
            <ChatComposer
              value={input()}
              onInput={setInput}
              onSubmit={submit}
              sending={sending()}
              error={sendError()}
              placeholder={`Post to #${props.channel} as the human operator. Enter to send, Shift+Enter for newline.`}
            />
          }
        />
      </div>
    </div>
  );
}

function messageToItem(msg: WireChannelMessage, i: number): ChatItem {
  const id = `m-${i}`;
  if (msg.is_welcome) {
    return {
      kind: "event",
      id,
      evt: {
        kind: "info",
        icon: "info",
        summary: msg.body,
      },
    };
  }
  const isSelf = msg.sender === "user";
  return {
    kind: "message",
    id,
    self: isSelf,
    avatarTone: isSelf ? "self" : "bot",
    avatarText: msg.sender.slice(0, 2).toUpperCase(),
    authorName: msg.sender,
    authorBadge: isSelf ? "you" : "agent",
    body: msg.body,
  };
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
