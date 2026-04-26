import { Show, createEffect, onCleanup, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import Icon from "./Icon";

interface Props {
  open: boolean;
  onClose: () => void;
  title?: JSX.Element;
  width?: number;
  children: JSX.Element;
  footer?: JSX.Element;
  dismissOnBackdrop?: boolean;
}

export default function Modal(props: Props): JSX.Element {
  const handleKey = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") props.onClose();
  };

  createEffect(() => {
    if (props.open) {
      document.addEventListener("keydown", handleKey);
      onCleanup(() => document.removeEventListener("keydown", handleKey));
    }
  });

  return (
    <Show when={props.open}>
      <Portal>
        <div
          role="dialog"
          aria-modal="true"
          style={{
            position: "fixed",
            inset: "0",
            background: "rgb(0 0 0 / 0.5)",
            "backdrop-filter": "blur(4px)",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            "z-index": "300",
          }}
          onClick={(e) => {
            if (props.dismissOnBackdrop !== false && e.target === e.currentTarget) {
              props.onClose();
            }
          }}
        >
          <div
            style={{
              background: "var(--bg-1)",
              border: "1px solid var(--border-strong)",
              "border-radius": "var(--r-lg)",
              width: `${props.width ?? 520}px`,
              "max-width": "calc(100vw - 32px)",
              "max-height": "calc(100vh - 64px)",
              display: "flex",
              "flex-direction": "column",
              overflow: "hidden",
            }}
          >
            <Show when={props.title}>
              <header
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "space-between",
                  padding: "14px 18px",
                  "border-bottom": "1px solid var(--border-subtle)",
                }}
              >
                <h3
                  style={{
                    "font-size": "14px",
                    "font-weight": "600",
                    color: "var(--fg-0)",
                    margin: "0",
                  }}
                >
                  {props.title}
                </h3>
                <button
                  type="button"
                  onClick={() => props.onClose()}
                  aria-label="Close"
                  style={{
                    background: "transparent",
                    border: "0",
                    color: "var(--fg-2)",
                    cursor: "pointer",
                    padding: "4px",
                    "border-radius": "4px",
                    display: "inline-flex",
                  }}
                >
                  <Icon name="x" size={16} />
                </button>
              </header>
            </Show>
            <div style={{ padding: "18px", overflow: "auto", flex: "1 1 auto" }}>
              {props.children}
            </div>
            <Show when={props.footer}>
              <footer
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "flex-end",
                  gap: "8px",
                  padding: "12px 18px",
                  "border-top": "1px solid var(--border-subtle)",
                }}
              >
                {props.footer}
              </footer>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
