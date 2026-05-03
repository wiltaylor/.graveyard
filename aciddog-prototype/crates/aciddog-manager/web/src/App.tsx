import { Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import AppShell from "./AppShell";
import LoginPage from "./components/LoginPage";
import { authApi } from "./auth";

export default function App(): JSX.Element {
  const [username, setUsername] = createSignal<string | null>(null);
  // Capability gate: defaults to `false` (fail-closed) until whoami
  // resolves AND surfaces `is_daemon_runner_enabled: true`. Per
  // decisions/154e9a7d the field is always present on a successful
  // whoami response, but we still default to false so a missing /
  // legacy field can never accidentally light up the Daemons tab.
  const [isDaemonRunnerEnabled, setIsDaemonRunnerEnabled] = createSignal(false);
  const [bootstrapped, setBootstrapped] = createSignal(false);

  const captureWhoami = (resp: {
    username: string;
    is_daemon_runner_enabled?: boolean;
  }) => {
    setUsername(resp.username);
    setIsDaemonRunnerEnabled(resp.is_daemon_runner_enabled === true);
  };

  onMount(async () => {
    try {
      const resp = await authApi.whoami();
      captureWhoami(resp);
    } catch {
      // not logged in — fall through to LoginPage
    } finally {
      setBootstrapped(true);
    }

    const handleLogout = () => {
      setUsername(null);
      setIsDaemonRunnerEnabled(false);
    };
    window.addEventListener("auth:logout", handleLogout);
    onCleanup(() => window.removeEventListener("auth:logout", handleLogout));
  });

  const handleLoggedIn = async (name: string) => {
    setUsername(name);
    // Re-fetch whoami so the gate signal reflects the post-login session.
    try {
      const resp = await authApi.whoami();
      captureWhoami(resp);
    } catch {
      setIsDaemonRunnerEnabled(false);
    }
  };

  return (
    <Show
      when={bootstrapped()}
      fallback={<div class="min-h-screen p-6 text-slate-400">Loading…</div>}
    >
      <Show
        when={username()}
        fallback={<LoginPage onLoggedIn={handleLoggedIn} />}
      >
        {(name) => (
          <AppShell
            username={name()}
            isDaemonRunnerEnabled={isDaemonRunnerEnabled()}
            onLoggedOut={() => {
              setUsername(null);
              setIsDaemonRunnerEnabled(false);
            }}
          />
        )}
      </Show>
    </Show>
  );
}
