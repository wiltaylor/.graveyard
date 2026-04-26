import { Show, type JSX } from "solid-js";

export type StatTone = "neutral" | "success" | "danger";

interface Props {
  label: JSX.Element;
  value: JSX.Element;
  delta?: JSX.Element;
  tone?: StatTone;
}

export default function Stat(props: Props): JSX.Element {
  return (
    <div class="fstat">
      <div class="eyebrow">{props.label}</div>
      <div class="fstat-value">{props.value}</div>
      <Show when={props.delta != null}>
        <div class={`fstat-delta tone-${props.tone ?? "neutral"}`}>
          {props.delta}
        </div>
      </Show>
    </div>
  );
}
