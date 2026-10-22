import { Show, createSignal, type JSX } from "solid-js";
import { api } from "../api";
import type { CreateInstanceInput, InstanceSummary, Transport } from "../types";
import { Button, Input, Select, Toast } from "./ui";

interface Props {
  onCreated: (inst: InstanceSummary) => void;
}

export default function AddInstanceForm(props: Props): JSX.Element {
  const [name, setName] = createSignal("");
  const [transport, setTransport] = createSignal<Transport>("unix");
  const [url, setUrl] = createSignal("");
  const [socketPath, setSocketPath] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [tlsCa, setTlsCa] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const reset = () => {
    setName("");
    setTransport("unix");
    setUrl("");
    setSocketPath("");
    setApiKey("");
    setTlsCa("");
    setError(null);
  };

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const payload: CreateInstanceInput = {
        name: name().trim(),
        transport: transport(),
      };
      if (transport() === "https") {
        payload.url = url().trim();
        if (apiKey().trim()) payload.api_key = apiKey().trim();
        if (tlsCa().trim()) payload.tls_ca_path = tlsCa().trim();
      } else {
        payload.socket_path = socketPath().trim();
      }
      const created = await api.createInstance(payload);
      props.onCreated(created);
      reset();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form
      onSubmit={submit}
      style={{ display: "flex", "flex-direction": "column", gap: "10px" }}
    >
      <Input
        label="Name"
        required
        value={name()}
        onInput={(e) => setName(e.currentTarget.value)}
        placeholder="my-aciddog"
      />

      <Select<Transport>
        label="Transport"
        value={transport()}
        onChange={(v) => setTransport(v)}
        options={[
          { value: "unix", label: "Unix socket" },
          { value: "https", label: "HTTPS" },
        ]}
      />

      <Show when={transport() === "unix"}>
        <Input
          label="Socket path"
          required
          value={socketPath()}
          onInput={(e) => setSocketPath(e.currentTarget.value)}
          placeholder="/tmp/aciddog-remote-demo.sock"
        />
      </Show>

      <Show when={transport() === "https"}>
        <Input
          label="URL"
          required
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
          placeholder="https://host:8443"
        />
        <Input
          label="API key"
          type="password"
          value={apiKey()}
          onInput={(e) => setApiKey(e.currentTarget.value)}
          placeholder="optional"
        />
        <Input
          label="TLS CA path"
          value={tlsCa()}
          onInput={(e) => setTlsCa(e.currentTarget.value)}
          placeholder="optional, e.g. ./ca.pem"
        />
      </Show>

      <Show when={error()}>
        <Toast tone="danger">{error()}</Toast>
      </Show>

      <Button
        type="submit"
        variant="primary"
        size="md"
        icon="plus"
        disabled={submitting()}
        class="w-full"
      >
        {submitting() ? "Adding…" : "Add instance"}
      </Button>
    </form>
  );
}
