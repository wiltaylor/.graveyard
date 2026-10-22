import type { JSX } from "solid-js";

export type DotTone = "success" | "warning" | "danger" | "info" | "neutral";

interface Props {
  tone: DotTone;
  class?: string;
}

export default function StatusDot(props: Props): JSX.Element {
  return (
    <span class={`fdot fdot-${props.tone}${props.class ? ` ${props.class}` : ""}`} />
  );
}
