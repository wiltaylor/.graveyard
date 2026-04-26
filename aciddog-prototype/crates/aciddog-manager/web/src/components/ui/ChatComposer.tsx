import { Show, type JSX } from "solid-js";

import Icon from "./Icon";
import Toast from "./Toast";

interface Props {
  value: string;
  onInput: (v: string) => void;
  onSubmit: () => void;
  sending: boolean;
  error?: string | null;
  placeholder?: string;
}

export default function ChatComposer(props: Props): JSX.Element {
  const submit = (e: SubmitEvent) => {
    e.preventDefault();
    if (props.sending || props.value.trim() === "") return;
    props.onSubmit();
  };

  return (
    <form class="chat-composer" onSubmit={submit}>
      <Show when={props.error}>
        <Toast tone="danger">{props.error}</Toast>
      </Show>
      <div class="chat-composer-row">
        <textarea
          value={props.value}
          onInput={(e) => props.onInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              (e.currentTarget.form as HTMLFormElement).requestSubmit();
            }
          }}
          placeholder={props.placeholder}
          rows={1}
        />
        <button
          type="submit"
          class="btn primary"
          disabled={props.sending || props.value.trim() === ""}
        >
          <Icon name="arrow-right" size={12} />
          {props.sending ? "Sending…" : "Send"}
        </button>
      </div>
    </form>
  );
}
