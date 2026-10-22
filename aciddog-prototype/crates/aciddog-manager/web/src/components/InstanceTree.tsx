import { Show, createSignal, type JSX } from "solid-js";
import { Tree } from "./ui";
import type { TreeNode } from "./ui";
import type {
  AgentListResponse,
  ChannelListResponse,
  InstanceSummary,
} from "../types";

interface Props {
  instances: InstanceSummary[];
  selectedId: string | null;
  selectedAgent: string | null;
  selectedChannel: string | null;
  onSelect: (id: string) => void;
  onSelectAgent: (instanceId: string, agentId: string) => void;
  onSelectChannel: (instanceId: string, channelId: string) => void;
  agentsFor: (id: string) => AgentListResponse | undefined;
  channelsFor: (id: string) => ChannelListResponse | undefined;
  onExpand: (id: string) => void;
}

export default function InstanceTree(props: Props): JSX.Element {
  const [expanded, setExpanded] = createSignal(new Set<string>());

  const toggle = (id: string) => {
    const next = new Set(expanded());
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
      // expand both agent + channel groups for the instance prefix
      if (id.startsWith("inst:")) {
        const instId = id.slice("inst:".length);
        next.add(`agents:${instId}`);
        next.add(`channels:${instId}`);
        // Auto-expand every group bucket under the agents node so the
        // user sees their agents on first click rather than a row of
        // collapsed group folders. Buckets enumerated lazily from the
        // current agentsFor() snapshot — ones that arrive later via
        // SSE will start collapsed (user can click to open).
        const agentsResp = props.agentsFor(instId);
        if (agentsResp?.ok) {
          const seen = new Set<string>();
          for (const a of agentsResp.agents) {
            const key = a.group ?? "__ungrouped__";
            if (seen.has(key)) continue;
            seen.add(key);
            next.add(`agentgroup:${instId}:${key}`);
          }
        }
        props.onExpand(instId);
      }
    }
    setExpanded(next);
  };

  const handleSelect = (id: string) => {
    if (id.startsWith("inst:")) {
      const instId = id.slice("inst:".length);
      props.onSelect(instId);
      // also expand on first select
      const next = new Set(expanded());
      if (!next.has(id)) {
        next.add(id);
        next.add(`agents:${instId}`);
        next.add(`channels:${instId}`);
        props.onExpand(instId);
        setExpanded(next);
      }
    } else if (id.startsWith("agent:")) {
      const [, instId, agentId] = id.split(":");
      props.onSelectAgent(instId, agentId);
    } else if (id.startsWith("channel:")) {
      const [, instId, channelId] = id.split(":");
      props.onSelectChannel(instId, channelId);
    }
  };

  const selectedTreeId = (): string | null => {
    if (props.selectedAgent && props.selectedId) {
      return `agent:${props.selectedId}:${props.selectedAgent}`;
    }
    if (props.selectedChannel && props.selectedId) {
      return `channel:${props.selectedId}:${props.selectedChannel}`;
    }
    if (props.selectedId) {
      return `inst:${props.selectedId}`;
    }
    return null;
  };

  const buildNodes = (): TreeNode[] => {
    return props.instances.map((inst) => {
      const agentsResp = props.agentsFor(inst.id);
      const channelsResp = props.channelsFor(inst.id);

      // Bucket agents by their `group` value, preserving
      // first-appearance order. `null` (no group) is appended last so
      // ungrouped rows always render below named groups, matching the
      // TUI sidebar's `(ungrouped)` placement. Within each bucket the
      // backend already pins leader=true rows first; we run a stable
      // sort here too so older daemons that don't sort still display
      // sensibly.
      const agentChildren: TreeNode[] = (() => {
        if (!agentsResp?.ok) return [];
        const buckets = new Map<string, typeof agentsResp.agents>();
        const order: string[] = [];
        for (const a of agentsResp.agents) {
          const key = a.group ?? "__ungrouped__";
          if (!buckets.has(key)) {
            buckets.set(key, []);
            order.push(key);
          }
          buckets.get(key)!.push(a);
        }
        if (!order.includes("__ungrouped__")) {
          order.push("__ungrouped__");
        }
        const nodes: TreeNode[] = [];
        for (const key of order) {
          const rows = buckets.get(key);
          if (!rows || rows.length === 0) continue;
          const sorted = [...rows].sort((a, b) => {
            const aIsLeader = a.leader && a.parent === null;
            const bIsLeader = b.leader && b.parent === null;
            if (aIsLeader === bIsLeader) return 0;
            return aIsLeader ? -1 : 1;
          });
          const label = key === "__ungrouped__" ? "(ungrouped)" : key;
          nodes.push({
            id: `agentgroup:${inst.id}:${key}`,
            label,
            icon: "users",
            meta: `${sorted.length}`,
            children: sorted.map((agent) => ({
              id: `agent:${inst.id}:${agent.id}`,
              label: agent.id,
              icon: "user",
              meta: agent.status,
            })),
          });
        }
        return nodes;
      })();

      const channelChildren: TreeNode[] = channelsResp?.ok
        ? channelsResp.channels.map((channel) => ({
            id: `channel:${inst.id}:${channel.id}`,
            label: `#${channel.id}`,
            icon: "menu",
            meta: `${channel.members.length}m`,
          }))
        : [];

      const agentsGroup: TreeNode = {
        id: `agents:${inst.id}`,
        label: "Agents",
        icon: "users",
        children: agentChildren.length
          ? agentChildren
          : [
              {
                id: `agents-empty:${inst.id}`,
                label: agentsResp
                  ? agentsResp.error ?? "No agents reported."
                  : "Loading…",
                icon: "circle",
              },
            ],
      };

      const channelsGroup: TreeNode = {
        id: `channels:${inst.id}`,
        label: "Channels",
        icon: "menu",
        children: channelChildren.length
          ? channelChildren
          : [
              {
                id: `channels-empty:${inst.id}`,
                label: channelsResp
                  ? channelsResp.error ?? "No channels declared."
                  : "Loading…",
                icon: "circle",
              },
            ],
      };

      return {
        id: `inst:${inst.id}`,
        label: inst.name,
        icon: "box",
        meta: inst.transport,
        children: [agentsGroup, channelsGroup],
      };
    });
  };

  return (
    <Show
      when={props.instances.length > 0}
      fallback={
        <div
          style={{
            padding: "8px 10px",
            "font-size": "12px",
            color: "var(--fg-2)",
            "font-style": "italic",
          }}
        >
          No instances registered yet.
        </div>
      }
    >
      <Tree
        nodes={buildNodes()}
        expanded={expanded()}
        selectedId={selectedTreeId()}
        onToggle={toggle}
        onSelect={handleSelect}
      />
    </Show>
  );
}
