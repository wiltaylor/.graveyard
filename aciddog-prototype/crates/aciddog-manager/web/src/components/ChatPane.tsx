import {
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";
import DOMPurify from "dompurify";
import { marked } from "marked";

import { api, chatEventsUrl } from "../api";
import type { AgentStatusEvent, WireChatBlock, ChatBlockEvent } from "../types";
import {
  Badge,
  Button,
  Chat,
  ChatComposer,
  Terminal,
  Toast,
  type ChatItem,
  type EventBlockProps,
  type EventKind,
  type IconName,
  type TerminalLine,
} from "./ui";
import Spinner from "./ui/Spinner";

interface Props {
  instanceId: string;
  agent: string;
  onClose: () => void;
}

/// Stream status surfaced in the chat header. Distinct from the "history
/// load" error because a lagged event is recoverable (the stream keeps
/// going), while a history fetch error is terminal.
type StreamStatus = "connecting" | "live" | "lagged" | "closed";

export default function ChatPane(props: Props): JSX.Element {
  const [blocks, setBlocks] = createSignal<WireChatBlock[]>([]);
  const [historyError, setHistoryError] = createSignal<string | null>(null);
  const [streamStatus, setStreamStatus] =
    createSignal<StreamStatus>("connecting");
  const [droppedCount, setDroppedCount] = createSignal(0);
  const [input, setInput] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [sendError, setSendError] = createSignal<string | null>(null);
  // True while the daemon reports this agent as `working` — drives the
  // animated `thinking…` indicator pinned above the composer. Toggled
  // off on every effect re-init so a stale `working` from a prior
  // pane (instance/agent change) cannot leak forward visually.
  const [isWorking, setIsWorking] = createSignal(false);

  const loadHistory = async () => {
    setHistoryError(null);
    try {
      const resp = await api.chatHistory(props.instanceId, props.agent);
      if (!resp.ok) {
        setHistoryError(resp.error ?? "remote returned no data");
        setBlocks([]);
        return;
      }
      setBlocks(resp.blocks);
    } catch (err) {
      setHistoryError(err instanceof Error ? err.message : String(err));
    }
  };

  // (Re)load history and open the SSE stream whenever the target
  // instance or agent changes. Solid's `onCleanup` inside a
  // `createEffect` runs before the next effect invocation and on
  // unmount, so EventSource handles are always torn down cleanly.
  createEffect(() => {
    const instId = props.instanceId;
    const agentId = props.agent;

    // Closure-local race shield. The SSE subscription and the history
    // fetch start in the same tick, so a live `ChatBlockEvent` can
    // land BEFORE `chatHistory` resolves; if we apply it then, the
    // upcoming `setBlocks(resp.blocks)` will clobber it. We instead
    // buffer live events while history is in flight and drain them
    // through the same apply path once the canonical state lands.
    // Intentionally NOT `createSignal` — promoting either binding to
    // a signal would re-trigger the effect, re-creating the race.
    let historyReady = false;
    const pendingEvents: ChatBlockEvent[] = [];

    // Reset BEFORE the new EventSource subscribes so a stale
    // `working` from the previously-viewed pane can't leak across
    // the boundary if the new daemon never sends a `status` event.
    setIsWorking(false);
    setBlocks([]);
    setDroppedCount(0);
    setStreamStatus("connecting");

    // Apply a single ChatBlockEvent to the blocks signal. `seq` is
    // the block's zero-based index in the agent chat at emit time
    // (see `aciddog::wire_chat` doc-comment), so:
    //   - `seq < length`  → mutation in place (e.g. tool pending
    //                       → completed). Preserves prior behaviour.
    //   - `seq === length`→ append into next slot (steady-state).
    //   - `seq > length`  → real gap. Loud-warn (loud-failure ethos),
    //                       buffer the event, and trigger a re-sync.
    // Side effects (warn / push / reload) happen OUTSIDE the
    // setBlocks updater so the updater stays pure.
    const applyEvent = (parsed: ChatBlockEvent): void => {
      let gap = false;
      let curLength = 0;
      setBlocks((prev) => {
        const seq = Number(parsed.seq);
        if (Number.isFinite(seq) && seq < prev.length) {
          const next = prev.slice();
          next[seq] = parsed.block;
          return next;
        }
        if (Number.isFinite(seq) && seq === prev.length) {
          return [...prev, parsed.block];
        }
        gap = true;
        curLength = prev.length;
        return prev;
      });
      if (gap) {
        console.warn("[ChatPane] gap detected", {
          agent: agentId,
          seq: parsed.seq,
          length: curLength,
        });
        pendingEvents.push(parsed);
        void reload();
      }
    };

    // Closure-local re-fetch + drain. Used on initial mount, on a
    // `lagged` re-sync, and on a detected gap. The outer
    // `loadHistory` (still wired to the Refresh button) is a simpler
    // user-initiated reload that doesn't participate in the buffer
    // path; live events arriving during that window are governed by
    // `historyReady` here through the SSE handlers.
    const reload = async (): Promise<void> => {
      historyReady = false;
      setHistoryError(null);
      try {
        const resp = await api.chatHistory(instId, agentId);
        if (!resp.ok) {
          setHistoryError(resp.error ?? "remote returned no data");
          setBlocks([]);
        } else {
          setBlocks(resp.blocks);
          // Drain in arrival order so a string of buffered events
          // applies left-to-right (replace-or-append semantics).
          const drained = pendingEvents.splice(0);
          for (const ev of drained) applyEvent(ev);
        }
      } catch (err) {
        setHistoryError(err instanceof Error ? err.message : String(err));
      } finally {
        // Always flip ready back on — even on error — so the SSE
        // stream isn't permanently buffered into oblivion.
        historyReady = true;
      }
    };

    void reload();

    const es = new EventSource(chatEventsUrl(instId, agentId));
    es.onopen = () => setStreamStatus("live");
    es.onmessage = (ev) => {
      let parsed: ChatBlockEvent;
      try {
        parsed = JSON.parse(ev.data) as ChatBlockEvent;
      } catch (err) {
        console.warn("[ChatPane] malformed chat block event", err);
        return;
      }
      if (!historyReady) {
        pendingEvents.push(parsed);
        return;
      }
      applyEvent(parsed);
    };
    es.addEventListener("status", (ev) => {
      try {
        const parsed = JSON.parse((ev as MessageEvent).data) as AgentStatusEvent;
        // Defensive: the daemon already filters by agent server-side,
        // but the upstream broadcaster fans out to every subscriber so
        // mid-stream agent re-targets are possible. Drop foreign rows.
        if (parsed.agent !== agentId) return;
        setIsWorking(parsed.status === "working");
      } catch {
        // Malformed event — ignore (mirror onmessage's defensive shape).
      }
    });
    es.addEventListener("lagged", (ev) => {
      setStreamStatus("lagged");
      try {
        const parsed = JSON.parse((ev as MessageEvent).data) as {
          dropped?: number;
        };
        if (typeof parsed.dropped === "number") {
          setDroppedCount((n) => n + parsed.dropped!);
        }
      } catch {
        // Ignore payload parse errors — the count stays as-is.
      }
      // Re-sync. Anything currently buffered is stale by definition
      // — the upcoming `chatHistory` snapshot is the canonical state
      // — so clear pendingEvents before flipping `historyReady` off.
      historyReady = false;
      pendingEvents.length = 0;
      void reload();
    });
    es.onerror = () => {
      setStreamStatus("closed");
      // EventSource auto-reconnects; `onopen` will flip us back to live.
    };
    onCleanup(() => es.close());
  });

  const submit = async () => {
    const body = input().trim();
    if (!body) return;
    setSending(true);
    setSendError(null);
    try {
      const resp = await api.sendAsUser(props.instanceId, props.agent, body);
      if (!resp.ok) {
        setSendError(resp.error ?? "send failed");
      } else {
        setInput("");
      }
    } catch (err) {
      setSendError(err instanceof Error ? err.message : String(err));
    } finally {
      setSending(false);
    }
  };

  const items = createMemo(() => blocksToChatItems(blocks(), props.agent));

  // The seeded `agent_info` block carries the session hash and is
  // always at `blocks[0]` (the backend updates it in place via the
  // SSE stream when /compact or /reset rolls the hash).
  const sessionHash = createMemo<string | undefined>(() => {
    const head = blocks()[0];
    if (head && head.kind === "agent_info") {
      const h = head.session_hash;
      return h && h.length > 0 ? h : undefined;
    }
    return undefined;
  });

  return (
    <div class="flex flex-col h-[calc(100vh-3.5rem)]">
      <Show when={historyError()}>
        <div class="mx-6 mt-3">
          <Toast tone="danger">{historyError()}</Toast>
        </div>
      </Show>
      <div class="flex-1 min-h-0 p-3">
        <Chat
          height="100%"
          title={
            <>
              <Button
                size="sm"
                variant="ghost"
                icon="arrow-right"
                onClick={props.onClose}
              >
                Back
              </Button>
              <span>{props.agent}</span>
              <Show when={sessionHash()}>
                {(hash) => (
                  <span class="font-mono text-xs text-fg-3">[{hash()}]</span>
                )}
              </Show>
            </>
          }
          meta={
            <span class="inline-flex items-center gap-2">
              <StreamStatusBadge
                status={streamStatus()}
                dropped={droppedCount()}
              />
              <Button
                size="sm"
                variant="ghost"
                icon="refresh"
                onClick={loadHistory}
              >
                Refresh
              </Button>
            </span>
          }
          messages={items()}
          emptyMessage={
            historyError() ? "history unavailable." : "(no messages yet)"
          }
          footer={
            <>
              <Show when={isWorking()}>
                <div
                  role="status"
                  aria-live="polite"
                  class="flex items-center gap-2 px-3 py-1 text-fg-3 italic"
                >
                  <Spinner size={14} />
                  <span>thinking…</span>
                </div>
              </Show>
              <ChatComposer
                value={input()}
                onInput={setInput}
                onSubmit={submit}
                sending={sending()}
                error={sendError()}
                placeholder="Send a message as the human operator. Enter to send, Shift+Enter for newline."
              />
            </>
          }
        />
      </div>
    </div>
  );
}

function StreamStatusBadge(props: {
  status: StreamStatus;
  dropped: number;
}): JSX.Element {
  const tone = () =>
    props.status === "live"
      ? "success"
      : props.status === "lagged"
        ? "warning"
        : props.status === "closed"
          ? "danger"
          : "neutral";
  const label = () =>
    props.status === "lagged" && props.dropped > 0
      ? `lagged · ${props.dropped} dropped`
      : props.status;
  return (
    <Badge tone={tone()} dot>
      {label()}
    </Badge>
  );
}

// ---------------------------------------------------------------------------
// Block → ChatItem mapping
// ---------------------------------------------------------------------------

/// Reduce the raw block stream into the `<Chat>`-friendly item list.
/// Consecutive `automaton_stdout` / `automaton_stderr` blocks are
/// clustered into a single Terminal-in-EventBlock so the chat doesn't
/// drown in single-line rows when an automaton is chatty.
function blocksToChatItems(
  blocks: WireChatBlock[],
  agent: string,
): ChatItem[] {
  const items: ChatItem[] = [];
  let autoBuf: { line: string; isErr: boolean }[] = [];
  let autoStartIdx = -1;

  const flushAuto = () => {
    if (autoBuf.length === 0) return;
    const buf = autoBuf;
    const lines: TerminalLine[] = buf.map((entry, j) => ({
      id: `auto-${autoStartIdx}-${j}`,
      level: entry.isErr ? "error" : "info",
      message: entry.line,
    }));
    items.push({
      kind: "event",
      id: `auto-${autoStartIdx}`,
      evt: {
        kind: "info",
        icon: "terminal",
        summary: `automaton output (${lines.length} ${
          lines.length === 1 ? "line" : "lines"
        })`,
        details: <Terminal lines={lines} follow={false} />,
      },
    });
    autoBuf = [];
    autoStartIdx = -1;
  };

  blocks.forEach((b, i) => {
    if (b.kind === "automaton_stdout" || b.kind === "automaton_stderr") {
      if (autoBuf.length === 0) autoStartIdx = i;
      autoBuf.push({ line: b.line, isErr: b.kind === "automaton_stderr" });
      return;
    }
    flushAuto();
    items.push(blockToItem(b, i, agent));
  });
  flushAuto();
  return items;
}

function blockToItem(b: WireChatBlock, i: number, agent: string): ChatItem {
  const id = `b-${i}`;
  switch (b.kind) {
    case "agent_info": {
      const used = (b.context_input_tokens ?? 0) + (b.context_output_tokens ?? 0);
      const usage =
        b.context_window && used > 0
          ? `${used.toLocaleString()} / ${b.context_window.toLocaleString()} tokens`
          : null;
      return {
        kind: "event",
        id,
        evt: {
          kind: "info",
          icon: "cpu",
          summary: b.core,
          meta: [
            b.tools.length > 0 ? `tools: ${b.tools.join(", ")}` : null,
            usage,
          ]
            .filter(Boolean)
            .join(" · ") || undefined,
          details: b.prompt ? <pre>{b.prompt}</pre> : undefined,
        },
      };
    }
    case "user_text":
      return {
        kind: "message",
        id,
        self: true,
        avatarTone: "self",
        avatarText: "YO",
        authorName: "you",
        authorBadge: "you",
        body: b.text,
      };
    case "ai_markdown":
      return {
        kind: "message",
        id,
        avatarTone: "bot",
        avatarText: agent.slice(0, 2).toUpperCase(),
        authorName: agent,
        authorBadge: "agent",
        body: <MarkdownBody raw={b.raw} />,
      };
    case "bash_tool":
      return makeToolEvent(id, "terminal", `$ ${b.command}`, {
        body: b.stdout,
        isError: b.is_error,
        pending: b.pending,
      });
    case "edit_tool":
    case "write_tool":
      return makeToolEvent(
        id,
        "file-text",
        `${b.kind === "edit_tool" ? "edit" : "write"} ${b.file_path}`,
        { body: b.diff, isError: b.is_error, pending: b.pending, diff: true },
      );
    case "read_tool":
      return makeToolEvent(
        id,
        "eye",
        `read ${b.file_path}${b.range ? ` [${b.range}]` : ""}`,
        { body: "", isError: b.is_error, pending: b.pending },
      );
    case "grep_tool":
    case "glob_tool":
      return makeToolEvent(
        id,
        "search",
        `${b.kind === "grep_tool" ? "grep" : "glob"} "${b.pattern}"${
          b.path ? ` in ${b.path}` : ""
        }`,
        { body: b.matches, isError: b.is_error, pending: b.pending },
      );
    case "generic_tool":
      return makeToolEvent(
        id,
        "box",
        `${b.name}${b.input_pretty ? ` ${b.input_pretty}` : ""}`,
        { body: b.result_pretty, isError: b.is_error, pending: b.pending },
      );
    case "system": {
      const kind: EventKind =
        b.level === "error"
          ? "danger"
          : b.level === "warn"
            ? "warning"
            : "info";
      const icon: IconName =
        b.level === "error"
          ? "x"
          : b.level === "warn"
            ? "alert-triangle"
            : "info";
      return {
        kind: "event",
        id,
        evt: { kind, icon, summary: b.text },
      };
    }
    case "system_prompt":
      // Log-only block: the daemon writes it to the JSONL but never
      // pushes it onto chat.blocks, so it doesn't reach the live SSE
      // stream or the history endpoint. The case exists purely to
      // keep the switch exhaustive — render it as a collapsed info
      // event in case it ever does arrive (e.g. a future endpoint
      // that replays a JSONL file verbatim).
      return {
        kind: "event",
        id,
        evt: {
          kind: "info",
          icon: "cpu",
          summary: "system prompt",
          details: <pre>{b.text}</pre>,
        },
      };
    case "mail":
      return {
        kind: "event",
        id,
        evt: {
          kind: b.is_error ? "danger" : "info",
          icon: "bell",
          summary: `mail ${b.direction === "sent" ? "→" : "←"} ${b.counterpart}`,
          meta: b.pending ? "pending…" : undefined,
          details: b.body ? <pre>{b.body}</pre> : undefined,
        },
      };
    case "worker_spawn":
      return makeToolEvent(
        id,
        "users",
        `worker spawn: ${b.template}${b.worker_id ? ` → ${b.worker_id}` : ""}`,
        { body: b.task, isError: b.is_error, pending: b.pending },
      );
    case "worker_complete":
      return makeToolEvent(id, "check", "worker complete", {
        body: b.body,
        isError: b.is_error,
        pending: b.pending,
      });
    case "channel_message":
      return {
        kind: "event",
        id,
        evt: {
          kind: b.is_welcome ? "info" : "info",
          icon: "users",
          summary: `#${b.sender}`,
          details: b.body ? <pre>{b.body}</pre> : undefined,
        },
      };
    case "remote_patch":
      return makeToolEvent(id, "git-branch", `patch: ${b.summary}`, {
        body: b.diff,
        isError: b.is_error,
        pending: b.pending,
        diff: true,
      });
    case "task_op": {
      const detailText =
        (b.body ? `→ ${b.body}\n` : "") + (b.reply ? `← ${b.reply}` : "");
      return makeToolEvent(id, "check", `task ${b.op}: ${b.subject}`, {
        body: detailText,
        isError: b.is_error,
        pending: b.pending,
      });
    }
    case "automaton_stdout":
    case "automaton_stderr":
      // Handled by the cluster reducer, never reaches here.
      return {
        kind: "node",
        id,
        node: <pre class="text-xs">{b.line}</pre>,
      };
  }
}

function makeToolEvent(
  id: string,
  icon: IconName,
  summary: string,
  opts: { body: string; isError: boolean; pending: boolean; diff?: boolean },
): ChatItem {
  const kind: EventKind = opts.isError
    ? "danger"
    : opts.pending
      ? "warning"
      : "success";
  const evt: EventBlockProps = {
    kind,
    icon,
    summary: <span style={{ "font-family": "var(--font-mono)" }}>{summary}</span>,
    meta: opts.pending ? "pending…" : undefined,
    details: opts.body ? <pre>{opts.body}</pre> : undefined,
  };
  return { kind: "event", id, evt };
}

function MarkdownBody(props: { raw: string }): JSX.Element {
  const html = () => DOMPurify.sanitize(marked.parse(props.raw, { async: false }) as string);
  return <div innerHTML={html()} />;
}
