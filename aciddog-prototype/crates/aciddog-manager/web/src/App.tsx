import { Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import AppShell from "./AppShell";
import LoginPage from "./components/LoginPage";
import { authApi } from "./auth";

export default function App(): JSX.Element {
  const [username, setUsername] = createSignal<string | null>(null);
  const [bootstrapped, setBootstrapped] = createSignal(false);

  onMount(async () => {
    try {
      const resp = await authApi.whoami();
      setUsername(resp.username);
    } catch {
      // not logged in — fall through to LoginPage
    } finally {
      setBootstrapped(true);
    }

    const handleLogout = () => setUsername(null);
    window.addEventListener("auth:logout", handleLogout);
    onCleanup(() => window.removeEventListener("auth:logout", handleLogout));
  });

  return (
    <Show
      when={bootstrapped()}
      fallback={<div class="min-h-screen p-6 text-slate-400">Loading…</div>}
    >
      <Show
        when={username()}
        fallback={<LoginPage onLoggedIn={(name) => setUsername(name)} />}
      >
        {(name) => (
          <AppShell username={name()} onLoggedOut={() => setUsername(null)} />
        )}
      </Show>
    </Show>
  );
}
