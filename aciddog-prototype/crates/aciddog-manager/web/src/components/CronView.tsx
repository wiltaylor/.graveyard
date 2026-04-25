//! Cron tab — list of the instance's cron jobs + Add/Edit modal +
//! live SSE updates. The manager exposes a one-shot list endpoint and
//! a cron-events SSE feed; we fetch once on open and keep local state
//! in sync with each `WireCronEvent` the server pushes.

import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";

import { api, cronEventsUrl } from "../api";
import type {
  AgentRow,
  CreateCronRequest,
  PatchCronRequest,
  WireCronEvent,
  WireCronJob,
  WireCronPayload,
  WireCronSchedule,
  WireCronTz,
} from "../types";

interface Props {
  instanceId: string;
  agents?: AgentRow[];
}

type PayloadKind = "message" | "command";
type ScheduleKind = "recurring" | "oneoff";

interface FormValues {
  mode: "create" | "edit";
  jobId?: string;
  target: string;
  payloadKind: PayloadKind;
  payloadBody: string;
  scheduleKind: ScheduleKind;
  scheduleText: string;
  tz: WireCronTz;
  catchupOnStartup: boolean;
  enabled: boolean;
}

function formFromJob(j: WireCronJob): FormValues {
  const payloadKind: PayloadKind =
    j.payload.kind === "slash_command" ? "command" : "message";
  const payloadBody =
    j.payload.kind === "slash_command" ? j.payload.command : j.payload.body;
  const scheduleKind: ScheduleKind =
    j.schedule.kind === "one_off" ? "oneoff" : "recurring";
  const scheduleText =
    j.schedule.kind === "one_off"
      ? j.schedule.run_at_local
      : j.schedule.expression;
  return {
    mode: "edit",
    jobId: j.id,
    target: j.target_agent,
    payloadKind,
    payloadBody,
    scheduleKind,
    scheduleText,
    tz: j.timezone,
    catchupOnStartup: j.catchup_on_startup,
    enabled: j.enabled,
  };
}

function emptyForm(): FormValues {
  return {
    mode: "create",
    target: "",
    payloadKind: "message",
    payloadBody: "",
    scheduleKind: "recurring",
    scheduleText: "",
    tz: "local",
    catchupOnStartup: false,
    enabled: true,
  };
}

function formatSchedule(s: WireCronSchedule, tz: WireCronTz): string {
  if (s.kind === "recurring") {
    return `every \`${s.expression}\``;
  }
  return `at ${s.run_at_local} (${tz})`;
}

function formatPayload(p: WireCronPayload): string {
  if (p.kind === "slash_command") return p.command;
  return p.body;
}

function payloadKindLabel(p: WireCronPayload): string {
  return p.kind === "slash_command" ? "cmd" : "msg";
}

function formatNextRun(next: string | null): { rel: string; abs: string } {
  if (!next) return { rel: "—", abs: "" };
  const when = new Date(next);
  if (Number.isNaN(when.getTime())) return { rel: next, abs: next };
  const deltaMs = when.getTime() - Date.now();
  const abs = when.toLocaleString();
  if (deltaMs < 0) {
    const ago = Math.abs(deltaMs);
    if (ago < 60_000) return { rel: "now", abs };
    return { rel: `${Math.floor(ago / 60_000)}m ago`, abs };
  }
  if (deltaMs < 60_000) return { rel: "in <1m", abs };
  if (deltaMs < 3_600_000)
    return { rel: `in ${Math.floor(deltaMs / 60_000)}m`, abs };
  if (deltaMs < 86_400_000)
    return { rel: `in ${Math.floor(deltaMs / 3_600_000)}h`, abs };
  return { rel: `in ${Math.floor(deltaMs / 86_400_000)}d`, abs };
}

