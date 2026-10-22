import type { JSX } from "solid-js";

interface Props {
  children: JSX.Element;
  class?: string;
}

export default function Eyebrow(props: Props): JSX.Element {
  return (
    <span class={`eyebrow${props.class ? ` ${props.class}` : ""}`}>
      {props.children}
    </span>
  );
}
