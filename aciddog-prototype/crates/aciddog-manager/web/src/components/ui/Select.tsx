import { Show, type JSX } from "solid-js";
import Icon from "./Icon";
import Menu from "./Menu";

export interface SelectOption<T extends string = string> {
  value: T;
  label: JSX.Element;
  hint?: JSX.Element;
  disabled?: boolean;
}

interface Props<T extends string = string> {
  options: SelectOption<T>[];
  value: T | null;
  onChange: (value: T) => void;
  placeholder?: JSX.Element;
  label?: string;
  help?: string;
  error?: boolean | string;
  disabled?: boolean;
  width?: number;
  containerClass?: string;
  triggerClass?: string;
}

export default function Select<T extends string = string>(
  props: Props<T>,
): JSX.Element {
  const current = () => props.options.find((o) => o.value === props.value) ?? null;
  const isError = () => Boolean(props.error);
  const helpText = () =>
    typeof props.error === "string" ? props.error : props.help;

  return (
    <label class={`ffield${props.containerClass ? ` ${props.containerClass}` : ""}`}>
      <Show when={props.label}>
        <span class="ffield-label">{props.label}</span>
      </Show>
      <Menu
        width={props.width}
        items={props.options.map((opt) => ({
          id: opt.value,
          label: opt.label,
          hint: opt.hint,
          disabled: opt.disabled,
          onSelect: () => props.onChange(opt.value),
        }))}
        trigger={({ toggle, isOpen }) => (
          <button
            type="button"
            class={`ffield-input${isError() ? " is-error" : ""}${props.triggerClass ? ` ${props.triggerClass}` : ""}`}
            disabled={props.disabled}
            onClick={toggle}
            aria-expanded={isOpen()}
            style={{
              width: "100%",
              cursor: props.disabled ? "not-allowed" : "pointer",
              "text-align": "left",
              "font-family": "inherit",
            }}
          >
            <span style={{ flex: "1", color: current() ? "var(--fg-0)" : "var(--fg-3)" }}>
              {current()?.label ?? props.placeholder ?? "Select…"}
            </span>
            <Icon name="chevron-down" size={14} />
          </button>
        )}
      />
      <Show when={helpText()}>
        <span class={`ffield-help${isError() ? " is-error" : ""}`}>
          {helpText()}
        </span>
      </Show>
    </label>
  );
}
