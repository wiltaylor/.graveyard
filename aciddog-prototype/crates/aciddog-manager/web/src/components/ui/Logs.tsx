import { For, type JSX } from "solid-js";

export type LogLevel = "debug" | "info" | "warn" | "error" | "fatal";

export interface LogLine {
  id: string;
  time: JSX.Element;
  level: LogLevel;
  message: JSX.Element;
}

interface Props {
  lines: LogLine[];
  height?: string;
  class?: string;
}

const LEVEL_CLASS: Record<LogLevel, string> = {
  debug: "debug",
  info: "info",
  warn: "warn",
  error: "error",
  fatal: "error",
};

export default function Logs(props: Props): JSX.Element {
  return (
    <div
      class={`flogs${props.class ? ` ${props.class}` : ""}`}
      style={{ height: props.height ?? undefined }}
    >
      <For each={props.lines}>
        {(line) => (
          <div class="flog-line">
            <span class="flog-time">{line.time}</span>
            <span class={`flog-level ${LEVEL_CLASS[line.level]}`}>{line.level.toUpperCase()}</span>
            <span class="flog-msg">{line.message}</span>
          </div>
        )}
      </For>
    </div>
  );
}
