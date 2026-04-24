import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";
import DOMPurify from "dompurify";
import { marked } from "marked";

import { api, chatEventsUrl } from "../api";
import type { ChatBlockEvent, WireChatBlock } from "../types";

interface Props {
  instanceId: string;
  agent: string;
  onClose: () => void;
}

/// Stream status surfaced next to the header. Distinct from the "last
/// error" line because a lagged event is recoverable (the stream keeps
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
    setBlocks([]);
    setDroppedCount(0);
    setStreamStatus("connecting");
    void loadHistory();

    const es = new EventSource(chatEventsUrl(instId, agentId));
    es.onopen = () => setStreamStatus("live");
    es.onmessage = (ev) => {
      try {
        const parsed = JSON.parse(ev.data) as ChatBlockEvent;
        setBlocks((prev) => [...prev, parsed.block]);
      } catch {
        // Malformed event — ignore rather than tearing down the stream.
      }
    };
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
      // The browser will keep delivering subsequent events; re-sync
      // history so the user's view is whole again.
      void loadHistory();
    });
    es.onerror = () => {
      setStreamStatus("closed");
      // EventSource auto-reconnects; `onopen` will flip us back to live.
    };
    onCleanup(() => es.close());
  });

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
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

  return (
    <div class="flex flex-col h-[calc(100vh-3.5rem)]">
      <header class="flex items-center justify-between gap-4 border-b border-slate-800 px-4 py-2 bg-slate-900/60">
        <div class="flex items-center gap-2">
          <button
            type="button"
            onClick={props.onClose}
            class="text-sm text-slate-400 hover:text-slate-100"
            title="Back to instance detail"
          >
            ← back
          </button>
          <h2 class="text-lg font-semibold">{props.agent}</h2>
          <StreamStatusBadge
            status={streamStatus()}
            dropped={droppedCount()}
          />
        </div>
        <button
          type="button"
          onClick={loadHistory}
          class="text-xs text-slate-400 hover:text-slate-100"
        >
          refresh
        </button>
      </header>

      <Show when={historyError()}>
        <div class="mx-4 mt-2 rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
          {historyError()}
        </div>
      </Show>

      <div class="flex-1 overflow-y-auto px-4 py-3 space-y-3">
        <Show
          when={blocks().length > 0}
          fallback={
            <div class="text-sm text-slate-500 italic">
              {historyError() ? "history unavailable." : "(no messages yet)"}
            </div>
          }
        >
          <For each={blocks()}>{(b) => <BlockView block={b} />}</For>
        </Show>
      </div>

      <form
        onSubmit={submit}
        class="border-t border-slate-800 p-3 bg-slate-900/60 space-y-2"
      >
        <Show when={sendError()}>
          <div class="rounded border border-red-800 bg-red-900/40 px-2 py-1 text-sm text-red-200">
            {sendError()}
          </div>
        </Show>
        <div class="flex gap-2">
          <textarea
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                (e.currentTarget.form as HTMLFormElement).requestSubmit();
              }
            }}
            placeholder="Send a message as the human operator. Enter to send, Shift+Enter for newline."
            rows={2}
            class="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-sm resize-y"
          />
          <button
            type="submit"
            disabled={sending() || input().trim() === ""}
            class="self-end rounded border border-emerald-700 bg-emerald-900/40 hover:bg-emerald-800 disabled:opacity-50 px-3 py-1 text-sm"
          >
            {sending() ? "Sending…" : "Send"}
          </button>
        </div>
      </form>
    </div>
  );
}

function StreamStatusBadge(props: {
  status: StreamStatus;
  dropped: number;
}): JSX.Element {
  return (
    <span
      class="text-xs rounded px-1.5 py-0.5 border"
      classList={{
        "text-slate-400 border-slate-700": props.status === "connecting",
        "text-emerald-300 border-emerald-800 bg-emerald-900/30":
          props.status === "live",
        "text-amber-300 border-amber-800 bg-amber-900/30":
          props.status === "lagged",
        "text-red-300 border-red-800 bg-red-900/30":
          props.status === "closed",
      }}
      title={
        props.status === "lagged" && props.dropped > 0
          ? `dropped ${props.dropped} event(s); history re-fetched`
          : undefined
      }
    >
      {props.status === "lagged" && props.dropped > 0
        ? `lagged · ${props.dropped} dropped`
        : props.status}
    </span>
  );
}

