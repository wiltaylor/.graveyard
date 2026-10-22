interface SpinnerProps {
  size?: number;
  label?: string;
  class?: string;
}

export default function Spinner(props: SpinnerProps) {
  return (
    <span
      role="status"
      aria-label={props.label ?? "Loading"}
      class={"spinner" + (props.class ? " " + props.class : "")}
      style={{ "--size": (props.size ?? 16) + "px" }}
    >
      <span class="spinner-ring" aria-hidden="true" />
    </span>
  );
}
