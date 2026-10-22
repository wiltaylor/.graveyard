import { Show, splitProps, type ComponentProps, type JSX } from "solid-js";
import Icon, { type IconName } from "./Icon";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

interface Props extends ComponentProps<"button"> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: IconName | (string & {});
  trailingIcon?: IconName | (string & {});
}

export default function Button(props: Props): JSX.Element {
  const [own, rest] = splitProps(props, [
    "variant",
    "size",
    "icon",
    "trailingIcon",
    "class",
    "children",
    "type",
  ]);
  const variant = () => own.variant ?? "secondary";
  const size = () => own.size ?? "md";
  const cls = () =>
    `fbtn fbtn-${variant()} fbtn-${size()}${own.class ? ` ${own.class}` : ""}`;
  const iconSize = () => (size() === "sm" ? 12 : 14);
  return (
    <button type={own.type ?? "button"} class={cls()} {...rest}>
      <Show when={own.icon}>{(name) => <Icon name={name()} size={iconSize()} />}</Show>
      {own.children}
      <Show when={own.trailingIcon}>
        {(name) => <Icon name={name()} size={iconSize()} />}
      </Show>
    </button>
  );
}
