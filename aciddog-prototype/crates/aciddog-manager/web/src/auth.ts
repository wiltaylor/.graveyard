import { request } from "./api";

export interface WhoamiResponse {
  username: string;
  // @deprecated — kept for one release per ADR-0008 (always true). Will be removed in the follow-up release.
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