function BlockView(props: { block: WireChatBlock }): JSX.Element {
  return (
    <Switch>
      <Match when={props.block.kind === "agent_info" && props.block}>
        {(b) => <AgentInfoBlock block={b()} />}
      </Match>
      <Match when={props.block.kind === "user_text" && props.block}>
        {(b) => <UserTextBlock text={b().text} />}
      </Match>
      <Match when={props.block.kind === "ai_markdown" && props.block}>
        {(b) => <AiMarkdownBlock raw={b().raw} />}
      </Match>
      <Match when={props.block.kind === "bash_tool" && props.block}>
        {(b) => (
          <ToolCard
            heading={`$ ${b().command}`}
            body={b().stdout}
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match
        when={
          (props.block.kind === "edit_tool" ||
            props.block.kind === "write_tool") &&
          props.block
        }
      >
        {(b) => (
          <ToolCard
            heading={`${b().kind === "edit_tool" ? "edit" : "write"} ${b().file_path}`}
            body={b().diff}
            isError={b().is_error}
            pending={b().pending}
            bodyClass="diff"
          />
        )}
      </Match>
      <Match when={props.block.kind === "read_tool" && props.block}>
        {(b) => (
          <ToolCard
            heading={`read ${b().file_path}${b().range ? ` [${b().range}]` : ""}`}
            body=""
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match
        when={
          (props.block.kind === "grep_tool" ||
            props.block.kind === "glob_tool") &&
          props.block
        }
      >
        {(b) => (
          <ToolCard
            heading={`${b().kind === "grep_tool" ? "grep" : "glob"} "${b().pattern}"${b().path ? ` in ${b().path}` : ""}`}
            body={b().matches}
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match when={props.block.kind === "generic_tool" && props.block}>
        {(b) => (
          <ToolCard
            heading={`${b().name}${b().input_pretty ? ` ${b().input_pretty}` : ""}`}
            body={b().result_pretty}
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match when={props.block.kind === "system" && props.block}>
        {(b) => <SystemBlock text={b().text} level={b().level} />}
      </Match>
      <Match when={props.block.kind === "mail" && props.block}>
        {(b) => (
          <MailBlock
            direction={b().direction}
            counterpart={b().counterpart}
            body={b().body}
            isError={b().is_error}
          />
        )}
      </Match>
      <Match when={props.block.kind === "worker_spawn" && props.block}>
        {(b) => (
          <ToolCard
            heading={`worker spawn: ${b().template}${b().worker_id ? ` → ${b().worker_id}` : ""}`}
            body={b().task}
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match when={props.block.kind === "worker_complete" && props.block}>
        {(b) => (
          <ToolCard
            heading="worker complete"
            body={b().body}
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match when={props.block.kind === "channel_message" && props.block}>
        {(b) => (
          <ChannelBlock
            sender={b().sender}
            body={b().body}
            isWelcome={b().is_welcome}
          />
        )}
      </Match>
      <Match when={props.block.kind === "remote_patch" && props.block}>
        {(b) => (
          <ToolCard
            heading={`patch: ${b().summary}`}
            body={b().diff}
            isError={b().is_error}
            pending={b().pending}
            bodyClass="diff"
          />
        )}
      </Match>
      <Match when={props.block.kind === "task_op" && props.block}>
        {(b) => (
          <ToolCard
            heading={`task ${b().op}: ${b().subject}`}
            body={
              (b().body ? `→ ${b().body}\n` : "") +
              (b().reply ? `← ${b().reply}` : "")
            }
            isError={b().is_error}
            pending={b().pending}
          />
        )}
      </Match>
      <Match
        when={
          (props.block.kind === "automaton_stdout" ||
            props.block.kind === "automaton_stderr") &&
          props.block
        }
      >
        {(b) => (
          <pre
            class="rounded bg-slate-900/60 border border-slate-800 px-2 py-1 text-xs whitespace-pre-wrap break-words"
            classList={{
              "text-slate-300": b().kind === "automaton_stdout",
              "text-red-300": b().kind === "automaton_stderr",
            }}
          >
            {b().line}
          </pre>
        )}
      </Match>
    </Switch>
  );
}

function UserTextBlock(props: { text: string }): JSX.Element {
  return (
    <div class="flex justify-end">
      <div class="max-w-[80%] rounded-lg bg-emerald-900/50 border border-emerald-800 px-3 py-2 text-sm whitespace-pre-wrap break-words">
        {props.text}
      </div>
    </div>
  );
}

function AiMarkdownBlock(props: { raw: string }): JSX.Element {
  const rendered = () => {
    const html = marked.parse(props.raw, { async: false }) as string;
    return DOMPurify.sanitize(html);
  };
  return (
    <div class="flex justify-start">
      <div
        class="max-w-[85%] rounded-lg bg-slate-900/80 border border-slate-800 px-3 py-2 text-sm prose prose-invert prose-sm max-w-none"
        innerHTML={rendered()}
      />
    </div>
  );
}

function ToolCard(props: {
  heading: string;
  body: string;
  isError: boolean;
  pending: boolean;
  bodyClass?: "diff";
}): JSX.Element {
  const [expanded, setExpanded] = createSignal(false);
  const bodyLines = () => props.body.split("\n");
  const showFull = () => expanded() || bodyLines().length <= 12;

  return (
    <div
      class="rounded border text-xs"
      classList={{
        "border-red-800 bg-red-950/40": props.isError,
        "border-slate-800 bg-slate-900/60": !props.isError,
      }}
    >
      <div class="flex items-center justify-between gap-2 px-2 py-1">
        <div class="font-mono truncate">{props.heading}</div>
        <div class="flex items-center gap-2">
          <Show when={props.pending}>
            <span class="text-amber-300">pending…</span>
          </Show>
          <Show when={bodyLines().length > 12}>
            <button
              type="button"
              class="text-slate-400 hover:text-slate-100"
              onClick={() => setExpanded(!expanded())}
            >
              {expanded() ? "collapse" : "expand"}
            </button>
          </Show>
        </div>
      </div>
      <Show when={props.body}>
        <pre
          class="border-t border-slate-800 px-2 py-1 font-mono whitespace-pre-wrap break-words"
          classList={{
            "text-[0.72rem]": props.bodyClass === "diff",
          }}
        >
          {showFull()
            ? props.body
            : bodyLines().slice(0, 12).join("\n") + "\n…"}
        </pre>
      </Show>
    </div>
  );
}

function SystemBlock(props: {
  text: string;
  level: "info" | "warn" | "error";
}): JSX.Element {
  return (
    <div
      class="rounded border px-2 py-1 text-xs"
      classList={{
        "border-slate-800 bg-slate-900/60 text-slate-400":
          props.level === "info",
        "border-amber-800 bg-amber-900/40 text-amber-200":
          props.level === "warn",
        "border-red-800 bg-red-900/40 text-red-200":
          props.level === "error",
      }}
    >
      {props.text}
    </div>
  );
}

function MailBlock(props: {
  direction: "sent" | "received";
  counterpart: string;
  body: string;
  isError: boolean;
}): JSX.Element {
  const arrow = () => (props.direction === "sent" ? "→" : "←");
  return (
    <div
      class="rounded border px-2 py-1 text-sm"
      classList={{
        "border-red-800 bg-red-950/40": props.isError,
        "border-sky-800 bg-sky-950/30": !props.isError,
      }}
    >
      <div class="text-xs text-slate-400 font-mono mb-1">
        mail {arrow()} {props.counterpart}
      </div>
      <div class="whitespace-pre-wrap break-words">{props.body}</div>
    </div>
  );
}

function ChannelBlock(props: {
  sender: string;
  body: string;
  isWelcome: boolean;
}): JSX.Element {
  return (
    <div
      class="rounded border px-2 py-1 text-sm"
      classList={{
        "border-slate-800 bg-slate-900/30 text-slate-400 italic":
          props.isWelcome,
        "border-indigo-800 bg-indigo-950/30": !props.isWelcome,
      }}
    >
      <div class="text-xs text-slate-400 font-mono mb-1">
        #{props.sender}
      </div>
      <div class="whitespace-pre-wrap break-words">{props.body}</div>
    </div>
  );
}

function AgentInfoBlock(props: {
  block: {
    kind: "agent_info";
    core: string;
    tools: string[];
    prompt: string;
    context_input_tokens?: number;
    context_output_tokens?: number;
    context_window?: number;
  };
}): JSX.Element {
  const used = () =>
    (props.block.context_input_tokens ?? 0) +
    (props.block.context_output_tokens ?? 0);
  const showUsage = () =>
    props.block.context_window && used() > 0;

  return (
    <div class="rounded border border-slate-700 bg-slate-900/80 px-3 py-2 text-xs space-y-1">
      <div class="font-semibold text-slate-200">{props.block.core}</div>
      <Show when={props.block.tools.length > 0}>
        <div class="text-slate-400">
          tools: {props.block.tools.join(", ")}
        </div>
      </Show>
      <Show when={showUsage()}>
        <div class="text-slate-400">
          context: {used().toLocaleString()} / {" "}
          {props.block.context_window?.toLocaleString()} tokens
        </div>
      </Show>
      <Show when={props.block.prompt}>
        <details class="text-slate-300">
          <summary class="cursor-pointer text-slate-400 hover:text-slate-100">
            show prompt
          </summary>
          <pre class="mt-1 whitespace-pre-wrap break-words text-[0.72rem]">
            {props.block.prompt}
          </pre>
        </details>
      </Show>
    </div>
  );
}
