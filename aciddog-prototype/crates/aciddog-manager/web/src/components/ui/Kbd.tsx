import type { JSX } from "solid-js";

interface Props {
  children: JSX.Element;
  class?: string;
}

export default function Kbd(props: Props): JSX.Element {
  return <kbd class={`fkbd${props.class ? ` ${props.class}` : ""}`}>{props.children}</kbd>;
}
