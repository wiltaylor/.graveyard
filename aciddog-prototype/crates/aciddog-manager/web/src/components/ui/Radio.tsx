import { For, Show, type JSX } from "solid-js";

export interface RadioOption<T extends string = string> {
  value: T;
  label: JSX.Element;
  description?: JSX.Element;
  badge?: JSX.Element;
  disabled?: boolean;
}

interface RadioGroupProps<T extends string = string> {
  options: RadioOption<T>[];
  value: T | null;
  onChange: (value: T) => void;
  class?: string;
}

export function RadioGroup<T extends string = string>(
  props: RadioGroupProps<T>,
): JSX.Element {
  return (
    <div class={`rg-list${props.class ? ` ${props.class}` : ""}`}>
      <For each={props.options}>
        {(opt) => (
          <label
            class={`rg-item${props.value === opt.value ? " active" : ""}`}
            onClick={() => !opt.disabled && props.onChange(opt.value)}
          >
            <span class="rg-radio" aria-hidden="true" />
            <span class="body">
              <span class="title">{opt.label}</span>
              <Show when={opt.description}>
                <span class="desc">{opt.description}</span>
              </Show>
            </span>
            <Show when={opt.badge}>
              <span class="badge">{opt.badge}</span>
            </Show>
          </label>
        )}
      </For>
    </div>
  );
}

interface SegmentedProps<T extends string = string> {
  options: { value: T; label: JSX.Element }[];
  value: T;
  onChange: (value: T) => void;
  class?: string;
}

export function Segmented<T extends string = string>(
  props: SegmentedProps<T>,
): JSX.Element {
  return (
    <div class={`seg${props.class ? ` ${props.class}` : ""}`}>
      <For each={props.options}>
        {(opt) => (
          <button
            type="button"
            class={props.value === opt.value ? "active" : ""}
            onClick={() => props.onChange(opt.value)}
          >
            {opt.label}
          </button>
        )}
      </For>
    </div>
  );
}

export default RadioGroup;
