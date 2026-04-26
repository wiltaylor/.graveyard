import { For, createSignal, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import Toast, { type ToastTone } from "./Toast";

interface ToastEntry {
  id: number;
  tone: ToastTone;
  message: JSX.Element;
}

const [toasts, setToasts] = createSignal<ToastEntry[]>([]);
let nextId = 1;

export function pushToast(
  message: JSX.Element,
  tone: ToastTone = "info",
  durationMs = 4000,
): void {
  const id = nextId++;
  setToasts([...toasts(), { id, tone, message }]);
  if (durationMs > 0) {
    window.setTimeout(() => {
      setToasts(toasts().filter((t) => t.id !== id));
    }, durationMs);
  }
}

export function dismissToast(id: number): void {
  setToasts(toasts().filter((t) => t.id !== id));
}

export default function ToastHost(): JSX.Element {
  return (
    <Portal>
      <div
        style={{
          position: "fixed",
          bottom: "20px",
          right: "20px",
          display: "flex",
          "flex-direction": "column",
          gap: "8px",
          "z-index": "1000",
          "pointer-events": "none",
        }}
      >
        <For each={toasts()}>
          {(t) => (
            <div
              style={{ "pointer-events": "auto" }}
              onClick={() => dismissToast(t.id)}
            >
              <Toast tone={t.tone}>{t.message}</Toast>
            </div>
          )}
        </For>
      </div>
    </Portal>
  );
}
