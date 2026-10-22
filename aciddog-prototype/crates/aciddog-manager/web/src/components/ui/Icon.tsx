import type { JSX } from "solid-js";

export type IconName =
  | "activity"
  | "alert-triangle"
  | "arrow-right"
  | "bell"
  | "box"
  | "check"
  | "chevron-down"
  | "chevron-right"
  | "circle"
  | "clock"
  | "cpu"
  | "database"
  | "download"
  | "eye"
  | "file-text"
  | "folder"
  | "git-branch"
  | "home"
  | "info"
  | "log-out"
  | "menu"
  | "moon"
  | "more-horizontal"
  | "pause"
  | "play"
  | "plus"
  | "refresh"
  | "search"
  | "settings"
  | "sun"
  | "terminal"
  | "upload"
  | "user"
  | "users"
  | "x"
  | "zap";

interface Props {
  name: IconName | (string & {});
  size?: number;
  class?: string;
  style?: JSX.CSSProperties | string;
}

/// Renders the icon as a CSS mask so it inherits `currentColor` from the
/// surrounding text — `<img>` SVGs run in their own document context and
/// don't pick up the parent's color, so masking is the only reliable way
/// to make Lucide icons retone with the design tokens.
export default function Icon(props: Props): JSX.Element {
  const size = () => props.size ?? 16;
  const url = () => `url(/icons/${props.name}.svg)`;
  const baseStyle: JSX.CSSProperties = {
    display: "inline-block",
    "vertical-align": "middle",
    "flex-shrink": "0",
    width: `${size()}px`,
    height: `${size()}px`,
    "background-color": "currentColor",
    "mask-image": url(),
    "-webkit-mask-image": url(),
    "mask-repeat": "no-repeat",
    "-webkit-mask-repeat": "no-repeat",
    "mask-position": "center",
    "-webkit-mask-position": "center",
    "mask-size": "contain",
    "-webkit-mask-size": "contain",
  };
  const merged: JSX.CSSProperties =
    typeof props.style === "object" && props.style != null
      ? { ...baseStyle, ...props.style }
      : baseStyle;
  return (
    <span
      role="img"
      aria-hidden="true"
      class={props.class}
      style={typeof props.style === "string" ? props.style : merged}
    />
  );
}
