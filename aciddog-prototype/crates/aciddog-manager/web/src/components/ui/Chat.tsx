import { For, Show, type JSX } from "solid-js";

export interface ChatMessage {
  id: string;
  authorName: JSX.Element;
  authorBadge?: JSX.Element;
  avatarText?: string;
  avatarTone?: "default" | "self" | "bot";
  time?: JSX.Element;
  body: JSX.Element;
  self?: boolean;
}

interface Props {
  title?: JSX.Element;
  meta?: JSX.Element;
  messages: ChatMessage[];
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
          <For each={props.messages}>
            {(m) => (
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
            )}
          </For>
        </Show>
      </div>
      <Show when={props.footer}>
        <footer
          style={{
            "border-top": "1px solid var(--border-subtle)",
            padding: "10px 14px",
            background: "var(--bg-1)",
          }}
        >
          {props.footer}
        </footer>
      </Show>
    </div>
  );
}
