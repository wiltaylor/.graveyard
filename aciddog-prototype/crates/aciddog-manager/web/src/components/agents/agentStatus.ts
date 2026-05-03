// Pure helper: maps daemon-emitted (kind, status) onto a Badge tone +
// glyph + label. Mirrors the TUI's `render_agent_list` palette
// (`crates/aciddog/src/tui/ui.rs:340-383`) and `status_label`
// (`crates/aciddog/src/tui/app.rs:9270-9281`) so a user toggling
// between TUI and web sees the same shape language.
//
// Round palette is used for kind === "agent" / "worker" (and any
// future non-automaton kind); square palette is reserved for
// kind === "automaton". Automaton has no `hibernated` arm — the
// TUI catch-all `(ChatKind::Automaton, _) => "🟩"` is mirrored
// here as a fall-back to the ready glyph + ready tone for any
// unknown automaton status. For unknown non-automaton statuses we
// fall back to a neutral white-circle glyph plus the raw status as
// the label so a future daemon string still degrades gracefully.

import type { BadgeTone } from "../ui/Badge";

export type { BadgeTone };

export interface AgentStatusVisual {
  glyph: string;
  tone: BadgeTone;
  label: string;
}

const ROUND: Record<string, AgentStatusVisual> = {
  ready: { glyph: "🟢", tone: "success", label: "ready" },
  working: { glyph: "🟡", tone: "warning", label: "working" },
  error: { glyph: "🔴", tone: "danger", label: "error" },
  hibernated: { glyph: "💤", tone: "neutral", label: "hibernated" },
  asleep: { glyph: "🌙", tone: "info", label: "asleep" },
};

const SQUARE: Record<string, AgentStatusVisual> = {
  ready: { glyph: "🟩", tone: "success", label: "ready" },
  working: { glyph: "🟨", tone: "warning", label: "working" },
  error: { glyph: "🟥", tone: "danger", label: "error" },
  asleep: { glyph: "🟦", tone: "info", label: "asleep" },
};

export function agentStatusVisual(
  kind: string,
  status: string,
): AgentStatusVisual {
  if (kind === "automaton") {
    // No `hibernated` arm — automatons have no AI state to park.
    // TUI catch-all `(ChatKind::Automaton, _) => "🟩"`: anything we
    // don't recognise still gets the ready square so the row is at
    // least visible.
    return SQUARE[status] ?? { glyph: "🟩", tone: "success", label: "ready" };
  }
  return ROUND[status] ?? { glyph: "⚪", tone: "neutral", label: status };
}
