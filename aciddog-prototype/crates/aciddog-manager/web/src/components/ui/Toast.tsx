import { Show, type JSX } from "solid-js";
import Icon, { type IconName } from "./Icon";

export type ToastTone = "success" | "warning" | "danger" | "info";

interface Props {
  tone?: ToastTone;
  icon?: IconName | (string & {}) | false;
  class?: string;
  children: JSX.Element;
}

const DEFAULT_ICON: Record<ToastTone, IconName> = {
  success: "check",
  warning: "alert-triangle",
  danger: "alert-triangle",
  info: "info",
};

export default function Toast(props: Props): JSX.Element {
  const tone = () => props.tone ?? "info";
  const icon = () => {
    if (props.icon === false) return null;
    return props.icon ?? DEFAULT_ICON[tone()];
  };
  return (
    <div class={`ftoast ftoast-${tone()}${props.class ? ` ${props.class}` : ""}`}>
      <Show when={icon()}>{(name) => <Icon name={name()} size={14} />}</Show>
      <span>{props.children}</span>
    </div>
  );
}
