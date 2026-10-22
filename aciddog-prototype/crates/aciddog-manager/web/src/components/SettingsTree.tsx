import { createSignal, type JSX } from "solid-js";
import { Tree } from "./ui";
import type { TreeNode } from "./ui";

export type SettingsNodeId = "users" | "skills";

interface Props {
  selected: SettingsNodeId | null;
  onSelect: (id: SettingsNodeId) => void;
}

const NODES: TreeNode[] = [
  {
    id: "web-ui",
    label: "Web UI",
    icon: "settings",
    children: [
      { id: "users", label: "Users", icon: "users" },
      { id: "skills", label: "Skills", icon: "box" },
    ],
  },
];

export default function SettingsTree(props: Props): JSX.Element {
  const [expanded, setExpanded] = createSignal(new Set(["web-ui"]));

  const toggle = (id: string) => {
    const next = new Set(expanded());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setExpanded(next);
  };

  const handleSelect = (id: string) => {
    if (id === "users" || id === "skills") props.onSelect(id);
  };

  return (
    <Tree
      nodes={NODES}
      expanded={expanded()}
      selectedId={props.selected}
      onToggle={toggle}
      onSelect={handleSelect}
    />
  );
}
