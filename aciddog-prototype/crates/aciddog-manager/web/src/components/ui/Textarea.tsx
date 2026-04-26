import { Show, splitProps, type ComponentProps, type JSX } from "solid-js";

interface Props extends ComponentProps<"textarea"> {
  label?: string;
  help?: string;
  error?: boolean | string;
  mono?: boolean;
  optional?: boolean;
  containerClass?: string;
}

export default function Textarea(props: Props): JSX.Element {
  const [own, rest] = splitProps(props, [
    "label",
    "help",
    "error",
    "mono",
    "optional",
    "containerClass",
    "class",
  ]);
  const isError = () => Boolean(own.error);
  const meta = () => (typeof own.error === "string" ? own.error : own.help);
  return (
    <label
      class={`ta-field${own.mono ? " ta-mono" : ""}${isError() ? " error" : ""}${
        own.containerClass ? ` ${own.containerClass}` : ""
      }`}
    >
      <Show when={own.label}>
        <span>
          {own.label}
          <Show when={own.optional}>
            <span class="opt">optional</span>
          </Show>
        </span>
      </Show>
      <textarea class={own.class} {...rest} />
      <Show when={meta()}>
        <span class={`ta-meta${isError() ? " err" : ""}`}>{meta()}</span>
      </Show>
    </label>
  );
}
