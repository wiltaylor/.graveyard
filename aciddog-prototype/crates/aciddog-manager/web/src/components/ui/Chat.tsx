import { For, Show, type JSX } from "solid-js";

import EventBlock, { type EventBlockProps } from "./EventBlock";

export interface ChatMessage {
  kind?: "message";
  id: string;
  authorName: JSX.Element;
  authorBadge?: JSX.Element;
  avatarText?: string;
  avatarTone?: "default" | "self" | "bot";
  time?: JSX.Element;
  body: JSX.Element;
  self?: boolean;
}

export interface ChatEventItem {
  kind: "event";
  id: string;
  evt: EventBlockProps;
}

export interface ChatNodeItem {
  kind: "node";
  id: string;
  node: JSX.Element;
}

export type ChatItem = ChatMessage | ChatEventItem | ChatNodeItem;

interface Props {
  title?: JSX.Element;
  meta?: JSX.Element;
  messages: ChatItem[];
  footer?: JSX.Element;
  emptyMessage?: JSX.Element;
  height?: string;
  bodyRef?: (el: HTMLDivElement) => void;
}

export default function Chat(props: Props): JSX.Element {
  return (
    <div class="chat" style={{ height: props.height }}>
      <Show when={props.title || props.meta}>
        <header class="chat-head">
          <span class="title">
            <span class="dot" />
            <span>{props.title}</span>
          </span>
          <Show when={props.meta}>
            <span class="meta">{props.meta}</span>
          </Show>
        </header>
      </Show>
      <div class="chat-body" ref={props.bodyRef}>
        <Show
          when={props.messages.length > 0}
          fallback={
            <div
              style={{
                color: "var(--fg-2)",
                "font-size": "13px",
                "text-align": "center",
                padding: "32px",
              }}
            >
              {props.emptyMessage ?? "No messages yet."}
            </div>
          }
        >
          <For each={props.messages}>{(item) => <ChatItemView item={item} />}</For>
        </Show>
      </div>
      <Show when={props.footer}>{props.footer}</Show>
    </div>
  );
}

function ChatItemView(props: { item: ChatItem }): JSX.Element {
  return (
    <Show
      when={props.item.kind === "event"}
      fallback={
        <Show
          when={props.item.kind === "node"}
          fallback={<MessageRow msg={props.item as ChatMessage} />}
        >
          {(props.item as ChatNodeItem).node}
        </Show>
      }
    >
      <EventBlock {...(props.item as ChatEventItem).evt} standalone />
    </Show>
  );
}

function MessageRow(props: { msg: ChatMessage }): JSX.Element {
  const m = props.msg;
  return (
    <div class={`msg${m.self ? " self" : ""}`}>
      <span
        class={`avatar${
          m.avatarTone === "self"
            ? " self"
            : m.avatarTone === "bot"
              ? " bot"
              : ""
        }`}
      >
        {m.avatarText ?? "•"}
      </span>
      <div class="msg-main">
        <div class="msg-head">
          <span class={`name${m.self ? " self" : ""}`}>{m.authorName}</span>
          <Show when={m.authorBadge}>
            <span class={`badge${m.avatarTone === "bot" ? " bot" : ""}`}>
              {m.authorBadge}
            </span>
          </Show>
          <Show when={m.time}>
            <span class="time">{m.time}</span>
          </Show>
        </div>
        <div class="msg-body">{m.body}</div>
      </div>
    </div>
  );
}
