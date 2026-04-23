export type Transport = "https" | "unix";

export interface InstanceSummary {
  id: string;
  name: string;
  transport: Transport;
  url: string | null;
  socket_path: string | null;
  tls_ca_path: string | null;
  has_api_key: boolean;
  created_at: string;
}

export interface CreateInstanceInput {
  name: string;
  transport: Transport;
  url?: string;
  socket_path?: string;
  api_key?: string;
  tls_ca_path?: string;
}

export interface AgentRow {
  id: string;
  kind: string;
  status: string;
  parent: string | null;
  ai_core: string | null;
}

export interface AgentListResponse {
  agents: AgentRow[];
  ok: boolean;
  raw: string | null;
  error: string | null;
}

export interface HealthResponse {
  ok: boolean;
  status: number;
  body: string;
}

export interface ApiError {
  error: string;
}
