import { Show, createSignal, type JSX } from "solid-js";

import type { ManagedDaemonView } from "../../types";
import { Badge, Button, Modal, Spinner, Toast } from "../ui";
import { daemonStateVisual } from "./daemonStatus";

interface Props {
  daemon: ManagedDaemonView;
  selected: boolean;
  onSelect: (id: string) => void;
  onStart: (id: string) => void | Promise<void>;
  onStop: (id: string) => void | Promise<void>;
  onReload: (id: string) => void | Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onOpenInChat: (instanceId: string) => void;
}

/// Render one daemon row.
///
/// Action affordance gating (from the plan / decisions/585d4b15):
///   * Start  — desired === 'stopped' OR observed in {stopped, crashed}
///   * Stop   — desired === 'running' OR observed in {starting, running}
///   * Reload — observed === 'running' (only)
///   * Delete — always; opens confirm Modal.
///   * Open in chat — paired_instance_id !== null AND observed === 'running'.
///
/// Icon-only buttons carry an `aria-label` containing the verb + the
/// daemon name so screen readers + tests (`getByLabelText`) can find
/// them without relying on the icon glyph.
export default function DaemonRow(props: Props): JSX.Element {
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false);
  const [deleteError, setDeleteError] = createSignal<string | null>(null);

  const desiredVisual = () =>
    daemonStateVisual("desired", props.daemon.desired_state);
  const observedVisual = () =>
    daemonStateVisual("observed", props.daemon.observed_state);

  const showStart = () =>
    props.daemon.desired_state === "stopped" ||
    props.daemon.observed_state === "stopped" ||
    props.daemon.observed_state === "crashed";
  const showStop = () =>
    props.daemon.desired_state === "running" ||
    props.daemon.observed_state === "starting" ||
    props.daemon.observed_state === "running";
  const showReload = () => props.daemon.observed_state === "running";
  const showOpenInChat = () =>
    props.daemon.paired_instance_id !== null &&
    props.daemon.observed_state === "running";

  const closeConfirm = () => {
    if (deleting()) return; // don't dismiss mid-delete
    setConfirmOpen(false);
    setDeleteError(null);
  };

  const handleConfirmDelete = async () => {
    setDeleting(true);
    setDeleteError(null);
    try {
      await props.onDelete(props.daemon.id);
      setConfirmOpen(false);
    } catch (err) {
      // 500 (StillAlive) etc — show the verbatim server error in a
      // Toast; leave the modal open so the user can retry.
      setDeleteError(err instanceof Error ? err.message : String(err));
    } finally {
      setDeleting(false);
    }
  };

  return (
    <tr
      data-daemon-id={props.daemon.id}
      onClick={() => props.onSelect(props.daemon.id)}
      style={{
        cursor: "pointer",
        background: props.selected ? "var(--bg-2)" : undefined,
      }}
    >
      <td class="col-mono">{props.daemon.name}</td>
      <td
        class="col-mono"
        style={{ color: "var(--fg-2)", "font-size": "12px" }}
      >
        {props.daemon.work_dir ?? ""}
      </td>
      <td>
        <Badge tone={desiredVisual().tone}>
          {desiredVisual().glyph} {desiredVisual().label}
        </Badge>
      </td>
      <td>
        <Badge tone={observedVisual().tone} dot>
          {observedVisual().glyph} {observedVisual().label}
        </Badge>
      </td>
      <td style={{ "text-align": "right", "white-space": "nowrap" }}>
        <span
          style={{ display: "inline-flex", gap: "6px" }}
          onClick={(e) => e.stopPropagation()}
        >
          <Show when={showStart()}>
            <Button
              size="sm"
              variant="secondary"
              icon="play"
              aria-label={`Start daemon ${props.daemon.name}`}
              onClick={() => void props.onStart(props.daemon.id)}
            >
              Start
            </Button>
          </Show>
          <Show when={showStop()}>
            <Button
              size="sm"
              variant="secondary"
              icon="pause"
              aria-label={`Stop daemon ${props.daemon.name}`}
              onClick={() => void props.onStop(props.daemon.id)}
            >
              Stop
            </Button>
          </Show>
          <Show when={showReload()}>
            <Button
              size="sm"
              variant="secondary"
              icon="refresh"
              aria-label={`Reload daemon ${props.daemon.name}`}
              onClick={() => void props.onReload(props.daemon.id)}
            >
              Reload
            </Button>
          </Show>
          <Show when={showOpenInChat()}>
            <Button
              size="sm"
              variant="ghost"
              icon="arrow-right"
              aria-label={`Open ${props.daemon.name} in chat`}
              onClick={() =>
                props.onOpenInChat(props.daemon.paired_instance_id as string)
              }
            >
              Open in chat
            </Button>
          </Show>
          <Button
            size="sm"
            variant="danger"
            icon="x"
            aria-label={`Delete daemon ${props.daemon.name}`}
            onClick={() => {
              setDeleteError(null);
              setConfirmOpen(true);
            }}
          >
            Delete
          </Button>
        </span>

        <Modal
          open={confirmOpen()}
          onClose={closeConfirm}
          title={`Delete daemon "${props.daemon.name}"?`}
          width={520}
          footer={
            <>
              <Button
                variant="secondary"
                aria-label={`Cancel deleting daemon ${props.daemon.name}`}
                disabled={deleting()}
                ref={(el: HTMLButtonElement | undefined) => {
                  // Autofocus the safe default. Pressing Enter while
                  // focus is here just clicks Cancel — the destructive
                  // action requires an explicit click on the danger
                  // button.
                  if (el) queueMicrotask(() => el.focus());
                }}
                onClick={closeConfirm}
              >
                Cancel
              </Button>
              <Button
                variant="danger"
                icon="x"
                aria-label={`Confirm delete daemon ${props.daemon.name}`}
                disabled={deleting()}
                onClick={() => void handleConfirmDelete()}
              >
                <Show when={deleting()} fallback={<>Delete daemon</>}>
                  <span
                    style={{
                      display: "inline-flex",
                      "align-items": "center",
                      gap: "6px",
                    }}
                  >
                    <Spinner size={12} label="Deleting" /> Deleting…
                  </span>
                </Show>
              </Button>
            </>
          }
        >
          <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
            {/*
              Body copy is verbatim from the plan (decisions/585d4b15
              + plan TODO 8). Stored as template-literal strings so a
              `grep` for the canonical text picks it up in one match
              instead of fighting JSX whitespace collapsing.
            */}
            <p
              data-test="daemon-delete-confirm-p1"
              style={{ margin: "0", color: "var(--fg-1)", "font-size": "13px" }}
            >
              {`This stops the daemon (SIGTERM, 5s grace, SIGKILL fallback) and removes both the managed-daemon row and its paired instance from the manager. Logs and on-disk team data in \`${props.daemon.work_dir}\` are NOT touched.`}
            </p>
            <p
              data-test="daemon-delete-confirm-p2"
              style={{ margin: "0", color: "var(--fg-1)", "font-size": "13px" }}
            >
              This cannot be undone.
            </p>
            <Show when={deleteError()}>
              {(msg) => <Toast tone="danger">{msg()}</Toast>}
            </Show>
          </div>
        </Modal>
      </td>
    </tr>
  );
}
