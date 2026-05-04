import { Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import AppShell from "./AppShell";
import LoginPage from "./components/LoginPage";
import { authApi } from "./auth";

export default function App(): JSX.Element {
  const [username, setUsername] = createSignal<string | null>(null);
  const [bootstrapped, setBootstrapped] = createSignal(false);

  const captureWhoami = (resp: { username: string }) => {
    setUsername(resp.username);
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
    };
    window.addEventListener("auth:logout", handleLogout);
    onCleanup(() => window.removeEventListener("auth:logout", handleLogout));
  });

  const handleLoggedIn = async (name: string) => {
    setUsername(name);
    try {
      const resp = await authApi.whoami();
      captureWhoami(resp);
    } catch {
      setUsername(null);
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
            onLoggedOut={() => {
              setUsername(null);
            }}
          />
        )}
      </Show>
    </Show>
  );
}
