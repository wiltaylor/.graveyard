import { UnauthorizedError } from "./errors";
import type {
  AgentListResponse,
  ApiError,
  BanksResponse,
  ChannelHistoryResponse,
  ChannelListResponse,
  ChatHistoryResponse,
  CreateCronRequest,
  CreateInstanceInput,
  CreateUserRequest,
  CronListResponse,
  GraphResponse,
  HealthResponse,
  InboxListResponse,
  InstanceSummary,
  LinksResponse,
  MemoryMutationResponse,
  MutationResponse,
  PatchCronRequest,
  MarketplaceAvailableSkill,
  MarketplaceProgressEvent,
  MarketplaceSummary,
  ReadWclResponse,
  ReloadResponse,
  RemoteSkillSummary,
  SearchResponse,
  SendResponse,
  SkillPushResult,
  SkillsListResponse,
  TaskListResponse,
  TaskStatus,
  TeamInfoResponse,
  TodoStatus,
  UserSummary,
  WireLinkDirection,
  WorkflowInstancesResponse,
  WorkflowTemplatesResponse,
} from "./types";

export async function request<T>(
  input: RequestInfo,
  init?: RequestInit,
): Promise<T> {
  const res = await fetch(input, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (res.status === 401) {
    // Anything other than the explicit auth probes treats 401 as "session
    // expired" — drop the SPA back to login without threading callbacks
    // through every call site.
    if (typeof window !== "undefined") {
      const url = typeof input === "string" ? input : input.url;
      if (!url.endsWith("/api/auth/whoami") && !url.endsWith("/api/auth/login")) {
        window.dispatchEvent(new CustomEvent("auth:logout"));
      }
    }
    throw new UnauthorizedError();
  }
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

  teamInfo: (id: string) =>
    request<TeamInfoResponse>(`/api/instances/${id}/team`),

  reloadTeam: (id: string) =>
    request<ReloadResponse>(`/api/instances/${id}/team/reload`, {
      method: "POST",
    }),

  readTeamWcl: (id: string) =>
    request<ReadWclResponse>(`/api/instances/${id}/team/wcl`),

  writeTeamWcl: (id: string, wcl: string) =>
    request<ReloadResponse>(`/api/instances/${id}/team/wcl`, {
      method: "POST",
      body: JSON.stringify({ wcl }),
    }),

  // Channels
  listChannels: (id: string) =>
    request<ChannelListResponse>(`/api/instances/${id}/channels`),
  channelHistory: (id: string, channel: string) =>
    request<ChannelHistoryResponse>(
      `/api/instances/${id}/channels/${encodeURIComponent(channel)}/history`,
    ),
  sendChannelMessage: (id: string, channel: string, body: string) =>
    request<MutationResponse>(
      `/api/instances/${id}/channels/${encodeURIComponent(channel)}/messages`,
      { method: "POST", body: JSON.stringify({ body }) },
    ),

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

  // Memory
  listMemoryBanks: (id: string) =>
    request<BanksResponse>(`/api/instances/${id}/memory/banks`),
  searchMemory: (
    id: string,
    bank: string,
    params: {
      q?: string;
      title_contains?: string;
      include_tags?: string[];
      exclude_tags?: string[];
      limit?: number;
    },
  ) => {
    const qs = new URLSearchParams();
    if (params.q) qs.set("q", params.q);
    if (params.title_contains)
      qs.set("title_contains", params.title_contains);
    if (params.include_tags && params.include_tags.length > 0)
      qs.set("include_tags", params.include_tags.join(","));
    if (params.exclude_tags && params.exclude_tags.length > 0)
      qs.set("exclude_tags", params.exclude_tags.join(","));
    if (params.limit) qs.set("limit", String(params.limit));
    const query = qs.toString();
    return request<SearchResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/search${
        query ? `?${query}` : ""
      }`,
    );
  },
  storeMemoryEntry: (
    id: string,
    bank: string,
    body: { body: string; title?: string; tags?: string[] },
  ) =>
    request<MemoryMutationResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/entries`,
      { method: "POST", body: JSON.stringify(body) },
    ),
  updateMemoryEntry: (
    id: string,
    bank: string,
    entryId: string,
    body: { body: string; title?: string; tags?: string[] },
  ) =>
    request<MemoryMutationResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/entries/${entryId}`,
      { method: "PATCH", body: JSON.stringify(body) },
    ),
  removeMemoryEntry: (id: string, bank: string, entryId: string) =>
    request<MemoryMutationResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/entries/${entryId}`,
      { method: "DELETE" },
    ),
  memoryEntryLinks: (
    id: string,
    bank: string,
    entryId: string,
    direction: WireLinkDirection = "both",
  ) =>
    request<LinksResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/entries/${entryId}/links?direction=${direction}`,
    ),
  linkMemory: (
    id: string,
    bank: string,
    body: { from: string; to: string; kind?: string },
  ) =>
    request<MemoryMutationResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/links`,
      { method: "POST", body: JSON.stringify(body) },
    ),
  unlinkMemory: (
    id: string,
    bank: string,
    body: { from: string; to: string },
  ) =>
    request<MemoryMutationResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/links`,
      { method: "DELETE", body: JSON.stringify(body) },
    ),
  memoryGraph: (id: string, bank: string) =>
    request<GraphResponse>(
      `/api/instances/${id}/memory/banks/${encodeURIComponent(bank)}/graph`,
    ),

  // Workflows
  listWorkflowTemplates: (id: string) =>
    request<WorkflowTemplatesResponse>(
      `/api/instances/${id}/workflows/templates`,
    ),
  listWorkflowInstances: (id: string) =>
    request<WorkflowInstancesResponse>(
      `/api/instances/${id}/workflows/instances`,
    ),
  startWorkflow: (
    id: string,
    body: { workflow: string; properties: Record<string, unknown> },
  ) =>
    request<MutationResponse>(`/api/instances/${id}/workflows/instances`, {
      method: "POST",
      body: JSON.stringify(body),
    }),
  dismissWorkflowInstance: (id: string, wfId: number) =>
    request<MutationResponse>(
      `/api/instances/${id}/workflows/instances/${wfId}`,
      { method: "DELETE" },
    ),

  // Cron
  listCron: (id: string) =>
    request<CronListResponse>(`/api/instances/${id}/cron`),
  createCron: (id: string, body: CreateCronRequest) =>
    request<MutationResponse>(`/api/instances/${id}/cron`, {
      method: "POST",
      body: JSON.stringify(body),
    }),
  patchCron: (id: string, jobId: string, body: PatchCronRequest) =>
    request<MutationResponse>(`/api/instances/${id}/cron/${jobId}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteCron: (id: string, jobId: string) =>
    request<MutationResponse>(`/api/instances/${id}/cron/${jobId}`, {
      method: "DELETE",
    }),

  // Web UI users
  listUsers: () => request<UserSummary[]>("/api/users"),
  createUser: (body: CreateUserRequest) =>
    request<UserSummary>("/api/users", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  deleteUser: (id: string) =>
    request<void>(`/api/users/${id}`, { method: "DELETE" }),
  resetUserPassword: (id: string, password: string) =>
    request<void>(`/api/users/${id}/password`, {
      method: "POST",
      body: JSON.stringify({ password }),
    }),

  // Skills catalog
  listSkills: () =>
    request<SkillsListResponse>("/api/skills").then((r) => r.skills),
  uploadSkill: async (file: File) => {
    const form = new FormData();
    form.append("file", file);
    const res = await fetch("/api/skills/upload", {
      method: "POST",
      body: form,
    });
    if (res.status === 401) {
      throw new UnauthorizedError();
    }
    const text = await res.text();
    const body = text ? (JSON.parse(text) as unknown) : null;
    if (!res.ok) {
      const err = (body as ApiError | null)?.error ?? `HTTP ${res.status}`;
      throw new Error(err);
    }
    return body as { id: string };
  },
  deleteSkill: (id: string) =>
    request<void>(`/api/skills/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  pushSkill: (id: string, instanceIds: string[]) =>
    request<{ results: SkillPushResult[] }>(
      `/api/skills/${encodeURIComponent(id)}/push`,
      {
        method: "POST",
        body: JSON.stringify({ instance_ids: instanceIds }),
      },
    ),
  importSkillFromInstance: (instanceId: string, skillName: string) =>
    request<{ id: string }>(`/api/skills/import`, {
      method: "POST",
      body: JSON.stringify({ instance_id: instanceId, skill_name: skillName }),
    }),
  updateSkill: (id: string) =>
    request<void>(`/api/skills/${encodeURIComponent(id)}/update`, {
      method: "POST",
    }),
  listSkillsOnInstance: (instanceId: string) =>
    request<{ skills: RemoteSkillSummary[] }>(
      `/api/skills/instances/${instanceId}`,
    ).then((r) => r.skills),

  listMarketplaces: () =>
    request<{ marketplaces: MarketplaceSummary[] }>("/api/marketplaces").then(
      (r) => r.marketplaces,
    ),
  /**
   * Streaming variant: the manager returns NDJSON progress events while
   * the clone + plugin resolution run, so the modal can show staged
   * status instead of looking frozen during a long git clone.
   */
  addMarketplaceStream: (
    input: { gitUrl: string; ref?: string; name?: string },
    onEvent: (ev: MarketplaceProgressEvent) => void,
  ) =>
    streamMarketplaceOp("/api/marketplaces", "POST", onEvent, {
      git_url: input.gitUrl,
      ref: input.ref,
      name: input.name,
    }),
  updateMarketplaceStream: (
    id: string,
    onEvent: (ev: MarketplaceProgressEvent) => void,
  ) =>
    streamMarketplaceOp(
      `/api/marketplaces/${encodeURIComponent(id)}/update`,
      "POST",
      onEvent,
    ),
  deleteMarketplace: (id: string) =>
    request<void>(`/api/marketplaces/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  listMarketplaceSkills: (id: string) =>
    request<{ skills: MarketplaceAvailableSkill[] }>(
      `/api/marketplaces/${encodeURIComponent(id)}/skills`,
    ).then((r) => r.skills),
  installMarketplaceSkill: (id: string, name: string) =>
    request<{ id: string }>(
      `/api/marketplaces/${encodeURIComponent(id)}/skills/${encodeURIComponent(
        name,
      )}/install`,
      { method: "POST" },
    ),
};

/**
 * Read an NDJSON-streaming response from a marketplace endpoint and
 * deliver each event to `onEvent`. Resolves with the final
 * `MarketplaceSummary` on success or rejects with the error text from
 * the terminal `error` event (whichever comes first).
 *
 * Falls back gracefully when the server returns a non-streaming JSON
 * payload (e.g. an error from auth middleware that fires before
 * streaming kicks in).
 */
async function streamMarketplaceOp(
  path: string,
  method: "POST",
  onEvent: (ev: MarketplaceProgressEvent) => void,
  body?: unknown,
): Promise<MarketplaceSummary> {
  const res = await fetch(path, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (res.status === 401) {
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("auth:logout"));
    }
    throw new UnauthorizedError();
  }
  if (!res.ok && res.headers.get("content-type")?.includes("application/json")) {
    const txt = await res.text();
    const parsed = txt ? (JSON.parse(txt) as { error?: string }) : null;
    throw new Error(parsed?.error ?? `HTTP ${res.status}`);
  }
  if (!res.body) {
    throw new Error("server returned no body");
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let summary: MarketplaceSummary | null = null;
  let errorMessage: string | null = null;
  // eslint-disable-next-line no-constant-condition
  while (true) {
    const { done, value } = await reader.read();
    if (value) {
      buffer += decoder.decode(value, { stream: true });
      let idx = buffer.indexOf("\n");
      while (idx >= 0) {
        const raw = buffer.slice(0, idx).trim();
        buffer = buffer.slice(idx + 1);
        idx = buffer.indexOf("\n");
        if (!raw) continue;
        try {
          const ev = JSON.parse(raw) as MarketplaceProgressEvent;
          onEvent(ev);
          if (ev.event === "done") summary = ev.summary;
          else if (ev.event === "error") errorMessage = ev.error;
        } catch {
          // Malformed line — skip; continue reading.
        }
      }
    }
    if (done) break;
  }
  if (errorMessage !== null) throw new Error(errorMessage);
  if (summary) return summary;
  throw new Error("stream ended without `done` event");
}

export function instanceEventsUrl(id: string): string {
  return `/api/instances/${id}/events`;
}

export function channelEventsUrl(id: string, channel: string): string {
  return `/api/instances/${id}/channels/${encodeURIComponent(channel)}/events`;
}

/// Path for an EventSource that streams `ChatBlockEvent`s for a single
/// agent. Exposed here so callers don't hand-build the URL.
export function chatEventsUrl(id: string, agent: string): string {
  return `/api/instances/${id}/agents/${encodeURIComponent(agent)}/events`;
}

/// Path for an EventSource that streams `WireWorkflowEvent`s. Without
/// `instanceId` the stream carries events for every instance; passing
/// one scopes it to that single instance server-side.
export function workflowEventsUrl(id: string, instanceId?: number): string {
  const base = `/api/instances/${id}/workflows/events`;
  return instanceId === undefined ? base : `${base}?instance=${instanceId}`;
}

/// Path for an EventSource that streams `WireCronEvent`s for the given
/// instance. No filtering — the stream carries every created / updated /
/// removed / fired mutation for every job.
export function cronEventsUrl(id: string): string {
  return `/api/instances/${id}/cron/events`;
}
