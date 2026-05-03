import { request } from "./api";

export interface WhoamiResponse {
  username: string;
  /// Capability gate: true when the manager process was started with
  /// `--enable-daemon-runner` (or the env equivalent). Server-side
  /// field is already populated per decisions/154e9a7d (sibling
  /// 33fc8b7b, merged at b820a48). Frontend MUST default to `false`
  /// when absent — fail-closed gate.
  is_daemon_runner_enabled: boolean;
}

export const authApi = {
  whoami: () => request<WhoamiResponse>("/api/auth/whoami"),
  login: (username: string, password: string) =>
    request<WhoamiResponse>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request<void>("/api/auth/logout", { method: "POST" }),
  changeMyPassword: (current_password: string, new_password: string) =>
    request<void>("/api/auth/password", {
      method: "POST",
      body: JSON.stringify({ current_password, new_password }),
    }),
};
