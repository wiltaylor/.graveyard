//! Workflows tab — templates sidebar + property form + live instance
//! panel. SSE-driven: the initial list is fetched once per instance,
//! then every `WireWorkflowEvent` from
//! `/api/instances/:id/workflows/events` is applied to local state.
//!
//! State model mirrors the TUI: `WireWorkflowInstance.remove_in_ms` is
//! nonzero for succeeded instances and they auto-disappear after that
//! interval. Failed instances linger until the user dismisses them.

import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";

import { api, workflowEventsUrl } from "../api";
import type {
  WireStepProgress,
  WireTemplateStep,
  WireVarSnapshot,
  WireWorkflowEvent,
  WireWorkflowInstance,
  WireWorkflowTemplate,
  WorkflowInstanceStatus,
  WorkflowStepStatus,
} from "../types";
import { Badge, Button, Toast } from "./ui";

interface Props {
  instanceId: string;
}

const TAIL_CAP = 200;

const STATUS_LABEL: Record<WorkflowInstanceStatus, string> = {
  running: "Running",
  failed: "Failed",
  succeeded: "Succeeded",
};

const STATUS_CLASS: Record<WorkflowInstanceStatus, string> = {
  running: "border-accent bg-accent-bg text-accent-fg",
  failed: "border-danger bg-danger-bg text-danger-fg",
  succeeded: "border-success bg-success-bg text-success-fg",
};

const STEP_GLYPH: Record<WorkflowStepStatus, string> = {
  pending: "·",
  running: "◐",
  succeeded: "✓",
  failed: "✗",
  retrying: "↻",
};

/// Form state per visible property. Keys are property ids. Number
/// fields store the raw text the user typed so partial states don't
/// get coerced into NaN; we parse at submit time.
type FormValues = Record<string, unknown>;

