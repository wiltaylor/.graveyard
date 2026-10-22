import { Show, type JSX } from "solid-js";

import type { ManagedDaemonView } from "../../types";
import { Card, Empty, Toast } from "../ui";
import { daemonStateVisual } from "./daemonStatus";

interface Props {
  selectedDaemon: ManagedDaemonView | null;
  onOpenInChat: (instanceId: string) => void;
}

function formatDate(iso: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

const FIELD_LABEL: JSX.CSSProperties = {
  "font-size": "11px",
  "text-transform": "uppercase",
  "letter-spacing": "0.06em",
  color: "var(--fg-3)",
  margin: "0 0 4px",
};

const FIELD_VALUE: JSX.CSSProperties = {
  margin: "0",
  "font-family": "var(--font-mono)",
  "font-size": "12px",
  color: "var(--fg-1)",
  "word-break": "break-all",
};

/// Right-hand pane (or below the table on narrow viewports — the parent
/// DaemonsView lays this out with `max-md:flex-col`). Renders the
/// joined paired-instance row info + last_error if any. Uses the same
/// daemonStateVisual helper as the table for the observed-state pill,
/// per the "every Badge tone goes through the helper" precedent
/// (decisions/9de9cdf5).
export default function DaemonDetailPane(props: Props): JSX.Element {
  return (
    <Card title="Daemon details">
      <Show
        when={props.selectedDaemon}
        fallback={<Empty>Select a daemon to see details.</Empty>}
      >
        {(d) => {
          const observed = () =>
            daemonStateVisual("observed", d().observed_state);
          const canOpenChat = () =>
            d().paired_instance_id !== null && d().observed_state === "running";
          return (
            <div
              style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
            >
              <div>
                <h4
                  style={{
                    margin: "0",
                    "font-size": "16px",
                    "font-weight": "600",
                    color: "var(--fg-0)",
                  }}
                >
                  {d().name}
                </h4>
                <p
                  style={{
                    margin: "4px 0 0",
                    color: "var(--fg-2)",
                    "font-size": "12px",
                  }}
                >
                  {observed().glyph} {observed().label}
                </p>
              </div>

              <div>
                <p style={FIELD_LABEL}>id</p>
                <p style={FIELD_VALUE}>{d().id}</p>
              </div>

              <div>
                <p style={FIELD_LABEL}>work_dir</p>
                <p style={FIELD_VALUE}>{d().work_dir ?? "—"}</p>
              </div>

              <div>
                <p style={FIELD_LABEL}>socket_path</p>
                <p style={FIELD_VALUE}>{d().socket_path}</p>
              </div>

              <div>
                <p style={FIELD_LABEL}>paired_instance_id</p>
                <Show
                  when={d().paired_instance_id}
                  fallback={
                    <p style={{ ...FIELD_VALUE, color: "var(--fg-3)" }}>
                      (not yet paired)
                    </p>
                  }
                >
                  {(instanceId) => (
                    <Show
                      when={canOpenChat()}
                      fallback={<p style={FIELD_VALUE}>{instanceId()}</p>}
                    >
                      <button
                        type="button"
                        aria-label={`Open paired instance ${instanceId()} in chat`}
                        onClick={() => props.onOpenInChat(instanceId())}
                        style={{
                          ...FIELD_VALUE,
                          background: "transparent",
                          border: "0",
                          color: "var(--accent)",
                          cursor: "pointer",
                          padding: "0",
                          "text-decoration": "underline",
                          "text-align": "left",
                        }}
                      >
                        {instanceId()} →
                      </button>
                    </Show>
                  )}
                </Show>
              </div>

              <div>
                <p style={FIELD_LABEL}>created_at</p>
                <p style={FIELD_VALUE}>{formatDate(d().created_at)}</p>
              </div>

              <Show when={d().last_error}>
                {(err) => (
                  <div>
                    <p style={FIELD_LABEL}>last_error</p>
                    <Toast tone="danger">{err()}</Toast>
                  </div>
                )}
              </Show>
            </div>
          );
        }}
      </Show>
    </Card>
  );
}
