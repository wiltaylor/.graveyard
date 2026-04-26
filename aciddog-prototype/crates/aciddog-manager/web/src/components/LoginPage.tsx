import { createSignal, Show, type JSX } from "solid-js";
import { authApi } from "../auth";
import { UnauthorizedError } from "../errors";
import { Button, Card, Input, Toast } from "./ui";

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
    <div class="min-h-screen flex items-center justify-center bg-bg-0">
      <form
        onSubmit={handleSubmit}
        style={{
          width: "100%",
          "max-width": "360px",
        }}
      >
        <Card>
          <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
            <div>
              <h1
                style={{
                  "font-size": "18px",
                  "font-weight": "600",
                  color: "var(--fg-0)",
                  margin: "0",
                }}
              >
                aciddog manager
              </h1>
              <p
                style={{
                  "font-size": "12px",
                  color: "var(--fg-2)",
                  margin: "4px 0 0",
                }}
              >
                Sign in to continue.
              </p>
            </div>

            <Input
              label="Username"
              type="text"
              autocomplete="username"
              required
              icon="user"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
            />
            <Input
              label="Password"
              type="password"
              autocomplete="current-password"
              required
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
            />

            <Show when={error()}>
              <Toast tone="danger">{error()}</Toast>
            </Show>

            <Button
              type="submit"
              variant="primary"
              size="lg"
              disabled={submitting()}
              class="w-full"
            >
              {submitting() ? "Signing in…" : "Sign in"}
            </Button>
          </div>
        </Card>
      </form>
    </div>
  );
}
