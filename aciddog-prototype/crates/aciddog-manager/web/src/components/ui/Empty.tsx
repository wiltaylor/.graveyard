import { Show, type JSX } from "solid-js";

interface Props {
  title?: JSX.Element;
  children?: JSX.Element;
  action?: JSX.Element;
  class?: string;
}

export default function Empty(props: Props): JSX.Element {
  return (
    <div class={`empty${props.class ? ` ${props.class}` : ""}`}>
      <Show when={props.title}>
        <h3>{props.title}</h3>
      </Show>
      <Show when={props.children}>
        <div>{props.children}</div>
      </Show>
      <Show when={props.action}>
        <div style={{ "margin-top": "12px" }}>{props.action}</div>
      </Show>
    </div>
  );
}
