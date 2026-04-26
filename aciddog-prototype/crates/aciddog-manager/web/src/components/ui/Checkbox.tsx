import { Show, type JSX } from "solid-js";

interface Props {
  checked?: boolean;
  indeterminate?: boolean;
  disabled?: boolean;
  label?: JSX.Element;
  description?: JSX.Element;
  onChange?: (checked: boolean) => void;
  class?: string;
}

export default function Checkbox(props: Props): JSX.Element {
  const cls = () => {
    const states: string[] = ["cb-row"];
    if (props.indeterminate) states.push("indet");
    else if (props.checked) states.push("checked");
    if (props.disabled) states.push("disabled");
    if (props.class) states.push(props.class);
    return states.join(" ");
  };
  const handleClick = () => {
    if (props.disabled) return;
    props.onChange?.(!props.checked);
  };
  return (
    <label class={cls()} onClick={handleClick}>
      <span class="cb-box" aria-hidden="true" />
      <span>
        {props.label}
        <Show when={props.description}>
          <span class="desc">{props.description}</span>
        </Show>
      </span>
    </label>
  );
}
