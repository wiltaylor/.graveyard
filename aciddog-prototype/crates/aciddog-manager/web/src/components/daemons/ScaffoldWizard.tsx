import { Show, createSignal, type JSX } from "solid-js";

import { api } from "../../api";
import type {
  CreateManagedDaemonRequest,
  ManagedDaemonView,
} from "../../types";
import {
  Button,
  Checkbox,
  Input,
  Modal,
  Select,
  Spinner,
  Toast,
} from "../ui";

interface Props {
  open: boolean;
  onClose: () => void;
  onCreated: (created: ManagedDaemonView) => void;
}

/// Defensive client-side name regex. Mirrors the conventional aciddog
/// id rule until sibling 580b77a1 (scaffolding endpoint) lands and pins
/// the canonical regex on the server. The server is the source of
/// truth — this regex only catches obviously-bad input before we burn
/// a network round-trip on it.
const NAME_REGEX = /^[a-z0-9][a-z0-9_-]{0,62}$/;

const TEMPLATE_HELP =
  "Template catalog is not yet wired; the default scaffold runs `aciddog init` in a fresh work_dir.";

const IDLE_HELP = "Leave blank for never. Minimum 5 seconds.";

export default function ScaffoldWizard(props: Props): JSX.Element {
  const [name, setName] = createSignal("");
  const [template, setTemplate] = createSignal<string>("default");
  const [startOnBoot, setStartOnBoot] = createSignal(true);
  const [idleTimeout, setIdleTimeout] = createSignal(""); // string for the input
  const [startOnDemand, setStartOnDemand] = createSignal(false);

  const [submitting, setSubmitting] = createSignal(false);
  const [nameFieldError, setNameFieldError] = createSignal<string | null>(null);
  const [topError, setTopError] = createSignal<string | null>(null);

  let nameInputRef: HTMLInputElement | undefined;

  const reset = () => {
    setName("");
    setTemplate("default");
    setStartOnBoot(true);
    setIdleTimeout("");
    setStartOnDemand(false);
    setSubmitting(false);
    setNameFieldError(null);
    setTopError(null);
  };

  const close = () => {
    if (submitting()) return;
    reset();
    props.onClose();
  };

  const validate = (): { ok: true; req: CreateManagedDaemonRequest } | { ok: false } => {
    setNameFieldError(null);
    setTopError(null);

    const trimmed = name().trim();
    if (trimmed === "") {
      setNameFieldError("name is required");
      return { ok: false };
    }
    if (!NAME_REGEX.test(trimmed)) {
      setNameFieldError(
        "name must start with a lowercase letter or digit and contain only [a-z0-9_-] (max 63 chars)",
      );
      return { ok: false };
    }

    const idleStr = idleTimeout().trim();
    let idleSecs: number | undefined;
    if (idleStr !== "") {
      const parsed = Number(idleStr);
      if (!Number.isFinite(parsed) || !Number.isInteger(parsed) || parsed < 5) {
        setTopError("idle_timeout_secs must be an integer >= 5");
        return { ok: false };
      }
      idleSecs = parsed;
    }

    const req: CreateManagedDaemonRequest = {
      name: trimmed,
      template: template() || undefined,
      start_on_manager_boot: startOnBoot(),
      start_on_demand: startOnDemand(),
    };
    if (idleSecs !== undefined) req.idle_timeout_secs = idleSecs;
    return { ok: true, req };
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (submitting()) return;

    const v = validate();
    if (!v.ok) return;

    setSubmitting(true);
    try {
      const created = await api.createManagedDaemon(v.req);
      reset();
      props.onCreated(created);
      props.onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      // Heuristic: 409 conflict surfaces "already exists" / "name" /
      // "conflict" — render under the name field so the user can fix
      // input in place. Other errors land in a top-of-modal Toast.
      // The server returns the verbatim error message via request<T>'s
      // `throw new Error(error_string)` path.
      const looksLikeNameConflict =
        /already exists|conflict|name/i.test(msg) ||
        msg.toLowerCase().includes("name");
      if (looksLikeNameConflict) {
        setNameFieldError(msg);
        // Refocus name field but DO NOT clear input.
        queueMicrotask(() => nameInputRef?.focus());
      } else {
        setTopError(msg);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      open={props.open}
      onClose={close}
      title="Scaffold daemon"
      width={520}
      footer={
        <>
          <Button
            variant="secondary"
            disabled={submitting()}
            onClick={close}
          >
            Cancel
          </Button>
          <Button
            variant="primary"
            icon="plus"
            disabled={submitting()}
            onClick={(e) => void handleSubmit(e as unknown as Event)}
          >
            <Show when={submitting()} fallback={<>Scaffold daemon</>}>
              <span
                style={{
                  display: "inline-flex",
                  "align-items": "center",
                  gap: "6px",
                }}
              >
                <Spinner size={12} label="Scaffolding" /> Scaffolding…
              </span>
            </Show>
          </Button>
        </>
      }
    >
      <form
        onSubmit={(e) => void handleSubmit(e)}
        style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
      >
        <Show when={topError()}>
          {(msg) => <Toast tone="danger">{msg()}</Toast>}
        </Show>

        <Input
          label="Name"
          ref={(el: HTMLInputElement) => (nameInputRef = el)}
          autofocus
          required
          value={name()}
          onInput={(e) => {
            setName(e.currentTarget.value);
            if (nameFieldError()) setNameFieldError(null);
          }}
          placeholder="my-daemon"
          error={nameFieldError() ?? undefined}
        />

        <Select
          label="Template"
          value={template()}
          onChange={(v) => setTemplate(v)}
          options={[{ value: "default", label: "default" }]}
          help={TEMPLATE_HELP}
        />

        <Input
          label="Idle timeout (seconds)"
          type="number"
          min={5}
          value={idleTimeout()}
          onInput={(e) => setIdleTimeout(e.currentTarget.value)}
          help={IDLE_HELP}
          placeholder="(optional)"
        />

        <Checkbox
          checked={startOnBoot()}
          onChange={setStartOnBoot}
          label="Start on manager boot"
          description="When the manager process restarts, this daemon is launched automatically."
        />

        <Checkbox
          checked={startOnDemand()}
          onChange={setStartOnDemand}
          label="Start on demand"
          description="Boot the daemon the first time a request is routed to it."
        />
      </form>
    </Modal>
  );
}
