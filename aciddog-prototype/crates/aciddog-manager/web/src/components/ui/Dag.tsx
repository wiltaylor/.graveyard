import { Show, type JSX } from "solid-js";

interface Props {
  title?: JSX.Element;
  meta?: JSX.Element;
  toolbar?: JSX.Element;
  detail?: JSX.Element;
  height?: string;
  canvasClass?: string;
  canvasRef?: (el: HTMLDivElement) => void;
  children?: JSX.Element;
}

export default function Dag(props: Props): JSX.Element {
  return (
    <div
      class="dag-card"
      style={{
        height: props.height ?? "100%",
        "min-height": "0",
        "grid-template-rows": "auto 1fr",
      }}
    >
      <Show when={props.title || props.meta || props.toolbar}>
        <header class="dag-head">
          <Show when={props.title}>
            <span class="dag-title">{props.title}</span>
          </Show>
          <Show when={props.meta}>
            <span class="dag-meta">{props.meta}</span>
          </Show>
          <Show when={props.toolbar}>
            <span class="dag-tools">{props.toolbar}</span>
          </Show>
        </header>
      </Show>
      <div
        class="dag-body"
        style={{
          "grid-template-columns": props.detail ? "1fr 240px" : "1fr",
          "min-height": "0",
        }}
      >
        <div
          ref={props.canvasRef}
          class={`dag-canvas${props.canvasClass ? ` ${props.canvasClass}` : ""}`}
          style={{ height: "100%" }}
        >
          {props.children}
        </div>
        <Show when={props.detail}>
          <aside
            style={{
              "border-left": "1px solid var(--border-subtle)",
              background: "var(--bg-1)",
              padding: "12px 14px",
              "min-width": "0",
              overflow: "auto",
            }}
          >
            {props.detail}
          </aside>
        </Show>
      </div>
    </div>
  );
}
