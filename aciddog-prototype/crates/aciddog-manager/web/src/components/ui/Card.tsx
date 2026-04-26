import { Show, type JSX } from "solid-js";

interface Props {
  title?: JSX.Element;
  action?: JSX.Element;
  padded?: boolean;
  class?: string;
  bodyClass?: string;
  children: JSX.Element;
}

export default function Card(props: Props): JSX.Element {
  const padded = () => props.padded !== false;
  return (
    <section class={`fcard${props.class ? ` ${props.class}` : ""}`}>
      <Show when={props.title || props.action}>
        <header class="fcard-head">
          <h3 class="fcard-title">{props.title}</h3>
          <Show when={props.action}>{props.action}</Show>
        </header>
      </Show>
      <div
        class={`${padded() ? "fcard-body" : ""}${
          props.bodyClass ? ` ${props.bodyClass}` : ""
        }`}
      >
        {props.children}
      </div>
    </section>
  );
}
