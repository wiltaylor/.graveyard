import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  type JSX,
} from "solid-js";

import Modal from "./Modal";

export interface ChipOption {
  value: string;
  label?: string;
  hint?: string;
}

interface Props {
  /** Catalog of selectable values. */
  options: ChipOption[];
  /** Currently-selected option values. */
  value: string[];
  onChange: (next: string[]) => void;
  label?: string;
  help?: string;
  placeholder?: string;
  /** Allow values not present in `options` (free-form additions). */
  allowCustom?: boolean;
}

/**
 * Searchable multi-select with chip display.
 *
 * The field shows the current selections inline as removable chips.
 * Clicking the field opens a centered modal with a large search box
 * and a scrollable list of available options — the modal gives plenty
 * of room for long catalogs (tools, skills, etc.) and avoids any
 * clipping issues from ancestor `overflow:auto` containers.
 */
export default function ChipPicker(props: Props): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [highlight, setHighlight] = createSignal(0);
  let searchEl: HTMLInputElement | undefined;

  const selected = createMemo(() => new Set(props.value));

  const visible = createMemo(() => {
    const q = search().trim().toLowerCase();
    const sel = selected();
    let pool = props.options.filter((o) => !sel.has(o.value));
    if (q) {
      pool = pool.filter((o) => {
        const hay = `${o.label ?? o.value} ${o.hint ?? ""}`.toLowerCase();
        return hay.includes(q);
      });
    }
    return pool;
  });

  const showAddCustom = createMemo(() => {
    if (!props.allowCustom) return false;
    const q = search().trim();
    if (!q) return false;
    if (selected().has(q)) return false;
    return !props.options.some((o) => o.value === q);
  });

  const add = (value: string) => {
    if (!value) return;
    if (selected().has(value)) return;
    props.onChange([...props.value, value]);
    setSearch("");
    setHighlight(0);
    searchEl?.focus();
  };

  const remove = (value: string) => {
    props.onChange(props.value.filter((v) => v !== value));
  };

  const onKey = (ev: KeyboardEvent) => {
    if (ev.key === "ArrowDown") {
      ev.preventDefault();
      const total = visible().length + (showAddCustom() ? 1 : 0);
      if (total > 0) setHighlight((h) => Math.min(h + 1, total - 1));
    } else if (ev.key === "ArrowUp") {
      ev.preventDefault();
      setHighlight((h) => Math.max(h - 1, 0));
    } else if (ev.key === "Enter") {
      ev.preventDefault();
      const list = visible();
      const i = highlight();
      if (i < list.length) {
        add(list[i].value);
      } else if (showAddCustom()) {
        add(search().trim());
      }
    }
  };

  // Reset highlight whenever the filter shifts the visible list around
  // — keeps the keyboard cursor inside the new bounds.
  createEffect(() => {
    void visible().length;
    setHighlight(0);
  });

  // Auto-focus the search box when the modal opens.
  createEffect(() => {
    if (open()) {
      // Defer until after the modal mounts.
      queueMicrotask(() => searchEl?.focus());
    } else {
      setSearch("");
      setHighlight(0);
    }
  });

  const labelFor = (value: string): string => {
    const o = props.options.find((x) => x.value === value);
    return o?.label ?? value;
  };

  return (
    <label class="ffield">
      <Show when={props.label}>
        <span class="ffield-label">{props.label}</span>
      </Show>
      <div
        class="ffield-input"
        style={{
          display: "flex",
          "flex-wrap": "wrap",
          "align-content": "center",
          gap: "6px",
          "align-items": "center",
          "min-height": "36px",
          padding: "6px 8px",
          cursor: "pointer",
        }}
        onClick={() => setOpen(true)}
      >
        <Show
          when={props.value.length > 0}
          fallback={
            <span
              style={{
                color: "var(--fg-3)",
                "font-size": "13px",
                "line-height": "20px",
              }}
            >
              {props.placeholder ?? "Click to choose…"}
            </span>
          }
        >
          <For each={props.value}>
            {(v) => <Chip label={labelFor(v)} onRemove={() => remove(v)} />}
          </For>
        </Show>
      </div>
      <Show when={props.help}>
        <span class="ffield-help">{props.help}</span>
      </Show>

      <Modal
        open={open()}
        onClose={() => setOpen(false)}
        title={props.label ?? "Pick values"}
        width={520}
        footer={
          <button
            type="button"
            class="fbtn fbtn-primary"
            onClick={() => setOpen(false)}
          >
            Done
          </button>
        }
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <Show when={props.value.length > 0}>
            <div
              style={{
                display: "flex",
                "flex-wrap": "wrap",
                gap: "6px",
                padding: "8px",
                background: "var(--bg-0)",
                border: "1px solid var(--border-subtle)",
                "border-radius": "var(--r-md)",
              }}
            >
              <For each={props.value}>
                {(v) => <Chip label={labelFor(v)} onRemove={() => remove(v)} />}
              </For>
            </div>
          </Show>
          <input
            ref={(el) => {
              searchEl = el;
            }}
            type="text"
            value={search()}
            placeholder={props.placeholder ?? "Search…"}
            onInput={(e) => setSearch(e.currentTarget.value)}
            onKeyDown={onKey}
            style={{
              width: "100%",
              height: "32px",
              padding: "0 10px",
              background: "var(--bg-0)",
              border: "1px solid var(--border)",
              "border-radius": "var(--r-md)",
              color: "var(--fg-0)",
              font: "inherit",
              "font-size": "13px",
              outline: "none",
            }}
          />
          <div
            style={{
              "max-height": "320px",
              "min-height": "120px",
              overflow: "auto",
              border: "1px solid var(--border-subtle)",
              "border-radius": "var(--r-md)",
              background: "var(--bg-0)",
            }}
          >
            <Show
              when={visible().length > 0 || showAddCustom()}
              fallback={
                <div
                  style={{
                    padding: "16px",
                    "text-align": "center",
                    color: "var(--fg-2)",
                    "font-size": "12.5px",
                  }}
                >
                  No matches.
                </div>
              }
            >
              <For each={visible()}>
                {(opt, idx) => (
                  <button
                    type="button"
                    onClick={() => add(opt.value)}
                    onMouseEnter={() => setHighlight(idx())}
                    style={{
                      width: "100%",
                      display: "flex",
                      "align-items": "baseline",
                      gap: "8px",
                      "text-align": "left",
                      padding: "8px 12px",
                      background:
                        highlight() === idx() ? "var(--bg-2)" : "transparent",
                      border: "0",
                      color: "var(--fg-0)",
                      font: "inherit",
                      "font-size": "13px",
                      cursor: "pointer",
                    }}
                  >
                    <span style={{ "font-family": "var(--font-mono)" }}>
                      {opt.label ?? opt.value}
                    </span>
                    <Show when={opt.hint}>
                      <span
                        style={{ color: "var(--fg-2)", "font-size": "11.5px" }}
                      >
                        {opt.hint}
                      </span>
                    </Show>
                  </button>
                )}
              </For>
              <Show when={showAddCustom()}>
                <button
                  type="button"
                  onClick={() => add(search().trim())}
                  onMouseEnter={() => setHighlight(visible().length)}
                  style={{
                    width: "100%",
                    display: "block",
                    "text-align": "left",
                    padding: "8px 12px",
                    background:
                      highlight() === visible().length
                        ? "var(--bg-2)"
                        : "transparent",
                    border: "0",
                    "border-top": "1px solid var(--border-subtle)",
                    color: "var(--fg-1)",
                    font: "inherit",
                    "font-size": "12.5px",
                    "font-style": "italic",
                    cursor: "pointer",
                  }}
                >
                  Add “{search().trim()}”
                </button>
              </Show>
            </Show>
          </div>
        </div>
      </Modal>
    </label>
  );
}

function Chip(props: { label: string; onRemove: () => void }): JSX.Element {
  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        gap: "4px",
        padding: "3px 8px",
        background: "var(--bg-2)",
        border: "1px solid var(--border)",
        "border-radius": "999px",
        "font-size": "12px",
        "line-height": "16px",
        "font-family": "var(--font-mono)",
        color: "var(--fg-0)",
        "white-space": "nowrap",
      }}
    >
      {props.label}
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          props.onRemove();
        }}
        aria-label={`Remove ${props.label}`}
        style={{
          border: "none",
          background: "transparent",
          color: "var(--fg-3)",
          cursor: "pointer",
          padding: "0",
          "font-size": "14px",
          "line-height": "1",
          display: "inline-flex",
          "align-items": "center",
        }}
      >
        ×
      </button>
    </span>
  );
}
