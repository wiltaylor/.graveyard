import { For, Show, createSignal, type JSX } from "solid-js";

export type SettingsNodeId = "users";

interface SettingsNode {
  id: SettingsNodeId;
  label: string;
}

const NODES: SettingsNode[] = [{ id: "users", label: "Users" }];

interface Props {
  selected: SettingsNodeId | null;
  onSelect: (id: SettingsNodeId) => void;
}

export default function SettingsTree(props: Props): JSX.Element {
  const [open, setOpen] = createSignal(true);

  return (
    <ul class="space-y-1">
      <li>
        <div
          class="flex items-center gap-1 rounded px-2 py-1 hover:bg-slate-800 cursor-pointer"
          onClick={() => setOpen(!open())}
        >
          <span
            class="w-4 text-slate-400"
            aria-label={open() ? "Collapse" : "Expand"}
          >
            {open() ? "▾" : "▸"}
          </span>
          <span class="font-medium">Web UI</span>
        </div>
        <Show when={open()}>
          <ul class="ml-6 mt-1 space-y-0.5 text-sm">
            <For each={NODES}>
              {(node) => {
                const selected = () => props.selected === node.id;
                return (
                  <li>
                    <button
                      type="button"
                      onClick={() => props.onSelect(node.id)}
                      class="w-full text-left rounded px-2 py-0.5 text-slate-200 hover:bg-slate-800"
                      classList={{
                        "bg-slate-700 text-slate-50": selected(),
                      }}
                    >
                      {node.label}
                    </button>
                  </li>
                );
              }}
            </For>
          </ul>
        </Show>
      </li>
    </ul>
  );
}
