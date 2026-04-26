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

      const agentChildren: TreeNode[] = agentsResp?.ok
        ? agentsResp.agents.map((agent) => ({
            id: `agent:${inst.id}:${agent.id}`,
            label: agent.id,
            icon: "user",
            meta: agent.status,
          }))
        : [];

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
