// web/src/lib/types.ts

export type TaskStatus =
  | 'queued'
  | 'pending'
  | 'in_progress'
  | 'retrying'
  | 'pr_created'
  | 'needs_human'
  | 'failed'
  | 'skipped';

export type AgentStatus = 'idle' | 'running' | 'sleeping';

export interface Repo {
  id: number;
  owner: string;
  name: string;
  enabled: boolean;
  label_filter: string | null;
  labels_require: string[];
  labels_ignore: string[];
  labels_prioritize: string[];
  completion_prompt: string | null;
  created_at: number;
}

export interface Task {
  id: number;
  repo_id: number;
  issue_number: number;
  issue_title: string;
  issue_url: string;
  issue_body: string;
  status: TaskStatus;
  task_type: string;
  issue_labels: string[];
  is_prioritized: boolean;
  pr_url: string | null;
  pr_number: number | null;
  error_message: string | null;
  claude_session_id: string | null;
  retry_count: number;
  created_at: number;
  updated_at: number;
  started_at: number | null;
  completed_at: number | null;
}

export interface TaskEvent {
  id: number;
  task_id: number;
  event_type: string;
  payload: Record<string, unknown>;
  created_at: number;
}

export interface TaskWithEvents extends Task {
  events: TaskEvent[];
}

export interface AgentState {
  status: AgentStatus;
  wake_at: number | null;
  last_run_at: number | null;
  current_task_id: number | null;
  usage_note: string | null;
  usage_pct_7d: number | null;
  usage_pct_5h: number | null;
  usage_reset_at: number | null;
}

export interface GitHubAuthStatus {
  connected: boolean;
  github_login: string | null;
  token_scopes: string | null;
  connected_at: number | null;
}

export interface ClaudeSettings {
  model: string;
  effort_level: string;
  max_budget_usd: number | null;
  usage_limit_pct: number | null;
  system_prompt_append: string | null;
  allow_browser_automation: boolean;
  extra_flags: string[];
}

export interface ClaudeAuthCheckStatus {
  status: 'ok' | 'expired' | 'unknown';
  checked_at: number | null;
  error: string | null;
}

export interface ClaudeAuthStatus {
  configured: boolean;
  updated_at: number | null;
  check: ClaudeAuthCheckStatus;
}

export interface Prompt {
  id: number;
  category: string;
  label: string;
  name: string;
  content: string;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface NextIssueResponse {
  repo_id: number;
  issue_number: number;
  issue_title: string;
  issue_url: string;
  issue_body: string;
}

export interface SyncResult {
  created: number;
  updated: number;
  closed: number;
}
