// web/src/lib/api.ts
import type {
  Repo,
  Task,
  TaskWithEvents,
  TaskStatus,
  AgentState,
  AgentStatus,
  GitHubAuthStatus,
  ClaudeSettings,
  ClaudeAuthStatus,
  ClaudeAuthCheckStatus,
  NextIssueResponse,
  Prompt,
  SyncResult,
} from './types';

const BASE = '/api/v1';

async function request<T>(
  method: string,
  path: string,
  body?: unknown
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: {
      'Content-Type': 'application/json',
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    throw new Error(`${res.status}: ${text}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

// Repos
export const repos = {
  list: () => request<Repo[]>('GET', '/repos'),
  create: (owner: string, name: string) =>
    request<Repo>('POST', '/repos', { owner, name }),
  update: (id: number, patch: {
    enabled?: boolean;
    label_filter?: string | null;
    labels_require?: string[];
    labels_ignore?: string[];
    labels_prioritize?: string[];
    completion_prompt?: string | null;
  }) => request<Repo>('PATCH', `/repos/${id}`, patch),
  delete: (id: number) => request<void>('DELETE', `/repos/${id}`),
};

// Tasks
export const tasks = {
  list: (params?: { repo_id?: number; status?: TaskStatus }) => {
    const qs = new URLSearchParams();
    if (params?.repo_id !== undefined) qs.set('repo_id', String(params.repo_id));
    if (params?.status) qs.set('status', params.status);
    const q = qs.toString();
    return request<Task[]>('GET', `/tasks${q ? `?${q}` : ''}`);
  },
  create: (data: {
    repo_id: number;
    issue_number: number;
    issue_title: string;
    issue_url: string;
    issue_body: string;
  }) => request<Task>('POST', '/tasks', data),
  get: (id: number) => request<TaskWithEvents>('GET', `/tasks/${id}`),
  updateStatus: (id: number, status: TaskStatus) =>
    request<Task>('PATCH', `/tasks/${id}`, { status }),
  addEvent: (
    id: number,
    event_type: string,
    payload: Record<string, unknown>
  ) => request<void>('POST', `/tasks/${id}/events`, { event_type, payload }),
  delete: (id: number) => request<void>('DELETE', `/tasks/${id}`),
  run: (id: number) => request<{ spawned: boolean; task_id: number }>('POST', `/tasks/${id}/run`),
};

// Sync
export const sync = {
  all: () => request<SyncResult>('POST', '/sync', {}),
  repo: (id: number) => request<SyncResult>('POST', `/sync/${id}`, {}),
};

// Agent
export const agent = {
  getState: () => request<AgentState>('GET', '/agent/state'),
  setState: (patch: { status?: AgentStatus; wake_at?: number | null; usage_note?: string | null }) =>
    request<AgentState>('PUT', '/agent/state', patch),
  nextIssue: () => request<NextIssueResponse | null>('GET', '/agent/next-issue'),
  run: () => request<{ spawned: boolean }>('POST', '/agent/run'),
};

// Settings
export const settings = {
  get: () => request<ClaudeSettings>('GET', '/claude-settings'),
  update: (patch: Partial<ClaudeSettings>) =>
    request<ClaudeSettings>('PUT', '/claude-settings', patch),
};

// Claude Auth (credential sync)
export const claudeAuth = {
  status: () => request<ClaudeAuthStatus>('GET', '/claude-auth/status'),
  check: () => request<ClaudeAuthCheckStatus>('POST', '/claude-auth/check'),
  clear: () => request<void>('DELETE', '/claude-auth/'),
};

// Prompts
export const prompts = {
  list: () => request<Prompt[]>('GET', '/prompts'),
  create: (data: { category: string; label?: string; name: string; content: string }) =>
    request<Prompt>('POST', '/prompts', data),
  update: (id: number, patch: { category?: string; label?: string; name?: string; content?: string; enabled?: boolean }) =>
    request<Prompt>('PUT', `/prompts/${id}`, patch),
  delete: (id: number) => request<void>('DELETE', `/prompts/${id}`),
};

// GitHub Auth
export const auth = {
  status: () => request<GitHubAuthStatus>('GET', '/auth/github/status'),
  beginOAuth: (repoId?: number) => {
    const qs = repoId !== undefined ? `?repo_id=${repoId}` : '';
    return request<{ url: string }>('GET', `/auth/github/begin${qs}`);
  },
  disconnect: () => request<void>('DELETE', '/auth/github'),
};
