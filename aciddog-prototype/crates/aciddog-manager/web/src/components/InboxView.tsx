import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";

import { api, instanceEventsUrl } from "../api";
import type { AgentRow, WireInboxMessage } from "../types";
import {
  Badge,
  Button,
  Card,
  Empty,
  Eyebrow,
  Select,
  Textarea,
  Toast,
} from "./ui";

interface Props {
  instanceId: string;
  agents: AgentRow[];
}

type RowMode =
  | { kind: "collapsed" }
  | { kind: "reply"; body: string }
  | { kind: "forward"; to: string; body: string };

export default function InboxView(props: Props): JSX.Element {
  const [messages, setMessages] = createSignal<WireInboxMessage[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [status, setStatus] = createSignal<"connecting" | "live" | "closed">(
    "connecting",
  );

  const [rowModes, setRowModes] = createSignal<Record<number, RowMode>>({});
  const [busy, setBusy] = createSignal<number | null>(null);

  const loadInbox = async () => {
    setError(null);
    try {
      const resp = await api.listInbox(props.instanceId);
      if (!resp.ok) {
        setError(resp.error ?? "remote returned no data");
        setMessages([]);
      } else {
        setMessages(resp.messages);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  createEffect(() => {
    const id = props.instanceId;
    setMessages([]);
    setRowModes({});
    setStatus("connecting");
    void loadInbox();

    const es = new EventSource(instanceEventsUrl(id));
    es.onopen = () => setStatus("live");
    es.onmessage = (ev) => {
      try {
        const payload = JSON.parse(ev.data) as { kind?: string };
        if (payload.kind === "user_message") {
          void loadInbox();
        }
      } catch {
        // ignore malformed frames
      }
    };
    es.onerror = () => setStatus("closed");
    onCleanup(() => es.close());
  });

  const forwardCandidates = () =>
    props.agents.filter((a) => a.kind === "agent" || a.kind === "automaton");

  const openReply = (idx: number) => {
    setRowModes({ ...rowModes(), [idx]: { kind: "reply", body: "" } });
  };

  const openForward = (idx: number) => {
    const first = forwardCandidates()[0]?.id ?? "";
    setRowModes({
      ...rowModes(),
      [idx]: { kind: "forward", to: first, body: "" },
    });
  };

  const closeRow = (idx: number) => {
    const next = { ...rowModes() };
    delete next[idx];
    setRowModes(next);
  };

  const submitReply = async (idx: number) => {
    const mode = rowModes()[idx];
    if (!mode || mode.kind !== "reply") return;
    if (mode.body.trim() === "") return;
    setBusy(idx);
    setError(null);
    try {
      const resp = await api.replyInbox(props.instanceId, idx, mode.body);
      if (!resp.ok) {
        setError(resp.error ?? "reply failed");
      } else {
        closeRow(idx);
        await loadInbox();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  const submitForward = async (idx: number) => {
    const mode = rowModes()[idx];
    if (!mode || mode.kind !== "forward") return;
    if (mode.to === "" || mode.body.trim() === "") return;
    setBusy(idx);
    setError(null);
    try {
      const resp = await api.forwardInbox(
        props.instanceId,
        idx,
        mode.to,
        mode.body,
      );
      if (!resp.ok) {
        setError(resp.error ?? "forward failed");
      } else {
        closeRow(idx);
        await loadInbox();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  const submitDelete = async (idx: number) => {
    if (!window.confirm("Delete this inbox message?")) return;
    setBusy(idx);
    setError(null);
    try {
      const resp = await api.deleteInboxMessage(props.instanceId, idx);
      if (!resp.ok) {
        setError(resp.error ?? "delete failed");
      } else {
        await loadInbox();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      style={{
        height: "100%",
        overflow: "auto",
        padding: "20px 24px",
        display: "flex",
        "flex-direction": "column",
        gap: "16px",
      }}
    >
      <header
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
        }}
      >
        <h2
          style={{
            "font-size": "22px",
            "font-weight": "600",
            color: "var(--fg-0)",
            "letter-spacing": "-0.01em",
            margin: "0",
          }}
        >
          Inbox
        </h2>
        <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
          <StreamBadge status={status()} />
          <Button variant="ghost" size="sm" icon="refresh" onClick={loadInbox}>
            Refresh
          </Button>
        </div>
      </header>

      <Show when={error()}>
        <Toast tone="danger">{error()}</Toast>
      </Show>

      <Show
        when={messages().length > 0}
        fallback={
          <Empty>
            {error() ? "inbox unavailable." : "No messages in inbox."}
          </Empty>
        }
      >
        <For each={messages()}>
          {(msg, idx) => (
            <InboxRow
              msg={msg}
              index={idx()}
              mode={rowModes()[idx()]}
              busy={busy() === idx()}
              forwardCandidates={forwardCandidates()}
              onReplyClick={() => openReply(idx())}
              onForwardClick={() => openForward(idx())}
              onDelete={() => submitDelete(idx())}
              onCancel={() => closeRow(idx())}
              onReplyChange={(body) =>
                setRowModes({
                  ...rowModes(),
                  [idx()]: { kind: "reply", body },
                })
              }
              onForwardChange={(patch) =>
                setRowModes({
                  ...rowModes(),
                  [idx()]: {
                    kind: "forward",
                    to:
                      patch.to ??
                      (rowModes()[idx()] as { to: string } | undefined)?.to ??
                      "",
                    body:
                      patch.body ??
                      (rowModes()[idx()] as { body: string } | undefined)?.body ??
                      "",
                  },
                })
              }
              onSubmitReply={() => submitReply(idx())}
              onSubmitForward={() => submitForward(idx())}
            />
          )}
        </For>
      </Show>
    </div>
  );
}

function StreamBadge(props: {
  status: "connecting" | "live" | "closed";
}): JSX.Element {
  const tone = () =>
    props.status === "live"
      ? "success"
      : props.status === "closed"
        ? "danger"
        : "neutral";
  return (
    <Badge tone={tone()} dot>
      {props.status}
    </Badge>
  );
}

function relativeTime(unixSecs: number): string {
  const deltaMs = Date.now() - unixSecs * 1000;
  const mins = Math.floor(deltaMs / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

interface RowProps {
  msg: WireInboxMessage;
  index: number;
  mode: RowMode | undefined;
  busy: boolean;
  forwardCandidates: AgentRow[];
  onReplyClick: () => void;
  onForwardClick: () => void;
  onDelete: () => void;
  onCancel: () => void;
  onReplyChange: (body: string) => void;
  onForwardChange: (patch: { to?: string; body?: string }) => void;
  onSubmitReply: () => void;
  onSubmitForward: () => void;
}

function InboxRow(props: RowProps): JSX.Element {
  const mode = () => props.mode ?? { kind: "collapsed" as const };

  return (
    <Card
      title={
        <span style={{ display: "inline-flex", "align-items": "baseline", gap: "8px" }}>
          <span>{props.msg.from}</span>
          <span style={{ "font-size": "11px", color: "var(--fg-2)" }}>
            {relativeTime(props.msg.received_at)}
          </span>
        </span>
      }
      action={
        <span style={{ display: "inline-flex", gap: "6px" }}>
          <Button
            size="sm"
            variant="ghost"
            disabled={props.busy}
            onClick={props.onReplyClick}
          >
            Reply
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={props.busy}
            onClick={props.onForwardClick}
          >
            Forward
          </Button>
          <Button
            size="sm"
            variant="danger"
            disabled={props.busy}
            onClick={props.onDelete}
          >
            Delete
          </Button>
        </span>
      }
      padded={false}
    >
      <pre
        style={{
          "white-space": "pre-wrap",
          "word-break": "break-word",
          padding: "12px 16px",
          "font-family": "var(--font-mono)",
          "font-size": "13px",
          color: "var(--fg-0)",
          margin: "0",
          background: "transparent",
          border: "0",
        }}
      >
        {props.msg.body}
      </pre>
      <Show when={mode().kind === "reply"}>
        {(() => {
          const body = () => (props.mode as { body: string }).body;
          return (
            <div
              style={{
                "border-top": "1px solid var(--border-subtle)",
                background: "var(--bg-2)",
                padding: "12px 16px",
                display: "flex",
                "flex-direction": "column",
                gap: "10px",
              }}
            >
              <Eyebrow>
                Replying to <span style={{ "font-family": "var(--font-mono)" }}>{props.msg.from}</span>
              </Eyebrow>
              <Textarea
                rows={3}
                value={body()}
                onInput={(e) => props.onReplyChange(e.currentTarget.value)}
                placeholder="Reply body"
              />
              <div style={{ display: "flex", "justify-content": "flex-end", gap: "6px" }}>
                <Button size="sm" variant="ghost" onClick={props.onCancel}>
                  Cancel
                </Button>
                <Button
                  size="sm"
                  variant="primary"
                  onClick={props.onSubmitReply}
                  disabled={props.busy || body().trim() === ""}
                >
                  {props.busy ? "Sending…" : "Send"}
                </Button>
              </div>
            </div>
          );
        })()}
      </Show>
      <Show when={mode().kind === "forward"}>
        {(() => {
          const m = () => props.mode as { to: string; body: string };
          return (
            <div
              style={{
                "border-top": "1px solid var(--border-subtle)",
                background: "var(--bg-2)",
                padding: "12px 16px",
                display: "flex",
                "flex-direction": "column",
                gap: "10px",
              }}
            >
              <Select
                label="Forward to"
                value={m().to}
                onChange={(v) => props.onForwardChange({ to: v })}
                options={props.forwardCandidates.map((a) => ({
                  value: a.id,
                  label: `${a.id} (${a.kind})`,
                }))}
              />
              <pre
                style={{
                  "white-space": "pre-wrap",
                  "word-break": "break-word",
                  background: "var(--bg-1)",
                  border: "1px solid var(--border-subtle)",
                  "border-radius": "4px",
                  padding: "8px 10px",
                  "font-size": "12px",
                  color: "var(--fg-2)",
                  "max-height": "120px",
                  overflow: "auto",
                  margin: "0",
                }}
              >
                {props.msg.body}
              </pre>
              <Textarea
                rows={3}
                value={m().body}
                onInput={(e) => props.onForwardChange({ body: e.currentTarget.value })}
                placeholder="Add a note (original attached above)."
              />
              <div style={{ display: "flex", "justify-content": "flex-end", gap: "6px" }}>
                <Button size="sm" variant="ghost" onClick={props.onCancel}>
                  Cancel
                </Button>
                <Button
                  size="sm"
                  variant="primary"
                  onClick={props.onSubmitForward}
                  disabled={props.busy || m().body.trim() === "" || m().to === ""}
                >
                  {props.busy ? "Forwarding…" : "Forward"}
                </Button>
              </div>
            </div>
          );
        })()}
      </Show>
    </Card>
  );
}
