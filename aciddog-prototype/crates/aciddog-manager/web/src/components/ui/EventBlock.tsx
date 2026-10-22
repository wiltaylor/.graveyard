import { For, Show, createSignal, type JSX } from "solid-js";

import Icon, { type IconName } from "./Icon";

export type EventKind = "info" | "success" | "warning" | "danger";

export type EventActionKind = "primary" | "danger" | "ghost";

export interface EventAction {
  label: JSX.Element;
  kind?: EventActionKind;
  onClick?: (e: MouseEvent) => void;
}

export interface EventBlockProps {
  kind?: EventKind;
  icon?: IconName | JSX.Element;
  summary: JSX.Element;
  meta?: JSX.Element;
  details?: JSX.Element;
  actions?: EventAction[];
  inline?: boolean;
  standalone?: boolean;
  defaultOpen?: boolean;
}

export default function EventBlock(props: EventBlockProps): JSX.Element {
  const [open, setOpen] = createSignal(props.defaultOpen ?? false);
  const expandable = () => props.details != null;

  const wrapClass = () => {
    const parts = ["evt"];
    if (props.kind) parts.push(props.kind);
    if (props.standalone) parts.push("standalone");
    return parts.join(" ");
  };

  const toggle = () => {
    if (expandable()) setOpen(!open());
  };

  const inlineActions = () =>
    props.inline === true && props.actions && props.actions.length > 0;
  const blockActions = () =>
    props.inline !== true && props.actions && props.actions.length > 0;

  return (
    <div
      class={wrapClass()}
      data-expandable={expandable() ? "true" : "false"}
      data-open={open() ? "true" : "false"}
    >
      <div class="evt-summary" onClick={toggle}>
        <span class="evt-icon">
          <Show
            when={typeof props.icon === "string"}
            fallback={props.icon as JSX.Element | undefined}
          >
            <Icon name={props.icon as IconName} size={16} />
          </Show>
        </span>
        <div class="evt-text">
          <div class="evt-summary-line">{props.summary}</div>
          <Show when={props.meta}>
            <div class="evt-meta">{props.meta}</div>
          </Show>
        </div>
        <div class="evt-right">
          <Show when={inlineActions()}>
            <ActionRow actions={props.actions!} inline />
          </Show>
          <Show when={expandable()}>
            <span class="evt-chev">
              <Icon name="chevron-right" size={16} />
            </span>
          </Show>
        </div>
      </div>

      <Show when={expandable()}>
        <div class="evt-details">
          <div class="evt-details-inner">{props.details}</div>
        </div>
      </Show>

      <Show when={blockActions()}>
        <ActionRow actions={props.actions!} inline={false} />
      </Show>
    </div>
  );
}

function ActionRow(props: { actions: EventAction[]; inline: boolean }): JSX.Element {
  return (
    <div class={`evt-actions${props.inline ? " inline" : ""}`}>
      <For each={props.actions}>
        {(a) => (
          <button
            type="button"
            class={`btn${a.kind ? ` ${a.kind}` : ""}`}
            onClick={(e) => {
              e.stopPropagation();
              a.onClick?.(e);
            }}
          >
            {a.label}
          </button>
        )}
      </For>
    </div>
  );
}