export default function CronView(props: Props): JSX.Element {
  const [jobs, setJobs] = createSignal<WireCronJob[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [streamStatus, setStreamStatus] = createSignal<
    "connecting" | "live" | "closed" | "lagged"
  >("connecting");
  const [form, setForm] = createSignal<FormValues | null>(null);
  const [formError, setFormError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);

  // Tick to refresh relative next_run labels once per minute without
  // re-sorting or mutating the underlying job list.
  const [now, setNow] = createSignal(Date.now());
  const tickHandle = window.setInterval(() => setNow(Date.now()), 30_000);
  onCleanup(() => window.clearInterval(tickHandle));

  const loadJobs = async (id: string) => {
    setLoading(true);
    try {
      const resp = await api.listCron(id);
      if (!resp.ok) {
        setError(resp.error ?? "failed to fetch cron jobs");
        setJobs([]);
      } else {
        setError(null);
        setJobs(resp.jobs);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const applyEvent = (ev: WireCronEvent) => {
    switch (ev.kind) {
      case "created": {
        setJobs((xs) => {
          if (xs.some((j) => j.id === ev.job.id)) {
            return xs.map((j) => (j.id === ev.job.id ? ev.job : j));
          }
          return [...xs, ev.job];
        });
        break;
      }
      case "updated": {
        setJobs((xs) =>
          xs.map((j) => (j.id === ev.job.id ? ev.job : j)),
        );
        break;
      }
      case "removed": {
        setJobs((xs) => xs.filter((j) => j.id !== ev.id));
        break;
      }
      case "fired": {
        // Server emits a follow-up `updated` (recurring) or `removed`
        // (one-off) right after a `fired` — the `fired` frame itself
        // is purely informational. Skip it.
        break;
      }
    }
  };

  createEffect(() => {
    const id = props.instanceId;
    setForm(null);
    setFormError(null);
    void loadJobs(id);

    setStreamStatus("connecting");
    const es = new EventSource(cronEventsUrl(id));
    es.onopen = () => setStreamStatus("live");
    es.onmessage = (m) => {
      try {
        const parsed = JSON.parse(m.data) as WireCronEvent;
        applyEvent(parsed);
      } catch {
        // Skip malformed frames.
      }
    };
    es.addEventListener("lagged", () => {
      setStreamStatus("lagged");
      void loadJobs(id);
    });
    es.onerror = () => setStreamStatus("closed");
    onCleanup(() => es.close());
  });

  const openCreate = () => {
    setForm(emptyForm());
    setFormError(null);
  };
  const openEdit = (job: WireCronJob) => {
    setForm(formFromJob(job));
    setFormError(null);
  };
  const closeForm = () => {
    setForm(null);
    setFormError(null);
  };

  const submitForm = async (e: Event) => {
    e.preventDefault();
    const f = form();
    if (!f) return;
    if (f.target.trim() === "") {
      setFormError("target is required");
      return;
    }
    if (f.payloadBody.trim() === "") {
      setFormError("payload is required");
      return;
    }
    if (f.scheduleText.trim() === "") {
      setFormError("schedule is required");
      return;
    }
    if (f.catchupOnStartup && f.scheduleKind !== "oneoff") {
      setFormError("catchup_on_startup only applies to one-off jobs");
      return;
    }
    setSubmitting(true);
    try {
      if (f.mode === "create") {
        const body: CreateCronRequest = {
          target: f.target.trim(),
          kind: f.payloadKind,
          payload: f.payloadBody,
          schedule: f.scheduleText.trim(),
          tz: f.tz,
          oneoff: f.scheduleKind === "oneoff",
          catchup_on_startup: f.catchupOnStartup,
          disabled: !f.enabled,
        };
        const resp = await api.createCron(props.instanceId, body);
        if (!resp.ok) {
          setFormError(resp.error ?? "create failed");
          return;
        }
      } else if (f.jobId !== undefined) {
        const body: PatchCronRequest = {
          target: f.target.trim(),
          kind: f.payloadKind,
          payload: f.payloadBody,
          schedule: f.scheduleText.trim(),
          tz: f.tz,
          oneoff: f.scheduleKind === "oneoff",
          catchup_on_startup: f.catchupOnStartup,
          enabled: f.enabled,
        };
        const resp = await api.patchCron(props.instanceId, f.jobId, body);
        if (!resp.ok) {
          setFormError(resp.error ?? "update failed");
          return;
        }
      }
      closeForm();
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const toggleEnabled = async (job: WireCronJob) => {
    try {
      const resp = await api.patchCron(props.instanceId, job.id, {
        enabled: !job.enabled,
      });
      if (!resp.ok) {
        setError(resp.error ?? "toggle failed");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const remove = async (job: WireCronJob) => {
    if (!window.confirm(`Delete cron job for \`${job.target_agent}\`?`)) return;
    try {
      const resp = await api.deleteCron(props.instanceId, job.id);
      if (!resp.ok) setError(resp.error ?? "delete failed");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const streamBadge = () => {
    const status = streamStatus();
    const style = {
      connecting: "border-amber-700 bg-amber-900/40 text-amber-200",
      live: "border-emerald-700 bg-emerald-900/40 text-emerald-200",
      lagged: "border-amber-700 bg-amber-900/40 text-amber-200",
      closed: "border-red-700 bg-red-900/40 text-red-200",
    }[status];
    const label = {
      connecting: "connecting",
      live: "live",
      lagged: "lagged",
      closed: "offline",
    }[status];
    return (
      <span
        class={`rounded border px-2 py-0.5 text-xs ${style}`}
      >
        {label}
      </span>
    );
  };

  return (
    <div class="flex flex-col h-full overflow-hidden">
      <header class="flex items-center gap-3 border-b border-slate-800 bg-slate-900/40 px-4 py-2">
        <h2 class="text-sm font-semibold">Cron</h2>
        {streamBadge()}
        <span class="text-xs text-slate-500">
          {jobs().length} job{jobs().length === 1 ? "" : "s"}
        </span>
        <div class="flex-1" />
        <button
          type="button"
          onClick={() => void loadJobs(props.instanceId)}
          class="rounded border border-slate-700 bg-slate-900 hover:bg-slate-800 px-3 py-1 text-sm"
        >
          Refresh
        </button>
        <button
          type="button"
          onClick={openCreate}
          class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 px-3 py-1 text-sm"
        >
          + New cron job
        </button>
      </header>

      <Show when={error()}>
        {(msg) => (
          <div class="m-3 rounded border border-red-700 bg-red-900/40 px-3 py-2 text-sm text-red-200">
            {msg()}
          </div>
        )}
      </Show>

      <div class="flex-1 overflow-auto">
        <Show
          when={jobs().length > 0}
          fallback={
            <div class="p-6 text-sm text-slate-500">
              {loading()
                ? "Loading cron jobs…"
                : "No cron jobs yet. Click + New cron job to schedule one."}
            </div>
          }
        >
          <table class="w-full text-sm">
            <thead class="text-xs uppercase text-slate-500 border-b border-slate-800">
              <tr>
                <th class="text-left px-3 py-2 w-8"></th>
                <th class="text-left px-3 py-2">Target</th>
                <th class="text-left px-3 py-2">Kind</th>
                <th class="text-left px-3 py-2">Schedule</th>
                <th class="text-left px-3 py-2">Next run</th>
                <th class="text-left px-3 py-2">Payload</th>
                <th class="text-right px-3 py-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={jobs()}>
                {(job) => {
                  // Capture `now()` so the relative label refreshes
                  // when the interval tick bumps the signal.
                  const nextInfo = () => {
                    void now();
                    return formatNextRun(job.next_run);
                  };
                  return (
                    <tr class="border-b border-slate-900 hover:bg-slate-900/40">
                      <td class="px-3 py-2">
                        <button
                          type="button"
                          onClick={() => void toggleEnabled(job)}
                          title={job.enabled ? "Disable" : "Enable"}
                          class={
                            job.enabled
                              ? "text-emerald-400 hover:text-emerald-200"
                              : "text-slate-500 hover:text-slate-300"
                          }
                        >
                          {job.enabled ? "●" : "○"}
                        </button>
                      </td>
                      <td class="px-3 py-2 font-mono">{job.target_agent}</td>
                      <td class="px-3 py-2 text-slate-400">
                        {payloadKindLabel(job.payload)}
                      </td>
                      <td class="px-3 py-2 font-mono text-slate-300">
                        {formatSchedule(job.schedule, job.timezone)}
                      </td>
                      <td
                        class="px-3 py-2 text-slate-300"
                        title={nextInfo().abs}
                      >
                        {nextInfo().rel}
                      </td>
                      <td class="px-3 py-2 text-slate-400 truncate max-w-xs">
                        {formatPayload(job.payload).slice(0, 80)}
                      </td>
                      <td class="px-3 py-2 text-right whitespace-nowrap">
                        <button
                          type="button"
                          onClick={() => openEdit(job)}
                          class="rounded border border-slate-700 bg-slate-900 hover:bg-slate-800 px-2 py-0.5 text-xs"
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          onClick={() => void remove(job)}
                          class="ml-2 rounded border border-red-800 bg-red-900/40 hover:bg-red-800 px-2 py-0.5 text-xs"
                        >
                          Delete
                        </button>
                      </td>
                    </tr>
                  );
                }}
              </For>
            </tbody>
          </table>
        </Show>
      </div>

      <Show when={form()}>
        {(f) => (
          <CronFormModal
            form={f()}
            error={formError()}
            submitting={submitting()}
            agents={props.agents}
            onChange={(next) => setForm(next)}
            onSubmit={submitForm}
            onClose={closeForm}
          />
        )}
      </Show>
    </div>
  );
}

interface FormProps {
  form: FormValues;
  error: string | null;
  submitting: boolean;
  agents?: AgentRow[];
  onChange: (next: FormValues) => void;
  onSubmit: (e: Event) => void;
  onClose: () => void;
}

function CronFormModal(props: FormProps): JSX.Element {
  const title = props.form.mode === "create" ? "New cron job" : "Edit cron job";
  const update = (patch: Partial<FormValues>) =>
    props.onChange({ ...props.form, ...patch });

  return (
    <div
      class="fixed inset-0 z-50 bg-black/60 flex items-start justify-center overflow-y-auto"
      onClick={props.onClose}
    >
      <div
        class="my-10 w-full max-w-xl rounded border border-slate-700 bg-slate-900 p-4 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header class="flex items-center justify-between mb-3">
          <h2 class="text-base font-semibold">{title}</h2>
          <button
            type="button"
            onClick={props.onClose}
            class="text-slate-400 hover:text-slate-100"
          >
            ✕
          </button>
        </header>

        <form onSubmit={props.onSubmit} class="flex flex-col gap-3">
          <label class="flex flex-col gap-1 text-sm">
            <span class="text-slate-400">Target agent</span>
            <Show
              when={props.agents && props.agents.length > 0}
              fallback={
                <input
                  type="text"
                  value={props.form.target}
                  onInput={(e) =>
                    update({ target: (e.target as HTMLInputElement).value })
                  }
                  class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
                  placeholder="agent id"
                />
              }
            >
              <select
                value={props.form.target}
                onChange={(e) =>
                  update({ target: (e.target as HTMLSelectElement).value })
                }
                class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
              >
                <option value="">(select agent)</option>
                <For each={props.agents}>
                  {(a) => <option value={a.id}>{a.id}</option>}
                </For>
              </select>
            </Show>
          </label>

          <label class="flex flex-col gap-1 text-sm">
            <span class="text-slate-400">Payload kind</span>
            <select
              value={props.form.payloadKind}
              onChange={(e) =>
                update({
                  payloadKind: (e.target as HTMLSelectElement)
                    .value as PayloadKind,
                })
              }
              class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
            >
              <option value="message">message (wrapped as &lt;Message&gt;)</option>
              <option value="command">command (slash command)</option>
            </select>
          </label>

          <label class="flex flex-col gap-1 text-sm">
            <span class="text-slate-400">
              {props.form.payloadKind === "command"
                ? "Slash command (must start with /)"
                : "Message body"}
            </span>
            <textarea
              value={props.form.payloadBody}
              onInput={(e) =>
                update({
                  payloadBody: (e.target as HTMLTextAreaElement).value,
                })
              }
              rows={props.form.payloadKind === "command" ? 1 : 4}
              class="rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono"
            />
          </label>

          <div class="grid grid-cols-2 gap-3">
            <label class="flex flex-col gap-1 text-sm">
              <span class="text-slate-400">Schedule kind</span>
              <select
                value={props.form.scheduleKind}
                onChange={(e) => {
                  const next = (e.target as HTMLSelectElement)
                    .value as ScheduleKind;
                  const patch: Partial<FormValues> = { scheduleKind: next };
                  // Catchup only makes sense for one-off.
                  if (next === "recurring") patch.catchupOnStartup = false;
                  update(patch);
                }}
                class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
              >
                <option value="recurring">recurring</option>
                <option value="oneoff">one-off</option>
              </select>
            </label>
            <label class="flex flex-col gap-1 text-sm">
              <span class="text-slate-400">Timezone</span>
              <select
                value={props.form.tz}
                onChange={(e) =>
                  update({
                    tz: (e.target as HTMLSelectElement).value as WireCronTz,
                  })
                }
                class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
              >
                <option value="local">local</option>
                <option value="utc">utc</option>
              </select>
            </label>
          </div>

          <label class="flex flex-col gap-1 text-sm">
            <span class="text-slate-400">
              {props.form.scheduleKind === "oneoff"
                ? "Wall-clock (YYYY-MM-DDTHH:MM[:SS])"
                : "Cron expression (5-field: `min hour dom mon dow`)"}
            </span>
            <input
              type="text"
              value={props.form.scheduleText}
              onInput={(e) =>
                update({
                  scheduleText: (e.target as HTMLInputElement).value,
                })
              }
              placeholder={
                props.form.scheduleKind === "oneoff"
                  ? "2026-05-01T09:00"
                  : "*/5 * * * *"
              }
              class="rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono"
            />
          </label>

          <div class="flex flex-wrap gap-4 text-sm">
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                checked={props.form.enabled}
                onChange={(e) =>
                  update({ enabled: (e.target as HTMLInputElement).checked })
                }
              />
              <span>Enabled</span>
            </label>
            <label
              class="flex items-center gap-2"
              classList={{
                "opacity-40": props.form.scheduleKind !== "oneoff",
              }}
              title={
                props.form.scheduleKind !== "oneoff"
                  ? "Only applies to one-off jobs"
                  : undefined
              }
            >
              <input
                type="checkbox"
                disabled={props.form.scheduleKind !== "oneoff"}
                checked={props.form.catchupOnStartup}
                onChange={(e) =>
                  update({
                    catchupOnStartup: (e.target as HTMLInputElement).checked,
                  })
                }
              />
              <span>Catchup on startup</span>
            </label>
          </div>

          <Show when={props.error}>
            {(msg) => (
              <div class="rounded border border-red-700 bg-red-900/40 px-3 py-2 text-sm text-red-200">
                {msg()}
              </div>
            )}
          </Show>

          <footer class="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={props.onClose}
              class="rounded border border-slate-700 bg-slate-900 hover:bg-slate-800 px-3 py-1 text-sm"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={props.submitting}
              class="rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
            >
              {props.submitting ? "Saving…" : "Save"}
            </button>
          </footer>
        </form>
      </div>
    </div>
  );
}
