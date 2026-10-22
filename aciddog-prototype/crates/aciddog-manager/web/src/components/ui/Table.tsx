import { For, Show, type JSX } from "solid-js";

export interface Column<T> {
  key: string;
  header: JSX.Element;
  render: (row: T) => JSX.Element;
  mono?: boolean;
  width?: string;
  align?: "left" | "right" | "center";
}

interface Props<T> {
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  onRowClick?: (row: T) => void;
  selectedKey?: string | null;
  empty?: JSX.Element;
  class?: string;
}

export default function Table<T>(props: Props<T>): JSX.Element {
  return (
    <table class={`ftable${props.class ? ` ${props.class}` : ""}`}>
      <thead>
        <tr>
          <For each={props.columns}>
            {(col) => (
              <th
                style={{
                  "text-align": col.align,
                  width: col.width,
                }}
              >
                {col.header}
              </th>
            )}
          </For>
        </tr>
      </thead>
      <tbody>
        <Show
          when={props.rows.length > 0}
          fallback={
            <Show when={props.empty}>
              <tr>
                <td
                  colspan={props.columns.length}
                  style={{ "text-align": "center", color: "var(--fg-2)" }}
                >
                  {props.empty}
                </td>
              </tr>
            </Show>
          }
        >
          <For each={props.rows}>
            {(row) => (
              <tr
                onClick={() => props.onRowClick?.(row)}
                style={{
                  background:
                    props.selectedKey === props.rowKey(row)
                      ? "var(--bg-2)"
                      : undefined,
                }}
              >
                <For each={props.columns}>
                  {(col) => (
                    <td
                      class={col.mono ? "col-mono" : undefined}
                      style={{ "text-align": col.align }}
                    >
                      {col.render(row)}
                    </td>
                  )}
                </For>
              </tr>
            )}
          </For>
        </Show>
      </tbody>
    </table>
  );
}
