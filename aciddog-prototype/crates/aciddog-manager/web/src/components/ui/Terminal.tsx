import { For, Show, createEffect, type JSX } from "solid-js";

export type TerminalLevel = "info" | "success" | "warn" | "error" | "debug";

export interface TerminalLine {
  id: string;
  time?: JSX.Element;
  level?: TerminalLevel;
  tag?: JSX.Element;
  message: JSX.Element;
}

interface Props {
  title?: JSX.Element;
  meta?: JSX.Element;
  lines: TerminalLine[];
  follow?: boolean;
  height?: string;
  footer?: JSX.Element;
}

export default function Terminal(props: Props): JSX.Element {
  let bodyEl: HTMLDivElement | undefined;

  createEffect(() => {
    void props.lines.length;
    if (props.follow !== false && bodyEl) {
      bodyEl.scrollTop = bodyEl.scrollHeight;
    }
  });

  return (
    <div class="term" style={{ height: props.height }}>
      <Show when={props.title || props.meta}>
        <header class="term-head">
          <span class="term-title">
            <span class="dot" />
            {props.title}
          </span>
          <Show when={props.meta}>
            <span class="term-meta">{props.meta}</span>
          </Show>
        </header>
      </Show>
      <div class="term-body-wrap">
        <div class="term-body" ref={bodyEl}>
          <For each={props.lines}>
            {(line) => (
              <div class="tl" data-level={line.level ?? ""}>
                <Show when={line.time}>
                  <span class="tl-time">{line.time}</span>
                </Show>
                <Show when={line.tag ?? line.level}>
                  <span class="tl-tag">{line.tag ?? line.level}</span>
                </Show>
                <span class="tl-msg">{line.message}</span>
              </div>
            )}
          </For>
        </div>
      </div>
      <Show when={props.footer}>
        <div
          style={{
            "border-top": "1px solid var(--border-subtle)",
            background: "var(--bg-1)",
            padding: "8px 12px",
          }}
        >
          {props.footer}
        </div>
      </Show>
    </div>
  );
}
