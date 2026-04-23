import { Show, createSignal, type JSX } from "solid-js";
import { api } from "../api";
import type { CreateInstanceInput, InstanceSummary, Transport } from "../types";

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

  const inputCls =
    "w-full rounded bg-slate-800 border border-slate-700 px-2 py-1 text-sm focus:outline-none focus:border-slate-500";
  const labelCls = "block text-xs uppercase text-slate-400 mb-1 tracking-wider";

  return (
    <form onSubmit={submit} class="space-y-3">
      <div>
        <label class={labelCls}>Name</label>
        <input
          class={inputCls}
          required
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          placeholder="my-aciddog"
        />
      </div>

      <div>
        <label class={labelCls}>Transport</label>
        <select
          class={inputCls}
          value={transport()}
          onChange={(e) => setTransport(e.currentTarget.value as Transport)}
        >
          <option value="unix">Unix socket</option>
          <option value="https">HTTPS</option>
        </select>
      </div>

      <Show when={transport() === "unix"}>
        <div>
          <label class={labelCls}>Socket path</label>
          <input
            class={inputCls}
            required
            value={socketPath()}
            onInput={(e) => setSocketPath(e.currentTarget.value)}
            placeholder="/tmp/aciddog-remote-demo.sock"
          />
        </div>
      </Show>

      <Show when={transport() === "https"}>
        <div>
          <label class={labelCls}>URL</label>
          <input
            class={inputCls}
            required
            value={url()}
            onInput={(e) => setUrl(e.currentTarget.value)}
            placeholder="https://host:8443"
          />
        </div>
        <div>
          <label class={labelCls}>API key</label>
          <input
            class={inputCls}
            type="password"
            value={apiKey()}
            onInput={(e) => setApiKey(e.currentTarget.value)}
            placeholder="optional"
          />
        </div>
        <div>
          <label class={labelCls}>TLS CA path</label>
          <input
            class={inputCls}
            value={tlsCa()}
            onInput={(e) => setTlsCa(e.currentTarget.value)}
            placeholder="optional, e.g. ./ca.pem"
          />
        </div>
      </Show>

      <Show when={error()}>
        <div class="rounded bg-red-900/40 border border-red-800 px-2 py-1 text-sm text-red-200">
          {error()}
        </div>
      </Show>

      <button
        type="submit"
        disabled={submitting()}
        class="w-full rounded bg-emerald-700 hover:bg-emerald-600 disabled:opacity-50 px-3 py-1.5 text-sm font-medium"
      >
        {submitting() ? "Adding…" : "Add instance"}
      </button>
    </form>
  );
}
