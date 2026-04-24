import { For, Show, createEffect, createSignal, type JSX } from "solid-js";

import { api } from "../api";
import type { WireAiCore, WireTeamInfo } from "../types";

interface Props {
  instanceId: string;
}

export default function TeamView(props: Props): JSX.Element {
  const [info, setInfo] = createSignal<WireTeamInfo | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await api.teamInfo(props.instanceId);
      if (!resp.ok || !resp.info) {
        setError(resp.error ?? "remote returned no data");
        setInfo(null);
      } else {
        setInfo(resp.info);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    void props.instanceId;
    void load();
  });

  return (
    <div class="h-full overflow-y-auto p-4 space-y-4">
      <header class="flex items-center justify-between">
        <h2 class="text-lg font-semibold">Team</h2>
        <div class="flex items-center gap-2">
          <Show when={loading()}>
            <span class="text-xs text-slate-400">loading…</span>
          </Show>
          <button
            onClick={load}
            class="text-xs text-slate-400 hover:text-slate-100"
          >
            refresh
          </button>
        </div>
      </header>

      <Show when={error()}>
        <div class="rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <Show when={info()}>
        {(v) => (
          <>
            <section class="rounded border border-slate-800 bg-slate-900/60 p-3 space-y-2">
              <div class="flex items-baseline gap-3">
                <h3 class="text-base font-semibold">{v().team.id}</h3>
                <Show when={v().team.default_ai_core}>
                  <span class="text-xs text-slate-400">
                    default core:{" "}
                    <span class="font-mono text-slate-200">
                      {v().team.default_ai_core}
                    </span>
                  </span>
                </Show>
              </div>
              <p class="text-sm whitespace-pre-wrap text-slate-200">
                {v().team.description}
              </p>
              <Show when={v().team.replace_thinking_messages}>
                <p class="text-xs text-amber-300">
                  custom thinking messages replace built-ins
                </p>
              </Show>
            </section>

            <section>
              <h3 class="text-xs uppercase tracking-wider text-slate-400 mb-2">
                Fleet
              </h3>
              <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-5 gap-2">
                <FleetCell label="Agents" count={v().fleet.agents} />
                <FleetCell label="Workers" count={v().fleet.workers} />
                <FleetCell
                  label="Automatons"
                  count={v().fleet.automatons}
                />
                <FleetCell label="Channels" count={v().fleet.channels} />
                <FleetCell label="Workflows" count={v().fleet.workflows} />
              </div>
            </section>

            <section>
              <h3 class="text-xs uppercase tracking-wider text-slate-400 mb-2">
                AI Cores
              </h3>
              <Show
                when={v().ai_cores.length > 0}
                fallback={
                  <div class="text-sm text-slate-500 italic">
                    No ai_core blocks declared.
                  </div>
                }
              >
                <div class="space-y-2">
                  <For each={v().ai_cores}>
                    {(core) => <CoreCard core={core} />}
                  </For>
                </div>
              </Show>
            </section>
          </>
        )}
      </Show>
    </div>
  );
}

function FleetCell(props: { label: string; count: number }): JSX.Element {
  return (
    <div class="rounded border border-slate-800 bg-slate-900/40 px-3 py-2">
      <div class="text-xs uppercase tracking-wider text-slate-400">
        {props.label}
      </div>
      <div class="text-lg font-semibold">{props.count}</div>
    </div>
  );
}

function CoreCard(props: { core: WireAiCore }): JSX.Element {
  return (
    <article class="rounded border border-slate-800 bg-slate-900/60 p-3">
      <header class="flex items-center gap-2 flex-wrap">
        <h4 class="font-semibold">{props.core.id}</h4>
        <span class="rounded bg-slate-800 px-1.5 py-0.5 text-xs font-mono text-slate-300">
          {props.core.kind_label}
        </span>
        <Show when={props.core.is_team_default}>
          <span class="rounded border border-emerald-700 bg-emerald-900/30 px-1.5 py-0.5 text-xs text-emerald-200">
            team default
          </span>
        </Show>
        <Show when={props.core.usage_provider}>
          <span class="rounded border border-sky-800 bg-sky-950/40 px-1.5 py-0.5 text-xs text-sky-200">
            usage: {props.core.usage_provider}
          </span>
        </Show>
      </header>
      <dl class="mt-2 grid grid-cols-1 sm:grid-cols-2 gap-y-1 gap-x-4 text-sm">
        <Show when={props.core.model}>
          <div>
            <dt class="text-xs text-slate-400 uppercase tracking-wider">
              Model
            </dt>
            <dd class="font-mono">{props.core.model}</dd>
          </div>
        </Show>
        <Show when={props.core.credential}>
          <div>
            <dt class="text-xs text-slate-400 uppercase tracking-wider">
              Credential
            </dt>
            <dd class="font-mono">{props.core.credential}</dd>
          </div>
        </Show>
        <Show when={props.core.default_tools && props.core.default_tools.length > 0}>
          <div class="sm:col-span-2">
            <dt class="text-xs text-slate-400 uppercase tracking-wider">
              Default tools
            </dt>
            <dd class="flex flex-wrap gap-1 mt-0.5">
              <For each={props.core.default_tools ?? []}>
                {(t) => (
                  <span class="rounded bg-slate-800 px-1.5 py-0.5 text-xs">
                    {t}
                  </span>
                )}
              </For>
            </dd>
          </div>
        </Show>
        <Show when={props.core.has_system_prompt}>
          <div>
            <dt class="text-xs text-slate-400 uppercase tracking-wider">
              System prompt
            </dt>
            <dd class="text-slate-400 italic">configured</dd>
          </div>
        </Show>
        <Show when={props.core.agents_using.length > 0}>
          <div class="sm:col-span-2">
            <dt class="text-xs text-slate-400 uppercase tracking-wider">
              Used by
            </dt>
            <dd class="flex flex-wrap gap-1 mt-0.5">
              <For each={props.core.agents_using}>
                {(id) => (
                  <span class="rounded bg-slate-800 px-1.5 py-0.5 text-xs font-mono">
                    {id}
                  </span>
                )}
              </For>
            </dd>
          </div>
        </Show>
      </dl>
    </article>
  );
}
