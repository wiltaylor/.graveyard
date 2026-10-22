// Pure helper: maps a managed-daemon `desired_state` or `observed_state`
// onto a `Badge` tone + glyph + label. Mirrors the agents/agentStatus.ts
// precedent (decisions/9de9cdf5: "every row renders glyph + tone via the
// status-visual helper; never inline a tone map").
//
// Pill palette (from the plan / decisions/585d4b15):
//   DESIRED:
//     running   -> info,    glyph ▶, label "running"   (target state)
//     stopped   -> neutral, glyph ■, label "stopped"   (target state)
//   OBSERVED:
//     starting  -> warning, glyph ⌛, label "starting"  (transitional)
//     running   -> success, glyph 🟢, label "running"   (steady)
//     stopping  -> warning, glyph ⌛, label "stopping"  (transitional)
//     stopped   -> neutral, glyph ⚪, label "stopped"   (steady)
//     crashed   -> danger,  glyph 🔴, label "crashed"   (terminal-error)
//
// The two `kind` overloads keep call sites typed: passing a desired
// variant under the observed channel (or vice versa) is a compile
// error, so the wire-side mismatch can never silently desugar to a
// neutral fallback.

import type { BadgeTone } from "../ui/Badge";
import type {
  ManagedDaemonDesiredState,
  ManagedDaemonObservedState,
} from "../../types";

export type { BadgeTone };

export interface DaemonStateVisual {
  glyph: string;
  tone: BadgeTone;
  label: string;
}

const DESIRED: Record<ManagedDaemonDesiredState, DaemonStateVisual> = {
  running: { glyph: "▶", tone: "info", label: "running" },
  stopped: { glyph: "■", tone: "neutral", label: "stopped" },
};

const OBSERVED: Record<ManagedDaemonObservedState, DaemonStateVisual> = {
  starting: { glyph: "⌛", tone: "warning", label: "starting" },
  running: { glyph: "🟢", tone: "success", label: "running" },
  stopping: { glyph: "⌛", tone: "warning", label: "stopping" },
  stopped: { glyph: "⚪", tone: "neutral", label: "stopped" },
  crashed: { glyph: "🔴", tone: "danger", label: "crashed" },
};

export function daemonStateVisual(
  kind: "desired",
  state: ManagedDaemonDesiredState,
): DaemonStateVisual;
export function daemonStateVisual(
  kind: "observed",
  state: ManagedDaemonObservedState,
): DaemonStateVisual;
export function daemonStateVisual(
  kind: "desired" | "observed",
  state: ManagedDaemonDesiredState | ManagedDaemonObservedState,
): DaemonStateVisual {
  if (kind === "desired") {
    return DESIRED[state as ManagedDaemonDesiredState];
  }
  return OBSERVED[state as ManagedDaemonObservedState];
}
