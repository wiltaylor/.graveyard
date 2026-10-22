import { Show, createEffect, onCleanup, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

interface Props {
  open: boolean;
  onClose: () => void;
  side?: "left";
  width?: number;
  labelledBy?: string;
  triggerRef?: HTMLElement | (() => HTMLElement | null);
  children: JSX.Element;
}

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"]), input:not([disabled]), select:not([disabled]), textarea:not([disabled])';

export default function Drawer(props: Props): JSX.Element {
  let panelRef: HTMLDivElement | undefined;
  let wasOpen = false;

  const resolvedTriggerRef = () =>
    typeof props.triggerRef === "function"
      ? props.triggerRef()
      : props.triggerRef ?? null;

  const handleKey = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") props.onClose();
  };

  const focusFirst = () => {
    queueMicrotask(() => {
      if (!props.open || !panelRef) return;
      const first = panelRef.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
      (first ?? panelRef).focus();
    });
  };

  const trapTab = (ev: KeyboardEvent) => {
    if (ev.key !== "Tab" || !panelRef) return;

    const focusable = Array.from(
      panelRef.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
    );

    if (focusable.length === 0) {
      ev.preventDefault();
      panelRef.focus();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;

    if (ev.shiftKey && active === first) {
      ev.preventDefault();
      last.focus();
    } else if (!ev.shiftKey && active === last) {
      ev.preventDefault();
      first.focus();
    }
  };

  createEffect(() => {
    if (props.open) {
      document.addEventListener("keydown", handleKey);
      onCleanup(() => document.removeEventListener("keydown", handleKey));
    }
  });

  createEffect(() => {
    if (props.open) {
      wasOpen = true;
      focusFirst();
    } else if (wasOpen) {
      wasOpen = false;
      resolvedTriggerRef()?.focus();
    }
  });

  return (
    <>
      <div
        ref={panelRef}
        class="fsidebar-drawer-panel"
        data-side={props.side ?? "left"}
        role={props.open ? "dialog" : undefined}
        aria-modal={props.open ? "true" : undefined}
        aria-labelledby={props.open ? props.labelledBy : undefined}
        tabIndex={props.open ? -1 : undefined}
        onKeyDown={trapTab}
        style={{
          "--drawer-width": `${props.width ?? 280}px`,
        }}
      >
        {props.children}
      </div>
      <Show when={props.open}>
        <Portal>
          <div
            role="presentation"
            class="fsidebar-drawer-backdrop"
            onClick={(ev) => {
              if (ev.target === ev.currentTarget) props.onClose();
            }}
          />
        </Portal>
      </Show>
    </>
  );
}
