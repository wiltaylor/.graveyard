import { For, Show, createSignal, type JSX } from "solid-js";
import type { AgentListResponse, InstanceSummary } from "../types";

interface Props {
  instances: InstanceSummary[];
  selectedId: string | null;
  selectedAgent: string | null;
  onSelect: (id: string) => void;
  onSelectAgent: (instanceId: string, agentId: string) => void;
  agentsFor: (id: string) => AgentListResponse | undefined;
  onExpand: (id: string) => void;
}

export default function InstanceTree(props: Props): JSX.Element {
  const [expanded, setExpanded] = createSignal<Record<string, boolean>>({});

  const toggle = (id: string) => {
    const isOpen = !!expanded()[id];
    setExpanded({ ...expanded(), [id]: !isOpen });
    if (!isOpen) {
      props.onExpand(id);
    }
  };

  return (
    <ul class="space-y-1">
      <Show
        when={props.instances.length > 0}
        fallback={
          <li class="text-sm text-slate-500 italic">
            No instances registered yet.
          </li>
        }
      >
        <For each={props.instances}>
          {(inst) => {
            const open = () => !!expanded()[inst.id];
            const selected = () => props.selectedId === inst.id;
            const agents = () => props.agentsFor(inst.id);
            return (
              <li>
                <div
                  class="flex items-center gap-1 rounded px-2 py-1 hover:bg-slate-800 cursor-pointer"
                  classList={{ "bg-slate-800": selected() }}
                  onClick={() => props.onSelect(inst.id)}
                >
                  <button
                    class="w-4 text-slate-400 hover:text-slate-100"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggle(inst.id);
                    }}
                    aria-label={open() ? "Collapse" : "Expand"}
                  >
                    {open() ? "▾" : "▸"}
                  </button>
                  <span class="font-medium">{inst.name}</span>
                  <span class="ml-1 text-xs text-slate-400">
                    ({inst.transport})
                  </span>
                </div>
                <Show when={open()}>
                  <ul class="ml-6 mt-1 space-y-0.5 text-sm">
                    <li class="text-slate-400 uppercase text-xs tracking-wider">
                      Agents
                    </li>
                    <Show
                      when={agents()}
                      fallback={
                        <li class="text-slate-500 italic">Loading…</li>
                      }
                    >
                      {(resp) => (
                        <Show
                          when={resp().ok && resp().agents.length > 0}
                          fallback={
                            <li class="text-amber-400 italic">
                              {resp().error ?? "No agents reported."}
                            </li>
                          }
                        >
                          <For each={resp().agents}>
                            {(agent) => {
                              const isSelected = () =>
                                props.selectedId === inst.id &&
                                props.selectedAgent === agent.id;
                              return (
                                <li>
                                  <button
                                    type="button"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      props.onSelectAgent(inst.id, agent.id);
                                    }}
                                    class="w-full text-left rounded px-2 py-0.5 text-slate-200 hover:bg-slate-800"
                                    classList={{
                                      "bg-slate-700 text-slate-50": isSelected(),
                                    }}
                                  >
                                    {agent.id}{" "}
                                    <span class="text-xs text-slate-500">
                                      ({agent.kind} · {agent.status})
                                    </span>
                                  </button>
                                </li>
                              );
                            }}
                          </For>
                        </Show>
                      )}
                    </Show>
                  </ul>
                </Show>
              </li>
            );
          }}
        </For>
      </Show>
    </ul>
  );
}
