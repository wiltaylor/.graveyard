import type {
  AgentListResponse,
  ApiError,
  ChatHistoryResponse,
  CreateInstanceInput,
  HealthResponse,
  InstanceSummary,
  SendResponse,
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

  chatHistory: (id: string, agent: string) =>
    request<ChatHistoryResponse>(
      `/api/instances/${id}/agents/${encodeURIComponent(agent)}/history`,
    ),

  sendAsUser: (id: string, agent: string, body: string) =>
    request<SendResponse>(
      `/api/instances/${id}/agents/${encodeURIComponent(agent)}/messages`,
      { method: "POST", body: JSON.stringify({ body }) },
    ),
};

/// Path for an EventSource that streams `ChatBlockEvent`s for a single
/// agent. Exposed here so callers don't hand-build the URL.
export function chatEventsUrl(id: string, agent: string): string {
  return `/api/instances/${id}/agents/${encodeURIComponent(agent)}/events`;
}
