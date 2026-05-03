import { For, type JSX } from "solid-js";

import type { ManagedDaemonView } from "../../types";
import DaemonRow from "./DaemonRow";

interface Props {
  daemons: ManagedDaemonView[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onStart: (id: string) => void | Promise<void>;
  onStop: (id: string) => void | Promise<void>;
  onReload: (id: string) => void | Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onOpenInChat: (instanceId: string) => void;
}

/// Table renderer. One `<tr>` per row; the per-row affordance gating
/// + delete-confirm modal live inside `DaemonRow`. Columns:
///   - name (mono)
///   - work_dir (mono / dim)
///   - desired pill
///   - observed pill
///   - actions (right-aligned)
export default function DaemonsList(props: Props): JSX.Element {
  return (
    <table class="ftable" data-test="daemons-table">
      <thead>
        <tr>
          <th>Name</th>
          <th>Work dir</th>
          <th>Desired</th>
          <th>Observed</th>
          <th style={{ "text-align": "right" }}>Actions</th>
        </tr>
      </thead>
      <tbody>
        <For each={props.daemons}>
          {(d) => (
            <DaemonRow
              daemon={d}
              selected={props.selectedId === d.id}
              onSelect={props.onSelect}
              onStart={props.onStart}
              onStop={props.onStop}
              onReload={props.onReload}
              onDelete={props.onDelete}
              onOpenInChat={props.onOpenInChat}
            />
          )}
        </For>
      </tbody>
    </table>
  );
}