export default function WorkflowsView(props: Props): JSX.Element {
  const [templates, setTemplates] = createSignal<WireWorkflowTemplate[]>([]);
  const [instances, setInstances] = createSignal<WireWorkflowInstance[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [streamStatus, setStreamStatus] = createSignal<
    "connecting" | "live" | "closed" | "lagged"
  >("connecting");

  const [selectedTemplateId, setSelectedTemplateId] = createSignal<
    string | null
  >(null);
  const [formValues, setFormValues] = createSignal<FormValues>({});
  const [formError, setFormError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);

  const [selectedInstanceId, setSelectedInstanceId] = createSignal<
    number | null
  >(null);

  // Tracks setTimeout handles for successful-instance auto-removal so
  // we can clear them when the user switches instances or a fresh
  // snapshot replaces state.
  const removalTimers = new Map<number, number>();

  const clearRemovalTimers = () => {
    for (const handle of removalTimers.values()) {
      window.clearTimeout(handle);
    }
    removalTimers.clear();
  };

  const scheduleRemoval = (instanceId: number, ms: number) => {
    if (removalTimers.has(instanceId)) {
      window.clearTimeout(removalTimers.get(instanceId));
    }
    const handle = window.setTimeout(() => {
      setInstances((xs) => xs.filter((i) => i.id !== instanceId));
      removalTimers.delete(instanceId);
      if (selectedInstanceId() === instanceId) {
        setSelectedInstanceId(null);
      }
    }, ms);
    removalTimers.set(instanceId, handle);
  };

  const loadTemplates = async (id: string) => {
    try {
      const resp = await api.listWorkflowTemplates(id);
      if (!resp.ok) {
        setError(resp.error ?? "failed to fetch workflow templates");
        setTemplates([]);
      } else {
        setTemplates(resp.templates);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const loadInstances = async (id: string) => {
    try {
      const resp = await api.listWorkflowInstances(id);
      if (!resp.ok) {
        setError(resp.error ?? "failed to fetch workflow instances");
        return;
      }
      setInstances(resp.instances);
      // Rewire auto-remove timers from the snapshot's remove_in_ms.
      clearRemovalTimers();
      for (const inst of resp.instances) {
        if (inst.status === "succeeded" && inst.remove_in_ms !== undefined) {
          scheduleRemoval(inst.id, inst.remove_in_ms);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  /// Mutate one instance in place. No-op if `id` is unknown.
  const patchInstance = (
    id: number,
    patch: (inst: WireWorkflowInstance) => WireWorkflowInstance,
  ) => {
    setInstances((xs) =>
      xs.map((inst) => (inst.id === id ? patch(inst) : inst)),
    );
  };

  /// Apply one SSE event to `instances`, mirroring the TUI's
  /// `on_workflow_event` logic.
  const applyEvent = (ev: WireWorkflowEvent) => {
    switch (ev.event) {
      case "started": {
        // New instance: refetch the snapshot to populate the full
        // `steps` list (the `started` event alone doesn't carry step
        // metadata). Tiny race window of "spinner showing" in trade
        // for correctness of the step timeline.
        void loadInstances(props.instanceId);
        if (selectedInstanceId() === null) {
          setSelectedInstanceId(ev.instance_id);
        }
        break;
      }
      case "step_started": {
        patchInstance(ev.instance_id, (inst) => ({
          ...inst,
          current_step: ev.step_idx,
          steps: inst.steps.map((s, i) =>
            i === ev.step_idx ? { ...s, status: "running" } : s,
          ),
        }));
        break;
      }
      case "step_output": {
        patchInstance(ev.instance_id, (inst) => {
          const next = [...inst.tail_output, ev.line];
          if (next.length > TAIL_CAP) {
            next.splice(0, next.length - TAIL_CAP);
          }
          return { ...inst, tail_output: next };
        });
        break;
      }
      case "var_changed": {
        patchInstance(ev.instance_id, (inst) => {
          const existing = inst.vars.findIndex((v) => v.name === ev.name);
          const entry: WireVarSnapshot = {
            name: ev.name,
            display: ev.display,
          };
          const vars =
            existing === -1
              ? [...inst.vars, entry].sort((a, b) => a.name.localeCompare(b.name))
              : inst.vars.map((v) => (v.name === ev.name ? entry : v));
          return { ...inst, vars };
        });
        break;
      }
      case "step_finished": {
        patchInstance(ev.instance_id, (inst) => {
          const steps = inst.steps.map((s, i) => {
            if (i !== ev.step_idx) return s;
            switch (ev.outcome.outcome) {
              case "ok":
                return { ...s, status: "succeeded" as const };
              case "failed":
                return {
                  ...s,
                  status: "failed" as const,
                  reason: ev.outcome.reason,
                };
              case "retry":
                return {
                  ...s,
                  status: "retrying" as const,
                  reason: ev.outcome.reason,
                };
            }
          });
          return { ...inst, steps };
        });
        break;
      }
      case "retrying": {
        patchInstance(ev.instance_id, (inst) => ({
          ...inst,
          current_step: ev.target_step_idx,
          steps: inst.steps.map((s, i) =>
            i === ev.target_step_idx
              ? { ...s, retry_count: ev.attempt, status: "pending" as const }
              : s,
          ),
        }));
        break;
      }
      case "finished": {
        if (ev.ok) {
          patchInstance(ev.instance_id, (inst) => ({
            ...inst,
            status: "succeeded" as const,
            remove_in_ms: 4000,
          }));
          scheduleRemoval(ev.instance_id, 4000);
        } else {
          patchInstance(ev.instance_id, (inst) => ({
            ...inst,
            status: "failed" as const,
            failure_reason: ev.reason,
          }));
        }
        break;
      }
    }
  };

  const dismiss = async (instanceId: number) => {
    try {
      const resp = await api.dismissWorkflowInstance(
        props.instanceId,
        instanceId,
      );
      if (!resp.ok) {
        setError(resp.error ?? "dismiss failed");
        return;
      }
      setInstances((xs) => xs.filter((i) => i.id !== instanceId));
      if (selectedInstanceId() === instanceId) {
        setSelectedInstanceId(null);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  // Boot per instanceId: load templates + instances, open SSE stream.
  createEffect(() => {
    const id = props.instanceId;
    setError(null);
    setSelectedInstanceId(null);
    setSelectedTemplateId(null);
    setFormValues({});
    setFormError(null);
    clearRemovalTimers();
    setLoading(true);
    void (async () => {
      await Promise.all([loadTemplates(id), loadInstances(id)]);
      setLoading(false);
    })();

    setStreamStatus("connecting");
    const es = new EventSource(workflowEventsUrl(id));
    es.onopen = () => setStreamStatus("live");
    es.onmessage = (m) => {
      try {
        const parsed = JSON.parse(m.data) as WireWorkflowEvent;
        applyEvent(parsed);
      } catch {
        // Malformed event — skip.
      }
    };
    es.addEventListener("lagged", () => {
      setStreamStatus("lagged");
      // Re-sync from snapshot after a lag so we don't desync forever.
      void loadInstances(id);
    });
    es.onerror = () => setStreamStatus("closed");
    onCleanup(() => {
      es.close();
      clearRemovalTimers();
    });
  });

  // Form handling --------------------------------------------------------

  const selectedTemplate = createMemo<WireWorkflowTemplate | null>(() => {
    const sel = selectedTemplateId();
    if (sel === null) return null;
    return templates().find((t) => t.id === sel) ?? null;
  });

  const seedFormFromTemplate = (t: WireWorkflowTemplate) => {
    const values: FormValues = {};
    for (const p of t.properties) {
      switch (p.kind.kind) {
        case "text":
          values[p.id] = p.kind.default ?? "";
          break;
        case "number":
          values[p.id] =
            p.kind.default !== undefined ? String(p.kind.default) : "";
          break;
        case "select":
          values[p.id] = p.kind.default ?? p.kind.options[0] ?? "";
          break;
        case "multi_select":
          values[p.id] = p.kind.default ?? [];
          break;
      }
    }
    setFormValues(values);
    setFormError(null);
  };

  const selectTemplate = (tpl: WireWorkflowTemplate) => {
    setSelectedTemplateId(tpl.id);
    seedFormFromTemplate(tpl);
  };

  const submitForm = async (e: SubmitEvent) => {
    e.preventDefault();
    const tpl = selectedTemplate();
    if (!tpl) return;

    // Validate + coerce each field the same way the daemon does.
    const properties: Record<string, unknown> = {};
    const values = formValues();
    for (const p of tpl.properties) {
      switch (p.kind.kind) {
        case "text": {
          const v = (values[p.id] ?? "") as string;
          if (p.required && v.trim() === "") {
            setFormError(`\`${p.id}\` is required`);
            return;
          }
          if (v !== "") properties[p.id] = v;
          break;
        }
        case "number": {
          const raw = ((values[p.id] ?? "") as string).trim();
          if (raw === "") {
            if (p.required) {
              setFormError(`\`${p.id}\` is required`);
              return;
            }
            break;
          }
          const n = Number(raw);
          if (!Number.isFinite(n)) {
            setFormError(`\`${p.id}\` must be a number`);
            return;
          }
          if (p.kind.min !== undefined && n < p.kind.min) {
            setFormError(`\`${p.id}\` must be ≥ ${p.kind.min}`);
            return;
          }
          if (p.kind.max !== undefined && n > p.kind.max) {
            setFormError(`\`${p.id}\` must be ≤ ${p.kind.max}`);
            return;
          }
          properties[p.id] = n;
          break;
        }
        case "select": {
          const v = values[p.id] as string | undefined;
          if (v === undefined || v === "") {
            if (p.required) {
              setFormError(`\`${p.id}\` is required`);
              return;
            }
          } else {
            properties[p.id] = v;
          }
          break;
        }
        case "multi_select": {
          const v = (values[p.id] as string[] | undefined) ?? [];
          if (p.required && v.length === 0) {
            setFormError(`\`${p.id}\` requires at least one selection`);
            return;
          }
          properties[p.id] = v;
          break;
        }
      }
    }

    setFormError(null);
    setSubmitting(true);
    try {
      const resp = await api.startWorkflow(props.instanceId, {
        workflow: tpl.id,
        properties,
      });
      if (!resp.ok) {
        setFormError(resp.error ?? "start failed");
        return;
      }
      // Snapshot refetches itself via the incoming `started` event.
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  // Rendering helpers ----------------------------------------------------

  const sortedInstances = createMemo<WireWorkflowInstance[]>(() => {
    // Failed first so they're impossible to miss, then running, then
    // succeeded. Within each bucket, newest (highest id) first.
    const rank: Record<WorkflowInstanceStatus, number> = {
      failed: 0,
      running: 1,
      succeeded: 2,
    };
    return [...instances()].sort((a, b) => {
      const r = rank[a.status] - rank[b.status];
      if (r !== 0) return r;
      return b.id - a.id;
    });
  });

  const selectedInstance = createMemo<WireWorkflowInstance | null>(() => {
    const id = selectedInstanceId();
    if (id === null) return null;
    return instances().find((i) => i.id === id) ?? null;
  });

  const templateForInstance = (inst: WireWorkflowInstance) =>
    templates().find((t) => t.id === inst.workflow_id);

  return (
    <div class="h-full flex flex-col">
      <header class="flex items-center justify-between px-6 py-3 border-b border-border-subtle bg-bg-1">
        <div class="flex items-center gap-3">
          <h2 class="text-base font-semibold text-fg-0">Workflows</h2>
          <Badge
            tone={
              streamStatus() === "live"
                ? "success"
                : streamStatus() === "lagged"
                  ? "warning"
                  : streamStatus() === "closed"
                    ? "danger"
                    : "neutral"
            }
            dot
          >
            {streamStatus()}
          </Badge>
          <Show when={loading()}>
            <span class="text-xs text-fg-2">loading…</span>
          </Show>
        </div>
        <Button
          size="sm"
          variant="ghost"
          icon="refresh"
          onClick={() => void loadInstances(props.instanceId)}
        >
          Refresh
        </Button>
      </header>

      <Show when={error()}>
        <div class="mx-6 mt-3">
          <Toast tone="danger">
            {error()}
            <button
              type="button"
              class="ml-2 underline cursor-pointer"
              onClick={() => setError(null)}
            >
              dismiss
            </button>
          </Toast>
        </div>
      </Show>

      {/* Running instances strip */}
      <Show when={instances().length > 0}>
        <div class="px-4 py-2 border-b border-border-subtle bg-bg-0/50 overflow-x-auto">
          <div class="flex gap-2 min-w-max">
            <For each={sortedInstances()}>
              {(inst) => (
                <button
                  type="button"
                  onClick={() => setSelectedInstanceId(inst.id)}
                  classList={{
                    "rounded border px-3 py-1.5 text-xs text-left min-w-[12rem]":
                      true,
                    [STATUS_CLASS[inst.status]]: true,
                    "ring-2 ring-accent": selectedInstanceId() === inst.id,
                  }}
                >
                  <div class="font-mono text-[10px] opacity-70">
                    #{inst.id} · {inst.workflow_id}
                  </div>
                  <div>
                    {STATUS_LABEL[inst.status]} · step {inst.current_step + 1}/
                    {inst.steps.length}
                  </div>
                </button>
              )}
            </For>
          </div>
        </div>
      </Show>

      {/* Main three-pane */}
      <div class="flex-1 flex min-h-0">
        {/* Templates sidebar */}
        <aside class="w-56 border-r border-border-subtle overflow-y-auto">
          <div class="px-3 py-2 text-xs uppercase tracking-wide text-fg-3">
            Templates
          </div>
          <Show
            when={templates().length > 0}
            fallback={
              <div class="px-3 py-4 text-sm text-fg-3">
                No workflows declared.
              </div>
            }
          >
            <For each={templates()}>
              {(tpl) => (
                <button
                  type="button"
                  onClick={() => selectTemplate(tpl)}
                  classList={{
                    "w-full text-left px-3 py-2 border-l-2 hover:bg-bg-2/50":
                      true,
                    "border-accent bg-bg-2":
                      selectedTemplateId() === tpl.id,
                    "border-transparent": selectedTemplateId() !== tpl.id,
                  }}
                >
                  <div class="text-sm font-medium text-fg-0">{tpl.id}</div>
                  <Show when={tpl.description}>
                    <div class="text-xs text-fg-2 mt-0.5 line-clamp-2">
                      {tpl.description}
                    </div>
                  </Show>
                  <div class="text-[10px] text-fg-3 mt-1">
                    {tpl.steps.length} step
                    {tpl.steps.length === 1 ? "" : "s"} ·{" "}
                    {tpl.steps.map((s) => s.kind).join(" → ")}
                  </div>
                </button>
              )}
            </For>
          </Show>
        </aside>

        {/* Property form */}
        <section class="w-80 border-r border-border-subtle overflow-y-auto">
          <div class="px-3 py-2 text-xs uppercase tracking-wide text-fg-3">
            Run
          </div>
          <Show
            when={selectedTemplate()}
            fallback={
              <div class="px-3 py-4 text-sm text-fg-3">
                Select a template to start a run.
              </div>
            }
          >
            {(tpl) => (
              <form class="px-3 pb-4 space-y-3" onSubmit={submitForm}>
                <Show when={tpl().description}>
                  <p class="text-sm text-fg-1">{tpl().description}</p>
                </Show>
                <Show
                  when={tpl().properties.length > 0}
                  fallback={
                    <div class="text-xs text-fg-3">
                      No user-visible properties.
                    </div>
                  }
                >
                  <For each={tpl().properties}>
                    {(prop) => (
                      <label class="block text-xs">
                        <span class="block mb-1 text-fg-1">
                          {prop.label ?? prop.id}
                          <Show when={prop.required}>
                            <span class="text-danger-fg ml-1">*</span>
                          </Show>
                          <span class="ml-2 text-fg-3 font-mono">
                            {prop.kind.kind}
                          </span>
                        </span>
                        {renderField(prop, formValues, setFormValues)}
                      </label>
                    )}
                  </For>
                </Show>
                <Show when={formError()}>
                  <div class="text-xs text-danger-fg border border-danger bg-danger-bg px-2 py-1 rounded">
                    {formError()}
                  </div>
                </Show>
                <button
                  type="submit"
                  disabled={submitting()}
                  class="w-full rounded border border-success bg-success-bg hover:bg-success disabled:opacity-50 px-3 py-1.5 text-sm"
                >
                  {submitting() ? "Starting…" : "Run"}
                </button>
              </form>
            )}
          </Show>
        </section>

        {/* Instance detail — or template preview when no instance is selected */}
        <section class="flex-1 min-w-0 overflow-y-auto">
          <Show
            when={selectedInstance()}
            fallback={
              <Show
                when={selectedTemplate()}
                fallback={
                  <div class="px-4 py-6 text-sm text-fg-3">
                    Pick a workflow to preview its flow, or select a
                    running / failed instance to inspect progress.
                  </div>
                }
              >
                {(tpl) => (
                  <div class="p-4 space-y-3">
                    <div class="text-xs uppercase tracking-wide text-fg-3">
                      Flow preview · {tpl().id}
                    </div>
                    <WorkflowDiagram steps={tpl().steps} />
                  </div>
                )}
              </Show>
            }
          >
            {(inst) => (
              <InstanceDetail
                instance={inst()}
                template={templateForInstance(inst())}
                onDismiss={() => dismiss(inst().id)}
              />
            )}
          </Show>
        </section>
      </div>
    </div>
  );
}

/// Input widget for one workflow property. Centralised so the
/// validation-on-change logic stays out of the submit path.
function renderField(
  prop: WireWorkflowTemplate["properties"][number],
  values: () => FormValues,
  setValues: (fn: (prev: FormValues) => FormValues) => void,
): JSX.Element {
  const current = () => values()[prop.id];
  const set = (v: unknown) =>
    setValues((prev) => ({ ...prev, [prop.id]: v }));

  switch (prop.kind.kind) {
    case "text":
      return (
        <input
          type="text"
          class="w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
          value={(current() as string) ?? ""}
          onInput={(e) => set(e.currentTarget.value)}
        />
      );
    case "number":
      return (
        <input
          type="number"
          min={prop.kind.min}
          max={prop.kind.max}
          class="w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
          value={(current() as string) ?? ""}
          onInput={(e) => set(e.currentTarget.value)}
        />
      );
    case "select":
      return (
        <select
          class="w-full rounded border border-border bg-bg-0 px-2 py-1 text-sm"
          value={(current() as string) ?? ""}
          onChange={(e) => set(e.currentTarget.value)}
        >
          <For each={prop.kind.options}>
            {(opt) => <option value={opt}>{opt}</option>}
          </For>
        </select>
      );
    case "multi_select": {
      const options = prop.kind.options;
      return (
        <div class="space-y-1">
          <For each={options}>
            {(opt) => {
              const chosen = () =>
                ((current() as string[] | undefined) ?? []).includes(opt);
              return (
                <label class="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={chosen()}
                    onChange={(e) => {
                      const prev =
                        (current() as string[] | undefined) ?? [];
                      const next = e.currentTarget.checked
                        ? [...prev, opt]
                        : prev.filter((x) => x !== opt);
                      set(next);
                    }}
                  />
                  <span>{opt}</span>
                </label>
              );
            }}
          </For>
        </div>
      );
    }
  }
}

/// Structural flowchart of a workflow, with optional runtime status
/// overlay. Always takes the template's `steps` (source of truth for
/// order + retry targets). When `progress` is supplied, each box
/// takes on the matching `WireStepProgress.status` colour and any
/// retry_count / failure reason is surfaced inline.
///
/// Layout: vertical stack of 52-tall rounded rects, 32px apart.
/// Forward arrows run straight down between consecutive boxes.
/// Retry edges (conditional / check → target) route out the right
/// side of the source box, up the `BACK_CHANNEL` corridor, and back
/// into the right side of the target box as a dashed amber line.
function WorkflowDiagram(props: {
  steps: WireTemplateStep[];
  progress?: WireStepProgress[];
  currentStep?: number;
  class?: string;
}): JSX.Element {
  const BOX_W = 220;
  const BOX_H = 52;
  const V_GAP = 32;
  const PAD = 12;
  const BACK_CHANNEL = 60;
  const TOTAL_W = BOX_W + BACK_CHANNEL + PAD * 2;

  const stepY = (i: number) => PAD + i * (BOX_H + V_GAP);
  const boxX = PAD;

  const totalH = () => {
    const n = props.steps.length;
    if (n === 0) return PAD * 2;
    return PAD * 2 + n * BOX_H + (n - 1) * V_GAP;
  };

  const progressFor = (i: number): WireStepProgress | undefined =>
    props.progress?.[i];

  const statusFor = (i: number): WorkflowStepStatus =>
    progressFor(i)?.status ?? "pending";

  const targetIndex = (s: WireTemplateStep): number | null => {
    if (!s.target) return null;
    const idx = props.steps.findIndex((x) => x.id === s.target);
    return idx === -1 ? null : idx;
  };

  const fillFor: Record<WorkflowStepStatus, string> = {
    pending: "var(--bg-1)",
    running: "var(--accent-bg)",
    succeeded: "var(--success-bg)",
    failed: "var(--danger-bg)",
    retrying: "var(--warning-bg)",
  };
  const strokeFor: Record<WorkflowStepStatus, string> = {
    pending: "var(--border)",
    running: "var(--accent)",
    succeeded: "var(--success)",
    failed: "var(--danger)",
    retrying: "var(--warning)",
  };
  const glyphFillFor: Record<WorkflowStepStatus, string> = {
    pending: "var(--fg-3)",
    running: "var(--accent-fg)",
    succeeded: "var(--success-fg)",
    failed: "var(--danger-fg)",
    retrying: "var(--warning-fg)",
  };

  return (
    <svg
      viewBox={`0 0 ${TOTAL_W} ${totalH()}`}
      class={props.class ?? "w-full max-w-sm h-auto"}
      preserveAspectRatio="xMidYMin meet"
    >
      <defs>
        <marker
          id="wf-arrow-fwd"
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="6"
          markerHeight="6"
          orient="auto-start-reverse"
        >
          <path d="M0,0 L10,5 L0,10 z" fill="var(--fg-2)" />
        </marker>
        <marker
          id="wf-arrow-back"
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="6"
          markerHeight="6"
          orient="auto-start-reverse"
        >
          <path d="M0,0 L10,5 L0,10 z" fill="var(--warning)" />
        </marker>
      </defs>

      {/* Forward arrows between consecutive steps */}
      <For each={props.steps.slice(0, -1)}>
        {(_, i) => {
          const x = boxX + BOX_W / 2;
          const y1 = stepY(i()) + BOX_H;
          const y2 = stepY(i() + 1);
          return (
            <line
              x1={x}
              y1={y1}
              x2={x}
              y2={y2 - 2}
              stroke="var(--fg-2)"
              stroke-width="1.5"
              marker-end="url(#wf-arrow-fwd)"
            />
          );
        }}
      </For>

      {/* Retry edges: right side of conditional/check → right side of target */}
      <For each={props.steps}>
        {(s, i) => {
          const ti = targetIndex(s);
          if (ti === null || ti === i()) return null;
          const fromY = stepY(i()) + BOX_H / 2;
          const toY = stepY(ti) + BOX_H / 2;
          const right = boxX + BOX_W;
          const channelX = right + BACK_CHANNEL - 6;
          const d = `M ${right},${fromY} L ${channelX},${fromY} L ${channelX},${toY} L ${right + 2},${toY}`;
          return (
            <path
              d={d}
              fill="none"
              stroke="var(--warning)"
              stroke-width="1.5"
              stroke-dasharray="4 3"
              marker-end="url(#wf-arrow-back)"
            />
          );
        }}
      </For>

      {/* Step boxes */}
      <For each={props.steps}>
        {(s, i) => {
          const y = stepY(i());
          const status = statusFor(i());
          const prog = progressFor(i());
          const isCurrent = props.currentStep === i();
          const titleText =
            prog?.reason ??
            (s.target ? `retries to "${s.target}" on failure` : "");
          return (
            <g>
              <title>{titleText}</title>
              <rect
                x={boxX}
                y={y}
                width={BOX_W}
                height={BOX_H}
                rx={6}
                fill={fillFor[status]}
                stroke={strokeFor[status]}
                stroke-width={isCurrent ? 2.5 : 1.25}
              />
              <text
                x={boxX + 12}
                y={y + 22}
                fill="var(--fg-0)"
                font-family="var(--font-mono)"
                font-size="12"
                font-weight="600"
              >
                {i() + 1}. {s.id}
              </text>
              <text
                x={boxX + 12}
                y={y + 38}
                fill="var(--fg-2)"
                font-family="var(--font-mono)"
                font-size="10"
              >
                [{s.kind}]
              </text>
              <Show when={prog && prog.retry_count > 0}>
                <text
                  x={boxX + BOX_W - 10}
                  y={y + 22}
                  text-anchor="end"
                  fill="var(--warning-fg)"
                  font-family="var(--font-mono)"
                  font-size="10"
                >
                  ×{prog!.retry_count}
                </text>
              </Show>
              <text
                x={boxX + BOX_W - 10}
                y={y + 38}
                text-anchor="end"
                fill={glyphFillFor[status]}
                font-family="ui-monospace, monospace"
                font-size="14"
              >
                {STEP_GLYPH[status]}
              </text>
            </g>
          );
        }}
      </For>
    </svg>
  );
}

/// Right-pane detail for a single instance: step timeline, vars, tail
/// output. Pulled out so the main component stays readable.
function InstanceDetail(props: {
  instance: WireWorkflowInstance;
  template: WireWorkflowTemplate | undefined;
  onDismiss: () => void;
}): JSX.Element {
  let tailEl: HTMLPreElement | undefined;
  createEffect(() => {
    // Re-scroll on every tail_output change so new lines are visible
    // without the user having to chase the scroll thumb.
    props.instance.tail_output.length;
    if (tailEl) {
      tailEl.scrollTop = tailEl.scrollHeight;
    }
  });

  return (
    <div class="p-4 space-y-4">
      <div class="flex items-center gap-3">
        <span
          classList={{
            "inline-block rounded border px-2 py-0.5 text-xs": true,
            [STATUS_CLASS[props.instance.status]]: true,
          }}
        >
          {STATUS_LABEL[props.instance.status]}
        </span>
        <div class="font-mono text-sm text-fg-0">
          #{props.instance.id} · {props.instance.workflow_id}
        </div>
        <div class="flex-1" />
        <button
          type="button"
          onClick={props.onDismiss}
          disabled={props.instance.status !== "failed"}
          class="rounded border border-border bg-bg-2 px-3 py-1 text-xs hover:bg-bg-3 disabled:opacity-40"
          title={
            props.instance.status === "failed"
              ? "Dismiss this failed instance"
              : "Only failed instances can be dismissed"
          }
        >
          Dismiss
        </button>
      </div>

      <Show when={props.instance.failure_reason}>
        <div class="border border-danger bg-danger-bg px-3 py-2 text-sm text-danger-fg">
          <div class="font-medium mb-1">Failure reason</div>
          <div class="whitespace-pre-wrap">{props.instance.failure_reason}</div>
        </div>
      </Show>

      <div>
        <div class="text-xs uppercase tracking-wide text-fg-3 mb-1">
          Flow
        </div>
        <Show
          when={props.template}
          fallback={
            <div class="text-xs text-fg-3">
              Template metadata unavailable; using instance steps only.
            </div>
          }
        >
          {(tpl) => (
            <WorkflowDiagram
              steps={tpl().steps}
              progress={props.instance.steps}
              currentStep={props.instance.current_step}
            />
          )}
        </Show>
        <Show when={props.instance.steps.some((s) => s.reason)}>
          <ul class="mt-2 space-y-0.5 text-xs font-mono">
            <For
              each={props.instance.steps.filter((s) => s.reason)}
            >
              {(step) => (
                <li>
                  <span class="text-fg-3">{step.id}</span>{" "}
                  <span
                    classList={{
                      "text-danger-fg": step.status === "failed",
                      "text-warning-fg": step.status === "retrying",
                      "text-fg-2":
                        step.status !== "failed" &&
                        step.status !== "retrying",
                    }}
                  >
                    {step.reason}
                  </span>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>

      <Show when={props.instance.vars.length > 0}>
        <div>
          <div class="text-xs uppercase tracking-wide text-fg-3 mb-1">
            Variables
          </div>
          <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-sm font-mono">
            <For each={props.instance.vars}>
              {(v) => (
                <>
                  <dt class="text-fg-2">{v.name}</dt>
                  <dd class="text-fg-0 truncate">{v.display}</dd>
                </>
              )}
            </For>
          </dl>
        </div>
      </Show>

      <div>
        <div class="flex items-center justify-between mb-1">
          <span class="text-xs uppercase tracking-wide text-fg-3">
            Output
          </span>
          <span class="text-[10px] text-fg-3">
            {props.instance.tail_output.length} / {TAIL_CAP} lines
          </span>
        </div>
        <pre
          ref={tailEl}
          class="bg-bg-0 border border-border-subtle rounded p-2 text-xs h-64 overflow-auto whitespace-pre-wrap font-mono text-fg-0"
        >
          <Show
            when={props.instance.tail_output.length > 0}
            fallback={<span class="text-fg-3">(no output yet)</span>}
          >
            {props.instance.tail_output.join("\n")}
          </Show>
        </pre>
      </div>
    </div>
  );
}
