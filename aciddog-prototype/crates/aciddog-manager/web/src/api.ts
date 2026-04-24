import type {
  AgentListResponse,
  ApiError,
  ChatHistoryResponse,
  CreateInstanceInput,
  HealthResponse,
  InboxListResponse,
  InstanceSummary,
  MutationResponse,
  SendResponse,
  TaskListResponse,
  TaskStatus,
  TodoStatus,
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

  // Tasks
  listTasks: (id: string) =>
    request<TaskListResponse>(`/api/instances/${id}/tasks`),
  createTask: (
    id: string,
    body: {
      title: string;
      description?: string;
      assignee?: string;
      blocked_by?: string[];
    },
  ) =>
    request<MutationResponse>(`/api/instances/${id}/tasks`, {
      method: "POST",
      body: JSON.stringify(body),
    }),
  patchTask: (
    id: string,
    taskId: string,
    body: {
      title?: string;
      description?: string;
      status?: TaskStatus;
      assignee?: string;
      unassign?: boolean;
      note?: string;
      add_blocked_by?: string[];
      remove_blocked_by?: string[];
    },
  ) =>
    request<MutationResponse>(`/api/instances/${id}/tasks/${taskId}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteTask: (id: string, taskId: string) =>
    request<MutationResponse>(`/api/instances/${id}/tasks/${taskId}`, {
      method: "DELETE",
    }),
  addTaskNote: (id: string, taskId: string, body: string) =>
    request<MutationResponse>(`/api/instances/${id}/tasks/${taskId}/notes`, {
      method: "POST",
      body: JSON.stringify({ body }),
    }),
  addTodo: (id: string, taskId: string, body: string) =>
    request<MutationResponse>(`/api/instances/${id}/tasks/${taskId}/todos`, {
      method: "POST",
      body: JSON.stringify({ body }),
    }),
  patchTodo: (
    id: string,
    taskId: string,
    todoId: string,
    body: { status?: TodoStatus; body?: string },
  ) =>
    request<MutationResponse>(
      `/api/instances/${id}/tasks/${taskId}/todos/${todoId}`,
      { method: "PATCH", body: JSON.stringify(body) },
    ),
  deleteTodo: (id: string, taskId: string, todoId: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/tasks/${taskId}/todos/${todoId}`,
      { method: "DELETE" },
    ),
  addComment: (id: string, taskId: string, body: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/tasks/${taskId}/comments`,
      { method: "POST", body: JSON.stringify({ body }) },
    ),
  patchComment: (id: string, taskId: string, commentId: string, body: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/tasks/${taskId}/comments/${commentId}`,
      { method: "PATCH", body: JSON.stringify({ body }) },
    ),
  deleteComment: (id: string, taskId: string, commentId: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/tasks/${taskId}/comments/${commentId}`,
      { method: "DELETE" },
    ),

  // Inbox
  listInbox: (id: string) =>
    request<InboxListResponse>(`/api/instances/${id}/inbox`),
  deleteInboxMessage: (id: string, index: number) =>
    request<MutationResponse>(`/api/instances/${id}/inbox/${index}`, {
      method: "DELETE",
    }),
  replyInbox: (id: string, index: number, body: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/inbox/${index}/reply`,
      { method: "POST", body: JSON.stringify({ body }) },
    ),
  forwardInbox: (id: string, index: number, to: string, body: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/inbox/${index}/forward`,
      { method: "POST", body: JSON.stringify({ to, body }) },
    ),
};

export function instanceEventsUrl(id: string): string {
  return `/api/instances/${id}/events`;
}

/// Path for an EventSource that streams `ChatBlockEvent`s for a single
/// agent. Exposed here so callers don't hand-build the URL.
export function chatEventsUrl(id: string, agent: string): string {
  return `/api/instances/${id}/agents/${encodeURIComponent(agent)}/events`;
}
