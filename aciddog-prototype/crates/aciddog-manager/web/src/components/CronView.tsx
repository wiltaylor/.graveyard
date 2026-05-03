//! Cron tab — list of the instance's cron jobs + Add/Edit modal +
//! live SSE updates.

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
import {
  Badge,
  Button,
  Checkbox,
  Empty,
  Input,
  Modal,
  Select,
  Textarea,
  Toast,
} from "./ui";

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
  if (s.kind === "recurring") return `every ${s.expression}`;
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
      case "created":
        setJobs((xs) => {
          if (xs.some((j) => j.id === ev.job.id)) {
            return xs.map((j) => (j.id === ev.job.id ? ev.job : j));
          }
          return [...xs, ev.job];
        });
        break;
      case "updated":
        setJobs((xs) => xs.map((j) => (j.id === ev.job.id ? ev.job : j)));
        break;
      case "removed":
        setJobs((xs) => xs.filter((j) => j.id !== ev.id));
        break;
      case "fired":
        break;
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
        // ignore malformed
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
      if (!resp.ok) setError(resp.error ?? "toggle failed");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const remove = async (job: WireCronJob) => {
    if (!window.confirm(`Delete cron job for "${job.target_agent}"?`)) return;
    try {
      const resp = await api.deleteCron(props.instanceId, job.id);
      if (!resp.ok) setError(resp.error ?? "delete failed");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const streamBadgeNode = (): JSX.Element => {
    const status = streamStatus();
    const tone =
      status === "live"
        ? "success"
        : status === "closed"
          ? "danger"
          : "warning";
    const label =
      status === "connecting"
        ? "connecting"
        : status === "live"
          ? "live"
          : status === "lagged"
            ? "lagged"
            : "offline";
    return (
      <Badge tone={tone} dot>
        {label}
      </Badge>
    );
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        overflow: "hidden",
      }}
    >
      <header
        style={{
          display: "flex",
          "align-items": "center",
          gap: "12px",
          "border-bottom": "1px solid var(--border-subtle)",
          background: "var(--bg-1)",
          padding: "10px 24px",
        }}
      >
        <h2
          style={{
            "font-size": "14px",
            "font-weight": "600",
            color: "var(--fg-0)",
            margin: "0",
          }}
        >
          Cron
        </h2>
        {streamBadgeNode()}
        <span style={{ color: "var(--fg-2)", "font-size": "12px" }}>
          {jobs().length} job{jobs().length === 1 ? "" : "s"}
        </span>
        <div style={{ flex: "1" }} />
        <Button
          size="sm"
          variant="secondary"
          icon="refresh"
          onClick={() => void loadJobs(props.instanceId)}
        >
          Refresh
        </Button>
        <Button size="sm" variant="primary" icon="plus" onClick={openCreate}>
          New cron job
        </Button>
      </header>

      <Show when={error()}>
        {(msg) => (
          <div style={{ padding: "12px 24px" }}>
            <Toast tone="danger">{msg()}</Toast>
          </div>
        )}
      </Show>

      <div style={{ flex: "1", overflow: "auto", padding: "0" }}>
        <Show
          when={jobs().length > 0}
          fallback={
            <div style={{ padding: "24px" }}>
              <Empty>
                {loading()
                  ? "Loading cron jobs…"
                  : "No cron jobs yet. Click New cron job to schedule one."}
              </Empty>
            </div>
          }
        >
          <table class="ftable">
            <thead>
              <tr>
                <th style={{ width: "32px" }} />
                <th>Target</th>
                <th>Kind</th>
                <th>Schedule</th>
                <th>Next run</th>
                <th>Payload</th>
                <th style={{ "text-align": "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={jobs()}>
                {(job) => {
                  const nextInfo = () => {
                    void now();
                    return formatNextRun(job.next_run);
                  };
                  return (
                    <tr>
                      <td>
                        <button
                          type="button"
                          onClick={() => void toggleEnabled(job)}
                          title={job.enabled ? "Disable" : "Enable"}
                          style={{
                            background: "transparent",
                            border: "0",
                            cursor: "pointer",
                            color: job.enabled ? "var(--success)" : "var(--fg-3)",
                            "font-size": "14px",
                            padding: "0",
                          }}
                        >
                          {job.enabled ? "●" : "○"}
                        </button>
                      </td>
                      <td class="col-mono">{job.target_agent}</td>
                      <td>
                        <Badge tone="neutral">{payloadKindLabel(job.payload)}</Badge>
                      </td>
                      <td class="col-mono">
                        {formatSchedule(job.schedule, job.timezone)}
                      </td>
                      <td title={nextInfo().abs}>{nextInfo().rel}</td>
                      <td
                        style={{
                          color: "var(--fg-2)",
                          "max-width": "300px",
                          overflow: "hidden",
                          "text-overflow": "ellipsis",
                          "white-space": "nowrap",
                          "font-family": "var(--font-mono)",
                          "font-size": "12px",
                        }}
                      >
                        {formatPayload(job.payload).slice(0, 80)}
                      </td>
                      <td style={{ "text-align": "right", "white-space": "nowrap" }}>
                        <span style={{ display: "inline-flex", gap: "6px" }}>
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => openEdit(job)}
                          >
                            Edit
                          </Button>
                          <Button
                            size="sm"
                            variant="danger"
                            onClick={() => void remove(job)}
                          >
                            Delete
                          </Button>
                        </span>
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

  const submit = (e: Event) => props.onSubmit(e);

  return (
    <Modal
      open={true}
      onClose={props.onClose}
      title={title}
      width={560}
      footer={
        <>
          <Button variant="ghost" onClick={props.onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            disabled={props.submitting}
            onClick={(e) => submit(e as unknown as Event)}
          >
            {props.submitting ? "Saving…" : "Save"}
          </Button>
        </>
      }
    >
      <form
        onSubmit={submit}
        style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
      >
        <Show
          when={props.agents && props.agents.length > 0}
          fallback={
            <Input
              label="Target agent"
              value={props.form.target}
              onInput={(e) => update({ target: e.currentTarget.value })}
              placeholder="agent id"
            />
          }
        >
          <Select
            label="Target agent"
            value={props.form.target}
            onChange={(v) => update({ target: v })}
            options={(props.agents ?? [])
              .filter((a) => a.kind === "agent" || a.kind === "automaton")
              .filter((a) => a.automaton_backend !== "webservice")
              .map((a) => ({
                value: a.id,
                label: a.id,
              }))}
            placeholder="(select agent or automaton)"
          />
        </Show>

        <Select
          label="Payload kind"
          value={props.form.payloadKind}
          onChange={(v) => update({ payloadKind: v as PayloadKind })}
          options={[
            { value: "message", label: "message" },
            { value: "command", label: "command (slash command)" },
          ]}
        />

        <Textarea
          label={
            props.form.payloadKind === "command"
              ? "Slash command (must start with /)"
              : "Message body"
          }
          value={props.form.payloadBody}
          onInput={(e) => update({ payloadBody: e.currentTarget.value })}
          rows={props.form.payloadKind === "command" ? 1 : 4}
          mono
        />

        <div
          style={{ display: "grid", "grid-template-columns": "1fr 1fr", gap: "12px" }}
        >
          <Select
            label="Schedule kind"
            value={props.form.scheduleKind}
            onChange={(v) => {
              const next = v as ScheduleKind;
              const patch: Partial<FormValues> = { scheduleKind: next };
              if (next === "recurring") patch.catchupOnStartup = false;
              update(patch);
            }}
            options={[
              { value: "recurring", label: "recurring" },
              { value: "oneoff", label: "one-off" },
            ]}
          />
          <Select
            label="Timezone"
            value={props.form.tz}
            onChange={(v) => update({ tz: v as WireCronTz })}
            options={[
              { value: "local", label: "local" },
              { value: "utc", label: "utc" },
            ]}
          />
        </div>

        <Input
          label={
            props.form.scheduleKind === "oneoff"
              ? "Wall-clock (YYYY-MM-DDTHH:MM[:SS])"
              : "Cron expression (5-field)"
          }
          value={props.form.scheduleText}
          onInput={(e) => update({ scheduleText: e.currentTarget.value })}
          placeholder={
            props.form.scheduleKind === "oneoff"
              ? "2026-05-01T09:00"
              : "*/5 * * * *"
          }
        />

        <div style={{ display: "flex", "flex-wrap": "wrap", gap: "16px" }}>
          <Checkbox
            checked={props.form.enabled}
            onChange={(v) => update({ enabled: v })}
            label="Enabled"
          />
          <Checkbox
            checked={props.form.catchupOnStartup}
            disabled={props.form.scheduleKind !== "oneoff"}
            onChange={(v) => update({ catchupOnStartup: v })}
            label="Catchup on startup"
            description={
              props.form.scheduleKind !== "oneoff"
                ? "Only applies to one-off jobs"
                : undefined
            }
          />
        </div>

        <Show when={props.error}>
          {(msg) => <Toast tone="danger">{msg()}</Toast>}
        </Show>
      </form>
    </Modal>
  );
}
