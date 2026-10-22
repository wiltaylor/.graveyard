import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
  type JSX,
} from "solid-js";
import { Portal } from "solid-js/web";
import Icon from "./Icon";

const DOW = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

function pad(n: number): string {
  return n.toString().padStart(2, "0");
}

function formatDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function parseDate(s: string): Date | null {
  const m = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(s);
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return Number.isNaN(d.getTime()) ? null : d;
}

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

interface CalendarBuildDay {
  date: Date;
  inMonth: boolean;
}

function buildMonth(year: number, month: number): CalendarBuildDay[] {
  const first = new Date(year, month, 1);
  const startOffset = (first.getDay() + 6) % 7;
  const start = new Date(year, month, 1 - startOffset);
  const days: CalendarBuildDay[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    days.push({ date: d, inMonth: d.getMonth() === month });
  }
  return days;
}

interface Props {
  label?: string;
  value: string | null;
  onChange: (value: string | null) => void;
  placeholder?: string;
  disabled?: boolean;
  error?: boolean | string;
}

export default function Datepicker(props: Props): JSX.Element {
  const today = new Date();
  const initial = props.value ? parseDate(props.value) : null;
  const [year, setYear] = createSignal((initial ?? today).getFullYear());
  const [month, setMonth] = createSignal((initial ?? today).getMonth());
  const [open, setOpen] = createSignal(false);
  const [anchor, setAnchor] = createSignal<{ left: number; top: number } | null>(null);
  let inputEl: HTMLButtonElement | undefined;
  let popEl: HTMLDivElement | undefined;

  const updateAnchor = () => {
    if (!inputEl) return;
    const r = inputEl.getBoundingClientRect();
    setAnchor({ left: r.left, top: r.bottom + 4 });
  };

  const docClick = (ev: MouseEvent) => {
    const t = ev.target as Node;
    if (popEl?.contains(t) || inputEl?.contains(t)) return;
    setOpen(false);
  };

  createEffect(() => {
    if (open()) {
      updateAnchor();
      document.addEventListener("mousedown", docClick);
      window.addEventListener("resize", updateAnchor);
      window.addEventListener("scroll", updateAnchor, true);
      onCleanup(() => {
        document.removeEventListener("mousedown", docClick);
        window.removeEventListener("resize", updateAnchor);
        window.removeEventListener("scroll", updateAnchor, true);
      });
    }
  });

  const days = () => buildMonth(year(), month());

  const selectDay = (d: Date) => {
    props.onChange(formatDate(d));
    setOpen(false);
  };

  const prev = () => {
    const m = month() - 1;
    if (m < 0) {
      setMonth(11);
      setYear(year() - 1);
    } else {
      setMonth(m);
    }
  };
  const next = () => {
    const m = month() + 1;
    if (m > 11) {
      setMonth(0);
      setYear(year() + 1);
    } else {
      setMonth(m);
    }
  };

  const selected = () => (props.value ? parseDate(props.value) : null);
  const isError = () => Boolean(props.error);
  const helpText = () => (typeof props.error === "string" ? props.error : null);

  return (
    <div class="dp-field">
      <Show when={props.label}>
        <label>{props.label}</label>
      </Show>
      <button
        ref={inputEl}
        type="button"
        class={`dp-input${open() ? " open" : ""}${isError() ? " is-error" : ""}`}
        disabled={props.disabled}
        onClick={() => setOpen(!open())}
        style={{
          width: "100%",
          "text-align": "left",
          font: "inherit",
          color: props.value ? "var(--fg-0)" : "var(--fg-3)",
        }}
      >
        <span class="icon">
          <Icon name="clock" size={14} />
        </span>
        <span style={{ flex: "1" }}>{props.value ?? props.placeholder ?? "Select date"}</span>
      </button>
      <Show when={helpText()}>
        <span style={{ "font-size": "11px", color: "var(--danger-fg)" }}>
          {helpText()}
        </span>
      </Show>
      <Show when={open() && anchor()}>
        <Portal>
          <div
            ref={popEl}
            class="cal"
            style={{
              position: "fixed",
              left: `${anchor()!.left}px`,
              top: `${anchor()!.top}px`,
              "z-index": "200",
            }}
          >
            <div class="cal-head">
              <span class="cal-month">
                {MONTHS[month()]} {year()}
              </span>
              <span class="cal-nav">
                <button type="button" onClick={prev} aria-label="Previous month">
                  ‹
                </button>
                <button type="button" onClick={next} aria-label="Next month">
                  ›
                </button>
              </span>
            </div>
            <div class="cal-grid">
              <For each={DOW}>{(d) => <span class="cal-dow">{d}</span>}</For>
              <For each={days()}>
                {(day) => {
                  const cls = () => {
                    const c = ["cal-day"];
                    if (!day.inMonth) c.push("muted");
                    if (sameDay(day.date, today)) c.push("today");
                    const sel = selected();
                    if (sel && sameDay(day.date, sel)) c.push("selected");
                    return c.join(" ");
                  };
                  return (
                    <button
                      type="button"
                      class={cls()}
                      onClick={() => selectDay(day.date)}
                      style={{ background: "transparent", border: "0", font: "inherit" }}
                    >
                      {day.date.getDate()}
                    </button>
                  );
                }}
              </For>
            </div>
            <div class="cal-foot">
              <span>{props.value ?? "—"}</span>
              <button type="button" onClick={() => selectDay(new Date())}>
                Today
              </button>
            </div>
          </div>
        </Portal>
      </Show>
    </div>
  );
}
