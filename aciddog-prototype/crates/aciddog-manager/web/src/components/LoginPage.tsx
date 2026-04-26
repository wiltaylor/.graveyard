import { createSignal, Show, type JSX } from "solid-js";
import { authApi } from "../auth";
import { UnauthorizedError } from "../errors";

interface Props {
  onLoggedIn: (username: string) => void;
}

export default function LoginPage(props: Props): JSX.Element {
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const resp = await authApi.login(username(), password());
      props.onLoggedIn(resp.username);
    } catch (err) {
      if (err instanceof UnauthorizedError) {
        setError("Invalid username or password.");
      } else {
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div class="min-h-screen flex items-center justify-center bg-slate-950 text-slate-100">
      <form
        onSubmit={handleSubmit}
        class="w-full max-w-sm rounded border border-slate-800 bg-slate-900/80 p-6 shadow-lg space-y-4"
      >
        <div>
          <h1 class="text-lg font-semibold">aciddog manager</h1>
          <p class="text-xs text-slate-400">Sign in to continue.</p>
        </div>

        <label class="block">
          <span class="text-xs uppercase tracking-wider text-slate-400">
            Username
          </span>
          <input
            type="text"
            autocomplete="username"
            required
            value={username()}
            onInput={(e) => setUsername(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-3 py-2 text-sm focus:border-sky-500 focus:outline-none"
          />
        </label>

        <label class="block">
          <span class="text-xs uppercase tracking-wider text-slate-400">
            Password
          </span>
          <input
            type="password"
            autocomplete="current-password"
            required
            value={password()}
            onInput={(e) => setPassword(e.currentTarget.value)}
            class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-3 py-2 text-sm focus:border-sky-500 focus:outline-none"
          />
        </label>

        <Show when={error()}>
          <div class="rounded border border-red-800 bg-red-950/60 px-3 py-2 text-sm text-red-300">
            {error()}
          </div>
        </Show>

        <button
          type="submit"
          disabled={submitting()}
          class="w-full rounded bg-sky-600 px-3 py-2 text-sm font-medium text-white hover:bg-sky-500 disabled:cursor-not-allowed disabled:opacity-60"
        >
          {submitting() ? "Signing in…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}
