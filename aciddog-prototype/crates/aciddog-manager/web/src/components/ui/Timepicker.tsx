import { For, Show, createSignal, type JSX } from "solid-js";

const DOW_LABELS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

function pad(n: number): string {
  return n.toString().padStart(2, "0");
}

interface Props {
  hour: number;
  minute: number;
  onChange: (hour: number, minute: number) => void;
  presets?: { label: string; hour: number; minute: number }[];
  showAmPm?: boolean;
  daysOfWeek?: boolean[];
  onDaysChange?: (days: boolean[]) => void;
  cronExpression?: string;
  cronNatural?: string;
}

export default function Timepicker(props: Props): JSX.Element {
  const [active, setActive] = createSignal<"h" | "m">("h");

  const adjust = (target: "h" | "m", dir: 1 | -1) => {
    if (target === "h") {
      const h = (props.hour + dir + 24) % 24;
      props.onChange(h, props.minute);
    } else {
      const m = (props.minute + dir + 60) % 60;
      props.onChange(props.hour, m);
    }
  };

  const toggleDay = (idx: number) => {
    if (!props.onDaysChange || !props.daysOfWeek) return;
    const next = [...props.daysOfWeek];
    next[idx] = !next[idx];
    props.onDaysChange(next);
  };

  const ampm = () => (props.hour < 12 ? "AM" : "PM");
  const display12 = () => {
    const h = props.hour % 12;
    return h === 0 ? 12 : h;
  };

  return (
    <div>
      <div class="tp-clock">
        <span
          class={`seg${active() === "h" ? " active" : ""}`}
          onClick={() => setActive("h")}
        >
          <span class="arrow" onClick={() => adjust("h", 1)}>
            ▲
          </span>
          <span>{props.showAmPm ? pad(display12()) : pad(props.hour)}</span>
          <span class="arrow" onClick={() => adjust("h", -1)}>
            ▼
          </span>
        </span>
        <span class="colon">:</span>
        <span
          class={`seg${active() === "m" ? " active" : ""}`}
          onClick={() => setActive("m")}
        >
          <span class="arrow" onClick={() => adjust("m", 1)}>
            ▲
          </span>
          <span>{pad(props.minute)}</span>
          <span class="arrow" onClick={() => adjust("m", -1)}>
            ▼
          </span>
        </span>
        <Show when={props.showAmPm}>
          <span class="ampm">
            <span
              class={ampm() === "AM" ? "on" : ""}
              onClick={() => {
                if (props.hour >= 12) props.onChange(props.hour - 12, props.minute);
              }}
            >
              AM
            </span>
            <span
              class={ampm() === "PM" ? "on" : ""}
              onClick={() => {
                if (props.hour < 12) props.onChange(props.hour + 12, props.minute);
              }}
            >
              PM
            </span>
          </span>
        </Show>
      </div>
      <Show when={props.presets && props.presets.length > 0}>
        <div class="presets">
          <For each={props.presets}>
            {(p) => (
              <button
                type="button"
                class={`preset${
                  p.hour === props.hour && p.minute === props.minute ? " on" : ""
                }`}
                onClick={() => props.onChange(p.hour, p.minute)}
              >
                {p.label}
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.daysOfWeek}>
        <div class="dow" style={{ "margin-top": "12px" }}>
          <For each={DOW_LABELS}>
            {(label, i) => (
              <button
                type="button"
                class={`${props.daysOfWeek![i()] ? "on" : ""}${i() >= 5 ? " weekend" : ""}`}
                onClick={() => toggleDay(i())}
              >
                {label}
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.cronExpression}>
        <div class="cron">
          <span>
            <span class="label">Cron</span>
            <span class="expr">{props.cronExpression}</span>
          </span>
          <Show when={props.cronNatural}>
            <span class="nat">{props.cronNatural}</span>
          </Show>
        </div>
      </Show>
    </div>
  );
}
