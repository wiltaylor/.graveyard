import { Show, type JSX } from "solid-js";

interface Props {
  title?: JSX.Element;
  action?: JSX.Element;
  padded?: boolean;
  scrollX?: boolean;
  class?: string;
  bodyClass?: string;
  children: JSX.Element;
}

export default function Card(props: Props): JSX.Element {
  const padded = () => props.padded !== false;
  const bodyClass = () => {
    const classes = [];
    if (padded() || props.scrollX) classes.push("fcard-body");
    if (!padded() && props.scrollX) classes.push("fcard-body-flush");
    if (props.bodyClass) classes.push(props.bodyClass);
    return classes.join(" ");
  };

  return (
    <section
      class={`fcard${props.scrollX ? " is-scroll-x" : ""}${
        props.class ? ` ${props.class}` : ""
      }`}
    >
      <Show when={props.title || props.action}>
        <header class="fcard-head">
          <h3 class="fcard-title">{props.title}</h3>
          <Show when={props.action}>{props.action}</Show>
        </header>
      </Show>
      <div class={bodyClass()}>{props.children}</div>
    </section>
  );
}
