/* LogViewer — manager-side stdout/stderr tail for managed daemons.
 *
 * This component owns one EventSource against
 * `/api/managed-daemons/<id>/logs?follow=1` and renders incoming
 * `line` / `gap` / `lagged` events as a monospace stream. The browser
 * automatically reattaches with the last seen `id:` value via the
 * `Last-Event-ID` header on reconnect, so the server's byte-anchored
 * `since` cursor seamlessly covers transient network blips.
 *
 * INTEGRATION NOTE — sibling f5f708a1 owns the Daemons-tab slot for
 * this viewer. As of the current `main` (`a72c359` + `e726b3f`) the
 * `DaemonDetailPane` does not yet expose a logs section, so this
 * component lives unintegrated. To wire it once a slot lands, drop a
 *
 *     <LogViewer daemonId={d().id} height="320px" />
 *
 * inside DaemonDetailPane (probably below the `last_error` block).
 * No other change is needed — the component cleans itself up via
 * `onCleanup`.
 */

import {
  For,
  Show,
  createSignal,
  onCleanup,
  onMount,
  type JSX,
} from "solid-js";

import { daemonLogsUrl } from "../../api";
import Badge from "../ui/Badge";

interface LineEntry {
  kind: "line";
  ts: string;
  stream: "stdout" | "stderr";
  line: string;
  offset: number;
}

interface MarkerEntry {
  kind: "gap" | "lagged";
  text: string;
  offset: number;
}

type Entry = LineEntry | MarkerEntry;

interface Props {
  daemonId: string;
  /** Optional height of the scroll container; defaults to 320px. */
  height?: string;
}

const MAX_ENTRIES = 5_000;
let nextSyntheticOffset = -1;

export default function LogViewer(props: Props): JSX.Element {
  const [entries, setEntries] = createSignal<Entry[]>([]);
  const [stickyToBottom, setStickyToBottom] = createSignal(true);
  let containerRef: HTMLDivElement | undefined;
  let source: EventSource | undefined;

  const append = (entry: Entry) => {
    setEntries((prev) => {
      const next = prev.length >= MAX_ENTRIES
        ? prev.slice(prev.length - MAX_ENTRIES + 1)
        : prev.slice();
      next.push(entry);
      return next;
    });
    queueMicrotask(() => {
      if (stickyToBottom() && containerRef) {
        containerRef.scrollTop = containerRef.scrollHeight;
      }
    });
  };

  const onScroll = () => {
    if (!containerRef) return;
    const { scrollTop, scrollHeight, clientHeight } = containerRef;
    const atBottom = scrollHeight - (scrollTop + clientHeight) < 4;
    setStickyToBottom(atBottom);
  };

  onMount(() => {
    const url = daemonLogsUrl(props.daemonId, { follow: 1 });
    source = new EventSource(url);

    source.addEventListener("line", (ev) => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as {
          ts: string;
          stream: "stdout" | "stderr";
          line: string;
        };
        const offset = Number((ev as MessageEvent).lastEventId ?? 0);
        append({ kind: "line", ...data, offset });
      } catch {
        // Malformed payload — ignore.
      }
    });

    source.addEventListener("gap", (ev) => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as {
          dropped_until?: number;
        };
        const text = `[~${data.dropped_until ?? 0} bytes dropped]`;
        append({ kind: "gap", text, offset: nextSyntheticOffset-- });
      } catch {
        append({
          kind: "gap",
          text: "[bytes dropped]",
          offset: nextSyntheticOffset--,
        });
      }
    });

    source.addEventListener("lagged", (ev) => {
      try {
        const data = JSON.parse((ev as MessageEvent).data) as {
          dropped?: number;
        };
        const text = `[lagged: ${data.dropped ?? 0} events dropped]`;
        append({ kind: "lagged", text, offset: nextSyntheticOffset-- });
      } catch {
        append({
          kind: "lagged",
          text: "[lagged]",
          offset: nextSyntheticOffset--,
        });
      }
    });

    // The server emits no default-named 'message' events for line
    // payloads — they are all named 'line'. We still wire 'message'
    // so any future plain SSE frames don't get silently dropped.
    source.addEventListener("message", (ev) => {
      // Defensive: try to parse as a line entry.
      try {
        const data = JSON.parse(ev.data) as {
          ts?: string;
          stream?: "stdout" | "stderr";
          line?: string;
        };
        if (data.ts && data.stream && data.line !== undefined) {
          const offset = Number(ev.lastEventId ?? 0);
          append({
            kind: "line",
            ts: data.ts,
            stream: data.stream,
            line: data.line,
            offset,
          });
        }
      } catch {
        // Ignore malformed default-named events.
      }
    });

    source.onerror = () => {
      // EventSource auto-reconnects; nothing to do beyond letting the
      // browser drive the reconnect with the cached Last-Event-ID.
    };
  });

  onCleanup(() => {
    source?.close();
    source = undefined;
  });

  const containerStyle: JSX.CSSProperties = {
    height: props.height ?? "320px",
    overflow: "auto",
    background: "var(--bg-2, #0c0c0e)",
    border: "1px solid var(--border-subtle, #2a2a30)",
    "border-radius": "4px",
    padding: "8px 12px",
    "font-family": "var(--font-mono, ui-monospace, SFMono-Regular, monospace)",
    "font-size": "12px",
    "line-height": "1.4",
    color: "var(--fg-1, #d0d0d6)",
  };

  return (
    <div
      data-test="daemon-log-viewer"
      ref={(el) => {
        containerRef = el;
      }}
      style={containerStyle}
      onScroll={onScroll}
    >
      <Show
        when={entries().length > 0}
        fallback={
          <div style={{ color: "var(--fg-3, #888)", "font-style": "italic" }}>
            Waiting for output…
          </div>
        }
      >
        <For each={entries()}>
          {(entry) => {
            if (entry.kind === "line") {
              return (
                <div
                  style={{
                    display: "flex",
                    gap: "8px",
                    "white-space": "pre-wrap",
                    "word-break": "break-all",
                  }}
                >
                  <Badge
                    tone={entry.stream === "stderr" ? "warning" : "neutral"}
                  >
                    {entry.stream}
                  </Badge>
                  <span>{entry.line}</span>
                </div>
              );
            }
            return (
              <div
                style={{
                  color: "var(--fg-3, #888)",
                  "font-style": "italic",
                  margin: "4px 0",
                }}
              >
                {entry.text}
              </div>
            );
          }}
        </For>
      </Show>
    </div>
  );
}
