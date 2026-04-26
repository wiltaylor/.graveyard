import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";
import { Portal } from "solid-js/web";
import Icon, { type IconName } from "./Icon";

export interface MenuItem {
  id: string;
  label: JSX.Element;
  icon?: IconName | (string & {});
  hint?: JSX.Element;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
  onSelect?: () => void;
}

export interface MenuSection {
  title?: JSX.Element;
  items: MenuItem[];
}

interface Props {
  trigger: (api: {
    toggle: () => void;
    open: () => void;
    close: () => void;
    isOpen: () => boolean;
  }) => JSX.Element;
  sections?: MenuSection[];
  items?: MenuItem[];
  width?: number;
  align?: "start" | "end";
}

export default function Menu(props: Props): JSX.Element {
  const [isOpen, setOpen] = createSignal(false);
  const [pos, setPos] = createSignal<{ left: number; top: number; width: number } | null>(null);
  let triggerWrap: HTMLSpanElement | undefined;
  let menuEl: HTMLDivElement | undefined;

  const updatePosition = () => {
    if (!triggerWrap) return;
    const rect = triggerWrap.getBoundingClientRect();
    const width = props.width ?? Math.max(rect.width, 180);
    const left =
      props.align === "end" ? rect.right - width : rect.left;
    setPos({ left, top: rect.bottom + 4, width });
  };

  const open = () => {
    updatePosition();
    setOpen(true);
  };
  const close = () => setOpen(false);
  const toggle = () => (isOpen() ? close() : open());

  const onDocClick = (ev: MouseEvent) => {
    const target = ev.target as Node;
    if (menuEl?.contains(target) || triggerWrap?.contains(target)) return;
    close();
  };
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") close();
  };

  createEffect(() => {
    if (isOpen()) {
      document.addEventListener("mousedown", onDocClick);
      document.addEventListener("keydown", onKey);
      window.addEventListener("resize", updatePosition);
      window.addEventListener("scroll", updatePosition, true);
      onCleanup(() => {
        document.removeEventListener("mousedown", onDocClick);
        document.removeEventListener("keydown", onKey);
        window.removeEventListener("resize", updatePosition);
        window.removeEventListener("scroll", updatePosition, true);
      });
    }
  });

  const sections = (): MenuSection[] => {
    if (props.sections) return props.sections;
    return [{ items: props.items ?? [] }];
  };

  const handleSelect = (item: MenuItem) => {
    if (item.disabled) return;
    item.onSelect?.();
    close();
  };

  return (
    <>
      <span ref={triggerWrap} style={{ display: "inline-block" }}>
        {props.trigger({ toggle, open, close, isOpen })}
      </span>
      <Show when={isOpen() && pos()}>
        <Portal>
          <div
            ref={menuEl}
            class="forge-menu"
            style={{
              position: "fixed",
              left: `${pos()!.left}px`,
              top: `${pos()!.top}px`,
              width: `${pos()!.width}px`,
              background: "var(--bg-4)",
              border: "1px solid var(--border-strong)",
              "border-radius": "6px",
              overflow: "hidden",
              "z-index": "200",
              "font-size": "13px",
            }}
            role="menu"
          >
            <For each={sections()}>
              {(section, idx) => (
                <>
                  <Show when={idx() > 0}>
                    <div style={{ "border-top": "1px solid var(--border-subtle)" }} />
                  </Show>
                  <Show when={section.title}>
                    <div
                      style={{
                        padding: "8px 10px",
                        "font-size": "10px",
                        "letter-spacing": "0.08em",
                        "text-transform": "uppercase",
                        color: "var(--fg-2)",
                        "font-weight": "600",
                      }}
                    >
                      {section.title}
                    </div>
                  </Show>
                  <For each={section.items}>
                    {(item) => (
                      <>
                        <Show when={item.separatorBefore}>
                          <div
                            style={{ "border-top": "1px solid var(--border-subtle)" }}
                          />
                        </Show>
                        <button
                          type="button"
                          role="menuitem"
                          disabled={item.disabled}
                          onClick={() => handleSelect(item)}
                          style={{
                            width: "100%",
                            display: "flex",
                            "align-items": "center",
                            gap: "8px",
                            padding: "6px 10px",
                            background: "transparent",
                            border: "0",
                            "text-align": "left",
                            color: item.danger
                              ? "var(--danger-fg)"
                              : item.disabled
                                ? "var(--fg-3)"
                                : "var(--fg-1)",
                            cursor: item.disabled ? "not-allowed" : "pointer",
                            font: "inherit",
                            "font-size": "13px",
                          }}
                          onMouseEnter={(e) => {
                            if (!item.disabled)
                              (e.currentTarget as HTMLElement).style.background =
                                "var(--bg-2)";
                          }}
                          onMouseLeave={(e) => {
                            (e.currentTarget as HTMLElement).style.background =
                              "transparent";
                          }}
                        >
                          <Show when={item.icon}>
                            {(n) => <Icon name={n()} size={14} />}
                          </Show>
                          <span style={{ flex: "1" }}>{item.label}</span>
                          <Show when={item.hint}>
                            <span style={{ color: "var(--fg-2)", "font-size": "11px" }}>
                              {item.hint}
                            </span>
                          </Show>
                        </button>
                      </>
                    )}
                  </For>
                </>
              )}
            </For>
          </div>
        </Portal>
      </Show>
    </>
  );
}
