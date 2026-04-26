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

interface Props {
  currentUsername: string;
}

const inputCls =
  "w-full rounded bg-slate-800 border border-slate-700 px-2 py-1 text-sm focus:outline-none focus:border-slate-500";
const labelCls = "block text-xs uppercase text-slate-400 mb-1 tracking-wider";
const sectionCls =
  "rounded border border-slate-800 bg-slate-900/40 p-4 space-y-3";
const sectionHeader = "text-sm font-semibold text-slate-200";
const errorCls =
  "rounded bg-red-900/40 border border-red-800 px-2 py-1 text-sm text-red-200";
const successCls =
  "rounded bg-emerald-900/40 border border-emerald-800 px-2 py-1 text-sm text-emerald-200";

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

  // Add-user form state
  const [newUsername, setNewUsername] = createSignal("");
  const [newPassword, setNewPassword] = createSignal("");
  const [addError, setAddError] = createSignal<string | null>(null);
  const [adding, setAdding] = createSignal(false);

  // Per-row state
  const [rowError, setRowError] = createSignal<{
    id: string;
    message: string;
  } | null>(null);
  const [resettingId, setResettingId] = createSignal<string | null>(null);
  const [resetPasswordValue, setResetPasswordValue] = createSignal("");
  const [busyId, setBusyId] = createSignal<string | null>(null);

  // Change-my-password state
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
    <div class="h-full overflow-y-auto p-6">
      <div class="max-w-3xl space-y-6">
        <header>
          <h2 class="text-lg font-semibold text-slate-100">Web UI users</h2>
          <p class="text-xs text-slate-400 mt-1">
            Accounts that can sign in to this manager portal.
          </p>
        </header>

        <section class={sectionCls}>
          <h3 class={sectionHeader}>Add user</h3>
          <form onSubmit={addUser} class="space-y-3">
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class={labelCls}>Username</label>
                <input
                  class={inputCls}
                  required
                  value={newUsername()}
                  onInput={(e) => setNewUsername(e.currentTarget.value)}
                  placeholder="alice"
                  autocomplete="off"
                />
              </div>
              <div>
                <label class={labelCls}>Password</label>
                <input
                  class={inputCls}
                  required
                  type="password"
                  value={newPassword()}
                  onInput={(e) => setNewPassword(e.currentTarget.value)}
                  placeholder="•••••••"
                  autocomplete="new-password"
                />
              </div>
            </div>
            <Show when={addError()}>
              <div class={errorCls}>{addError()}</div>
            </Show>
            <button
              type="submit"
              disabled={adding()}
              class="rounded bg-emerald-700 hover:bg-emerald-600 disabled:opacity-50 px-3 py-1.5 text-sm font-medium"
            >
              {adding() ? "Adding…" : "Add user"}
            </button>
          </form>
        </section>

        <section class={sectionCls}>
          <h3 class={sectionHeader}>Existing users</h3>
          <Show
            when={!users.loading}
            fallback={<div class="text-sm text-slate-500">Loading…</div>}
          >
            <Show
              when={userCount() > 0}
              fallback={
                <div class="text-sm text-slate-500 italic">
                  No users yet — add one above.
                </div>
              }
            >
              <table class="w-full text-sm">
                <thead>
                  <tr class="text-left text-xs uppercase text-slate-400 tracking-wider border-b border-slate-800">
                    <th class="py-2 pr-2">Username</th>
                    <th class="py-2 pr-2">Last login</th>
                    <th class="py-2 text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={users() ?? []}>
                    {(u) => (
                      <>
                        <tr class="border-b border-slate-800/50">
                          <td class="py-2 pr-2 text-slate-100">
                            {u.username}
                            <Show when={isSelf(u)}>
                              <span class="ml-2 text-xs text-sky-400">
                                (you)
                              </span>
                            </Show>
                          </td>
                          <td class="py-2 pr-2 text-slate-400">
                            {formatLastLogin(u.last_login)}
                          </td>
                          <td class="py-2 text-right space-x-2">
                            <button
                              type="button"
                              onClick={() => {
                                setRowError(null);
                                setResettingId(
                                  resettingId() === u.id ? null : u.id,
                                );
                                setResetPasswordValue("");
                              }}
                              class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-200 hover:bg-slate-800"
                            >
                              {resettingId() === u.id
                                ? "Cancel"
                                : "Reset password"}
                            </button>
                            <Show when={canDelete(u)}>
                              <button
                                type="button"
                                disabled={busyId() === u.id}
                                onClick={() => deleteUser(u)}
                                class="rounded border border-red-800 bg-red-900/40 px-2 py-1 text-xs text-red-200 hover:bg-red-900/60 disabled:opacity-50"
                              >
                                Delete
                              </button>
                            </Show>
                          </td>
                        </tr>
                        <Show when={resettingId() === u.id}>
                          <tr class="border-b border-slate-800/50 bg-slate-900/40">
                            <td colspan={3} class="py-3 px-2">
                              <form
                                onSubmit={(e) => submitReset(u, e)}
                                class="flex items-end gap-2"
                              >
                                <div class="flex-1">
                                  <label class={labelCls}>
                                    New password for {u.username}
                                  </label>
                                  <input
                                    class={inputCls}
                                    required
                                    type="password"
                                    value={resetPasswordValue()}
                                    onInput={(e) =>
                                      setResetPasswordValue(
                                        e.currentTarget.value,
                                      )
                                    }
                                    autocomplete="new-password"
                                  />
                                </div>
                                <button
                                  type="submit"
                                  disabled={busyId() === u.id}
                                  class="rounded bg-emerald-700 hover:bg-emerald-600 disabled:opacity-50 px-3 py-1.5 text-sm font-medium"
                                >
                                  {busyId() === u.id
                                    ? "Saving…"
                                    : "Set password"}
                                </button>
                              </form>
                            </td>
                          </tr>
                        </Show>
                        <Show when={rowError()?.id === u.id}>
                          <tr>
                            <td colspan={3} class="py-2 px-2">
                              <div class={errorCls}>{rowError()?.message}</div>
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
        </section>

        <section class={sectionCls}>
          <h3 class={sectionHeader}>Change my password</h3>
          <form onSubmit={submitChangePassword} class="space-y-3">
            <div>
              <label class={labelCls}>Current password</label>
              <input
                class={inputCls}
                required
                type="password"
                value={currentPw()}
                onInput={(e) => setCurrentPw(e.currentTarget.value)}
                autocomplete="current-password"
              />
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class={labelCls}>New password</label>
                <input
                  class={inputCls}
                  required
                  type="password"
                  value={newPw()}
                  onInput={(e) => setNewPw(e.currentTarget.value)}
                  autocomplete="new-password"
                />
              </div>
              <div>
                <label class={labelCls}>Confirm new password</label>
                <input
                  class={inputCls}
                  required
                  type="password"
                  value={confirmPw()}
                  onInput={(e) => setConfirmPw(e.currentTarget.value)}
                  autocomplete="new-password"
                />
              </div>
            </div>
            <Show when={changeError()}>
              <div class={errorCls}>{changeError()}</div>
            </Show>
            <Show when={changeSuccess()}>
              <div class={successCls}>Password updated.</div>
            </Show>
            <button
              type="submit"
              disabled={changing()}
              class="rounded bg-emerald-700 hover:bg-emerald-600 disabled:opacity-50 px-3 py-1.5 text-sm font-medium"
            >
              {changing() ? "Updating…" : "Update password"}
            </button>
          </form>
        </section>
      </div>
    </div>
  );
}
