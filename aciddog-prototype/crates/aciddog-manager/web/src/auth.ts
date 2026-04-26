import { request } from "./api";

export interface WhoamiResponse {
  username: string;
}

export const authApi = {
  whoami: () => request<WhoamiResponse>("/api/auth/whoami"),
  login: (username: string, password: string) =>
    request<WhoamiResponse>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request<void>("/api/auth/logout", { method: "POST" }),
};
