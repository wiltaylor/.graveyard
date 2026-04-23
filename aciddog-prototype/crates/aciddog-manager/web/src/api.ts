import type {
  AgentListResponse,
  ApiError,
  CreateInstanceInput,
  HealthResponse,
  InstanceSummary,
} from "./types";

async function request<T>(input: RequestInfo, init?: RequestInit): Promise<T> {
  const res = await fetch(input, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (res.status === 204) {
    return undefined as T;
  }
  const text = await res.text();
  const body = text ? (JSON.parse(text) as unknown) : null;
  if (!res.ok) {
    const err = (body as ApiError | null)?.error ?? `HTTP ${res.status}`;
    throw new Error(err);
  }
  return body as T;
}

export const api = {
  listInstances: () => request<InstanceSummary[]>("/api/instances"),

  createInstance: (input: CreateInstanceInput) =>
    request<InstanceSummary>("/api/instances", {
      method: "POST",
      body: JSON.stringify(input),
    }),

  deleteInstance: (id: string) =>
    request<void>(`/api/instances/${id}`, { method: "DELETE" }),

  listAgents: (id: string) =>
    request<AgentListResponse>(`/api/instances/${id}/agents`),

  health: (id: string) =>
    request<HealthResponse>(`/api/instances/${id}/health`),
};
