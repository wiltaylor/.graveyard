import { createEffect, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import Icon, { type IconName } from "./Icon";

export interface TabItem<T extends string = string> {
  id: T;
  label: JSX.Element;
  ariaLabel?: string;
  icon?: IconName | (string & {});
  count?: JSX.Element;
  dot?: boolean;
  disabled?: boolean;
}

interface Props<T extends string = string> {
  items: TabItem<T>[];
  active: T;
  onChange: (id: T) => void;
  variant?: "default" | "pill" | "vertical";
  actions?: JSX.Element;
  iconOnlyOnMobile?: boolean;
  class?: string;
}

export default function Tabs<T extends string = string>(
  props: Props<T>,
): JSX.Element {
  const tabRefs = new Map<T, HTMLButtonElement>();
  let tablistRef: HTMLDivElement | undefined;
  let firstActiveEffect = true;
  const variant = () => props.variant ?? "default";
  const cls = () => {
    const classes = ["tabs"];
    if (variant() === "pill") classes.push("pill");
    if (variant() === "vertical") classes.push("vertical");
    if (props.iconOnlyOnMobile) classes.push("icon-only-md");
    if (props.class) classes.push(props.class);
    return classes.join(" ");
  };
  const ariaLabel = (tab: TabItem<T>) =>
    tab.ariaLabel ?? (typeof tab.label === "string" ? tab.label : undefined);

  const onKey = (ev: KeyboardEvent) => {
    const enabled = props.items.filter((t) => !t.disabled);
    if (enabled.length === 0) return;
    const idx = enabled.findIndex((t) => t.id === props.active);
    let next = idx;
    if (ev.key === "ArrowLeft" || ev.key === "ArrowUp") {
      next = (idx - 1 + enabled.length) % enabled.length;
    } else if (ev.key === "ArrowRight" || ev.key === "ArrowDown") {
      next = (idx + 1) % enabled.length;
    } else if (ev.key === "Home") {
      next = 0;
    } else if (ev.key === "End") {
      next = enabled.length - 1;
    } else {
      return;
    }
    ev.preventDefault();
    props.onChange(enabled[next].id);
  };

  createEffect(() => {
    const active = props.active;
    const el = tabRefs.get(active);
    queueMicrotask(() => {
      const isFirstRun = firstActiveEffect;
      firstActiveEffect = false;
      const list = tablistRef;
      if (!list || !list.isConnected || !el) return;
      if (isFirstRun) {
        const listRect = list.getBoundingClientRect();
        const elRect = el.getBoundingClientRect();
        const tolerance = 1;
        const fullyInside =
          elRect.left >= listRect.left - tolerance &&
          elRect.right <= listRect.right + tolerance &&
          elRect.top >= listRect.top - tolerance &&
          elRect.bottom <= listRect.bottom + tolerance;
        if (fullyInside) return;
      }
      el.scrollIntoView({ inline: "nearest", block: "nearest" });
    });
  });

  onMount(() => {
    const list = tablistRef;
    if (!list) return;

    const updateOverflow = () => {
      const maxScroll = list.scrollWidth - list.clientWidth;
      if (maxScroll <= 1) {
        list.dataset.overflow = "none";
      } else if (list.scrollLeft <= 1) {
        list.dataset.overflow = "right";
      } else if (list.scrollLeft >= maxScroll - 1) {
        list.dataset.overflow = "left";
      } else {
        list.dataset.overflow = "both";
      }
    };

    const resizeObserver = new ResizeObserver(updateOverflow);
    resizeObserver.observe(list);
    list.addEventListener("scroll", updateOverflow);
    updateOverflow();

    onCleanup(() => {
      resizeObserver.disconnect();
      list.removeEventListener("scroll", updateOverflow);
    });
  });

  return (
    <div class={cls()}>
      <div
        ref={(el) => {
          tablistRef = el;
        }}
        class="tablist"
        role="tablist"
        onKeyDown={onKey}
      >
        <For each={props.items}>
          {(t) => (
            <button
              ref={(el) => tabRefs.set(t.id, el)}
              type="button"
              role="tab"
              class="tab"
              aria-label={ariaLabel(t)}
              aria-selected={props.active === t.id}
              tabIndex={props.active === t.id ? 0 : -1}
              disabled={t.disabled}
              onClick={() => !t.disabled && props.onChange(t.id)}
            >
              <Show when={t.icon}>
                {(name) => (
                  <span class="ico">
                    <Icon name={name()} size={14} />
                  </span>
                )}
              </Show>
              <span>{t.label}</span>
              <Show when={t.count != null}>
                <span class="count">{t.count}</span>
              </Show>
              <Show when={t.dot}>
                <span class="dot" aria-hidden="true" />
              </Show>
            </button>
          )}
        </For>
        <Show when={props.actions}>
          <span class="actions">{props.actions}</span>
        </Show>
      </div>
    </div>
  );
}
