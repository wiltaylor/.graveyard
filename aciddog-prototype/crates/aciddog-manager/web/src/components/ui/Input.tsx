import { Show, splitProps, type ComponentProps, type JSX } from "solid-js";
import Icon, { type IconName } from "./Icon";

interface Props extends ComponentProps<"input"> {
  label?: string;
  help?: string;
  error?: boolean | string;
  icon?: IconName | (string & {});
  trailingIcon?: IconName | (string & {});
  containerClass?: string;
}

export default function Input(props: Props): JSX.Element {
  const [own, rest] = splitProps(props, [
    "label",
    "help",
    "error",
    "icon",
    "trailingIcon",
    "containerClass",
    "class",
  ]);
  const isError = () => Boolean(own.error);
  const helpText = () =>
    typeof own.error === "string" ? own.error : own.help;
  return (
    <label class={`ffield${own.containerClass ? ` ${own.containerClass}` : ""}`}>
      <Show when={own.label}>
        <span class="ffield-label">{own.label}</span>
      </Show>
      <span class={`ffield-input${isError() ? " is-error" : ""}`}>
        <Show when={own.icon}>{(n) => <Icon name={n()} size={14} />}</Show>
        <input class={own.class} {...rest} />
        <Show when={own.trailingIcon}>
          {(n) => <Icon name={n()} size={14} />}
        </Show>
      </span>
      <Show when={helpText()}>
        <span class={`ffield-help${isError() ? " is-error" : ""}`}>
          {helpText()}
        </span>
      </Show>
    </label>
  );
}
