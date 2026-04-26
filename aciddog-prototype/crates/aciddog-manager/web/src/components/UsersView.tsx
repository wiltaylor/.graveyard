import {
  For,
  Show,
  createResource,
  createSignal,
  type JSX,
} from "solid-js";

import { api } from "../api";
import { authApi } from "../auth";
import type { UserSummary } from "../types";
import { Badge, Button, Card, Empty, Input, Toast } from "./ui";

interface Props {
  currentUsername: string;
}

function formatLastLogin(value: string | null): string {
  if (!value) return "never";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString();
}

export default function UsersView(props: Props): JSX.Element {
  const [users, { refetch }] = createResource<UserSummary[]>(
    () => api.listUsers(),
    { initialValue: [] },
  );

  const [newUsername, setNewUsername] = createSignal("");
  const [newPassword, setNewPassword] = createSignal("");
  const [addError, setAddError] = createSignal<string | null>(null);
  const [adding, setAdding] = createSignal(false);

  const [rowError, setRowError] = createSignal<{
    id: string;
    message: string;
  } | null>(null);
  const [resettingId, setResettingId] = createSignal<string | null>(null);
  const [resetPasswordValue, setResetPasswordValue] = createSignal("");
  const [busyId, setBusyId] = createSignal<string | null>(null);

  const [currentPw, setCurrentPw] = createSignal("");
  const [newPw, setNewPw] = createSignal("");
  const [confirmPw, setConfirmPw] = createSignal("");
  const [changeError, setChangeError] = createSignal<string | null>(null);
  const [changeSuccess, setChangeSuccess] = createSignal(false);
  const [changing, setChanging] = createSignal(false);

  const addUser = async (e: SubmitEvent) => {
    e.preventDefault();
    setAddError(null);
    setAdding(true);
    try {
      await api.createUser({
        username: newUsername().trim(),
        password: newPassword(),
      });
      setNewUsername("");
      setNewPassword("");
      await refetch();
    } catch (err) {
      setAddError(err instanceof Error ? err.message : String(err));
    } finally {
      setAdding(false);
    }
  };

  const deleteUser = async (u: UserSummary) => {
    if (!window.confirm(`Delete user "${u.username}"?`)) return;
    setRowError(null);
    setBusyId(u.id);
    try {
      await api.deleteUser(u.id);
      await refetch();
    } catch (err) {
      setRowError({
        id: u.id,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setBusyId(null);
    }
  };

  const submitReset = async (u: UserSummary, e: SubmitEvent) => {
    e.preventDefault();
    setRowError(null);
    setBusyId(u.id);
    try {
      await api.resetUserPassword(u.id, resetPasswordValue());
      setResettingId(null);
      setResetPasswordValue("");
    } catch (err) {
      setRowError({
        id: u.id,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setBusyId(null);
    }
  };

  const submitChangePassword = async (e: SubmitEvent) => {
    e.preventDefault();
    setChangeError(null);
    setChangeSuccess(false);
    if (newPw() !== confirmPw()) {
      setChangeError("New passwords do not match.");
      return;
    }
    if (!currentPw() || !newPw()) {
      setChangeError("Current and new password are required.");
      return;
    }
    setChanging(true);
    try {
      await authApi.changeMyPassword(currentPw(), newPw());
      setCurrentPw("");
      setNewPw("");
      setConfirmPw("");
      setChangeSuccess(true);
      window.setTimeout(() => setChangeSuccess(false), 3000);
    } catch (err) {
      setChangeError(err instanceof Error ? err.message : String(err));
    } finally {
      setChanging(false);
    }
  };

  const userCount = () => users()?.length ?? 0;
  const isSelf = (u: UserSummary) => u.username === props.currentUsername;
  const canDelete = (u: UserSummary) => !isSelf(u) && userCount() > 1;

  return (
    <div style={{ "max-width": "780px", display: "flex", "flex-direction": "column", gap: "20px" }}>
      <header>
        <h2
          style={{
            "font-size": "22px",
            "font-weight": "600",
            color: "var(--fg-0)",
            "letter-spacing": "-0.01em",
            margin: "0",
          }}
        >
          Web UI users
        </h2>
        <p style={{ color: "var(--fg-2)", "font-size": "13px", margin: "6px 0 0" }}>
          Accounts that can sign in to this manager portal.
        </p>
      </header>

      <Card title="Add user">
        <form onSubmit={addUser} style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <div style={{ display: "grid", "grid-template-columns": "1fr 1fr", gap: "12px" }}>
            <Input
              label="Username"
              required
              value={newUsername()}
              onInput={(e) => setNewUsername(e.currentTarget.value)}
              placeholder="alice"
              autocomplete="off"
            />
            <Input
              label="Password"
              required
              type="password"
              value={newPassword()}
              onInput={(e) => setNewPassword(e.currentTarget.value)}
              placeholder="•••••••"
              autocomplete="new-password"
            />
          </div>
          <Show when={addError()}>
            <Toast tone="danger">{addError()}</Toast>
          </Show>
          <div>
            <Button type="submit" variant="primary" icon="plus" disabled={adding()}>
              {adding() ? "Adding…" : "Add user"}
            </Button>
          </div>
        </form>
      </Card>

      <Card title="Existing users" padded={false}>
        <Show
          when={!users.loading}
          fallback={
            <div style={{ padding: "16px", "font-size": "13px", color: "var(--fg-2)" }}>
              Loading…
            </div>
          }
        >
          <Show
            when={userCount() > 0}
            fallback={
              <div style={{ padding: "20px" }}>
                <Empty>No users yet — add one above.</Empty>
              </div>
            }
          >
            <table class="ftable">
              <thead>
                <tr>
                  <th>Username</th>
                  <th>Last login</th>
                  <th style={{ "text-align": "right" }}>Actions</th>
                </tr>
              </thead>
              <tbody>
                <For each={users() ?? []}>
                  {(u) => (
                    <>
                      <tr>
                        <td>
                          {u.username}
                          <Show when={isSelf(u)}>
                            {" "}
                            <Badge tone="accent">you</Badge>
                          </Show>
                        </td>
                        <td class="col-mono">{formatLastLogin(u.last_login)}</td>
                        <td style={{ "text-align": "right" }}>
                          <span style={{ display: "inline-flex", gap: "6px" }}>
                            <Button
                              size="sm"
                              variant="secondary"
                              onClick={() => {
                                setRowError(null);
                                setResettingId(
                                  resettingId() === u.id ? null : u.id,
                                );
                                setResetPasswordValue("");
                              }}
                            >
                              {resettingId() === u.id ? "Cancel" : "Reset password"}
                            </Button>
                            <Show when={canDelete(u)}>
                              <Button
                                size="sm"
                                variant="danger"
                                disabled={busyId() === u.id}
                                onClick={() => deleteUser(u)}
                              >
                                Delete
                              </Button>
                            </Show>
                          </span>
                        </td>
                      </tr>
                      <Show when={resettingId() === u.id}>
                        <tr style={{ background: "var(--bg-2)" }}>
                          <td colspan={3} style={{ padding: "14px" }}>
                            <form
                              onSubmit={(e) => submitReset(u, e)}
                              style={{
                                display: "flex",
                                "align-items": "flex-end",
                                gap: "8px",
                              }}
                            >
                              <div style={{ flex: "1" }}>
                                <Input
                                  label={`New password for ${u.username}`}
                                  required
                                  type="password"
                                  value={resetPasswordValue()}
                                  onInput={(e) =>
                                    setResetPasswordValue(e.currentTarget.value)
                                  }
                                  autocomplete="new-password"
                                />
                              </div>
                              <Button
                                type="submit"
                                variant="primary"
                                disabled={busyId() === u.id}
                              >
                                {busyId() === u.id ? "Saving…" : "Set password"}
                              </Button>
                            </form>
                          </td>
                        </tr>
                      </Show>
                      <Show when={rowError()?.id === u.id}>
                        <tr>
                          <td colspan={3} style={{ padding: "8px 14px" }}>
                            <Toast tone="danger">{rowError()?.message}</Toast>
                          </td>
                        </tr>
                      </Show>
                    </>
                  )}
                </For>
              </tbody>
            </table>
          </Show>
        </Show>
      </Card>

      <Card title="Change my password">
        <form
          onSubmit={submitChangePassword}
          style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
        >
          <Input
            label="Current password"
            required
            type="password"
            value={currentPw()}
            onInput={(e) => setCurrentPw(e.currentTarget.value)}
            autocomplete="current-password"
          />
          <div style={{ display: "grid", "grid-template-columns": "1fr 1fr", gap: "12px" }}>
            <Input
              label="New password"
              required
              type="password"
              value={newPw()}
              onInput={(e) => setNewPw(e.currentTarget.value)}
              autocomplete="new-password"
            />
            <Input
              label="Confirm new password"
              required
              type="password"
              value={confirmPw()}
              onInput={(e) => setConfirmPw(e.currentTarget.value)}
              autocomplete="new-password"
            />
          </div>
          <Show when={changeError()}>
            <Toast tone="danger">{changeError()}</Toast>
          </Show>
          <Show when={changeSuccess()}>
            <Toast tone="success">Password updated.</Toast>
          </Show>
          <div>
            <Button type="submit" variant="primary" disabled={changing()}>
              {changing() ? "Updating…" : "Update password"}
            </Button>
          </div>
        </form>
      </Card>
    </div>
  );
}

