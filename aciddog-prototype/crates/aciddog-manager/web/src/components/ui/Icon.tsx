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

export default function Icon(props: Props): JSX.Element {
  const size = () => props.size ?? 16;
  return (
    <img
      src={`/icons/${props.name}.svg`}
      width={size()}
      height={size()}
      alt=""
      aria-hidden="true"
      class={props.class}
      style={
        typeof props.style === "string"
          ? props.style
          : { display: "inline-block", "vertical-align": "middle", ...props.style }
      }
    />
  );
}
