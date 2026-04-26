import { For, Show, createSignal, type JSX } from "solid-js";
import Icon from "./Icon";

export interface DroplistItem {
  id: string;
  label: JSX.Element;
  hint?: JSX.Element;
}

interface Props {
  available: DroplistItem[];
  selected: DroplistItem[];
  onChange: (selected: DroplistItem[]) => void;
  availableTitle?: JSX.Element;
  selectedTitle?: JSX.Element;
  searchPlaceholder?: string;
  height?: string;
}

export default function Droplist(props: Props): JSX.Element {
  const [filter, setFilter] = createSignal("");
  const [activeAvailable, setActiveAvailable] = createSignal<string | null>(null);
  const [activeSelected, setActiveSelected] = createSignal<string | null>(null);

  const visibleAvailable = () => {
    const f = filter().toLowerCase();
    const taken = new Set(props.selected.map((s) => s.id));
    const left = props.available.filter((a) => !taken.has(a.id));
    if (!f) return left;
    return left.filter((a) =>
      typeof a.label === "string"
        ? a.label.toLowerCase().includes(f)
        : a.id.toLowerCase().includes(f),
    );
  };

  const add = (id: string) => {
    const item = props.available.find((a) => a.id === id);
    if (!item) return;
    if (props.selected.some((s) => s.id === id)) return;
    props.onChange([...props.selected, item]);
  };
  const remove = (id: string) => {
    props.onChange(props.selected.filter((s) => s.id !== id));
  };
  const move = (id: string, dir: -1 | 1) => {
    const idx = props.selected.findIndex((s) => s.id === id);
    if (idx < 0) return;
    const next = [...props.selected];
    const target = idx + dir;
    if (target < 0 || target >= next.length) return;
    [next[idx], next[target]] = [next[target], next[idx]];
    props.onChange(next);
  };

  const colStyle = {
    display: "flex",
    "flex-direction": "column" as const,
    background: "var(--bg-1)",
    border: "1px solid var(--border)",
    "border-radius": "var(--r-md)",
    overflow: "hidden",
    height: props.height ?? "320px",
  };

  return (
    <div
      class="dd-wrap"
      style={{
        display: "grid",
        "grid-template-columns": "1fr auto 1fr",
        gap: "12px",
        "align-items": "stretch",
      }}
    >
      <div class="dd-col" style={colStyle}>
        <header
          class="dd-head"
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "8px 10px",
            "border-bottom": "1px solid var(--border-subtle)",
            background: "var(--bg-2)",
            "font-size": "10px",
            "letter-spacing": "0.08em",
            "text-transform": "uppercase",
            "font-weight": "600",
            color: "var(--fg-2)",
          }}
        >
          <span>{props.availableTitle ?? "Available"}</span>
          <span style={{ color: "var(--fg-3)" }}>{visibleAvailable().length}</span>
        </header>
        <div style={{ padding: "6px 8px", "border-bottom": "1px solid var(--border-subtle)" }}>
          <input
            class="dd-search"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
            placeholder={props.searchPlaceholder ?? "Search…"}
            style={{
              width: "100%",
              height: "26px",
              padding: "0 8px",
              background: "var(--bg-0)",
              border: "1px solid var(--border)",
              "border-radius": "3px",
              color: "var(--fg-0)",
              font: "inherit",
              "font-size": "12px",
              outline: "none",
            }}
          />
        </div>
        <div class="dd-list" style={{ flex: "1", overflow: "auto", padding: "4px 0" }}>
          <For each={visibleAvailable()}>
            {(item) => (
              <button
                type="button"
                class="dd-item"
                onClick={() => setActiveAvailable(item.id)}
                onDblClick={() => add(item.id)}
                style={{
                  width: "100%",
                  display: "block",
                  "text-align": "left",
                  padding: "6px 12px",
                  background:
                    activeAvailable() === item.id ? "var(--bg-2)" : "transparent",
                  border: "0",
                  color: "var(--fg-0)",
                  font: "inherit",
                  "font-size": "13px",
                  cursor: "pointer",
                }}
              >
                <span>{item.label}</span>
                <Show when={item.hint}>
                  <span
                    style={{
                      "margin-left": "8px",
                      "font-size": "11px",
                      color: "var(--fg-2)",
                    }}
                  >
                    {item.hint}
                  </span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </div>
      <div
        style={{
          display: "flex",
          "flex-direction": "column",
          "align-items": "center",
          "justify-content": "center",
          gap: "6px",
        }}
      >
        <button
          type="button"
          class="fbtn fbtn-secondary fbtn-sm"
          onClick={() => activeAvailable() && add(activeAvailable()!)}
          aria-label="Add"
        >
          <Icon name="arrow-right" size={12} />
        </button>
        <button
          type="button"
          class="fbtn fbtn-secondary fbtn-sm"
          onClick={() => activeSelected() && remove(activeSelected()!)}
          aria-label="Remove"
        >
          <Icon name="x" size={12} />
        </button>
        <button
          type="button"
          class="fbtn fbtn-ghost fbtn-sm"
          onClick={() => activeSelected() && move(activeSelected()!, -1)}
          aria-label="Move up"
        >
          ↑
        </button>
        <button
          type="button"
          class="fbtn fbtn-ghost fbtn-sm"
          onClick={() => activeSelected() && move(activeSelected()!, 1)}
          aria-label="Move down"
        >
          ↓
        </button>
      </div>
      <div class="dd-col" style={colStyle}>
        <header
          class="dd-head"
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "8px 10px",
            "border-bottom": "1px solid var(--border-subtle)",
            background: "var(--bg-2)",
            "font-size": "10px",
            "letter-spacing": "0.08em",
            "text-transform": "uppercase",
            "font-weight": "600",
            color: "var(--fg-2)",
          }}
        >
          <span>{props.selectedTitle ?? "Selected"}</span>
          <span style={{ color: "var(--fg-3)" }}>{props.selected.length}</span>
        </header>
        <div class="dd-list" style={{ flex: "1", overflow: "auto", padding: "4px 0" }}>
          <For each={props.selected}>
            {(item) => (
              <button
                type="button"
                class="dd-item"
                onClick={() => setActiveSelected(item.id)}
                onDblClick={() => remove(item.id)}
                style={{
                  width: "100%",
                  display: "block",
                  "text-align": "left",
                  padding: "6px 12px",
                  background:
                    activeSelected() === item.id ? "var(--bg-2)" : "transparent",
                  border: "0",
                  color: "var(--fg-0)",
                  font: "inherit",
                  "font-size": "13px",
                  cursor: "pointer",
                }}
              >
                <span>{item.label}</span>
                <Show when={item.hint}>
                  <span
                    style={{
                      "margin-left": "8px",
                      "font-size": "11px",
                      color: "var(--fg-2)",
                    }}
                  >
                    {item.hint}
                  </span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
