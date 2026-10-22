import { Show, type JSX } from "solid-js";

export type BadgeTone =
  | "neutral"
  | "success"
  | "warning"
  | "danger"
  | "info"
  | "accent";

interface Props {
  tone?: BadgeTone;
  dot?: boolean;
  class?: string;
  children: JSX.Element;
}

export default function Badge(props: Props): JSX.Element {
  const tone = () => props.tone ?? "neutral";
  return (
    <span
      class={`fbadge fbadge-${tone()}${props.class ? ` ${props.class}` : ""}`}
    >
      <Show when={props.dot}>
        <span class="fbadge-dot" />
      </Show>
      {props.children}
    </span>
  );
}
