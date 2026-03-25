# Crabbit Web Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the SvelteKit frontend that provides the UI for configuring repos, reviewing task status, managing GitHub auth, and configuring Claude settings — built to static assets and embedded in the Rust binary.

**Architecture:** SvelteKit with `adapter-static` exports to `web/build/`. All API calls go to `/api/v1/*` on the same origin. The Rust server embeds the build output via `rust-embed`. No SSR — pure SPA.

**Tech Stack:** SvelteKit 2, TypeScript, Svelte 5, `@sveltejs/adapter-static`, Vite, no CSS framework (plain CSS with CSS variables for theming)

**Prerequisite:** The crabbit-server must be running locally at `http://localhost:3000` for manual testing. The `web/vite.config.ts` should proxy `/api` to the server during dev.

---

## File Map

```
web/
  package.json
  svelte.config.js
  vite.config.ts
  tsconfig.json
  src/
    app.html                            HTML shell
    app.css                             global CSS variables, resets
    lib/
      api.ts                            typed fetch wrappers for all endpoints
      types.ts                          TypeScript types mirroring common/models.rs
      stores.ts                         Svelte stores for shared state (agent status, GitHub status)
      components/
        StatusBadge.svelte              colored badge for task status
        AgentStatusCard.svelte          shows idle/running/sleeping + countdown
        TaskCard.svelte                 compact task summary card
        ScreenshotViewer.svelte         renders base64 browser_screenshot events
        ConfirmDialog.svelte            reusable confirmation modal
    routes/
      +layout.svelte                    nav bar, GitHub status indicator
      +layout.ts                        load agent state + github status (used by layout)
      +page.svelte                      Dashboard: agent status + recent tasks
      repos/
        +page.svelte                    Repo list, add/remove/toggle
      tasks/
        +page.svelte                    Filterable task list
        [id]/
          +page.svelte                  Task detail with event timeline
      settings/
        +page.svelte                    Claude settings form
      auth/
        +page.svelte                    GitHub OAuth connect/disconnect
        callback/
          +page.svelte                  Handles OAuth redirect (shows success/error)
```

---

### Task 1: SvelteKit Project Setup

**Files:**
- Create: `web/package.json`
- Create: `web/svelte.config.js`
- Create: `web/vite.config.ts`
- Create: `web/tsconfig.json`
- Create: `web/src/app.html`
- Create: `web/src/app.css`

- [ ] **Step 1: Initialize SvelteKit project**

Run from `web/` directory:
```bash
cd web
npm create svelte@latest . -- --template skeleton --types typescript --no-prettier --no-eslint
npm install
npm install -D @sveltejs/adapter-static
```

- [ ] **Step 2: Configure adapter-static in svelte.config.js**

```javascript
import adapter from '@sveltejs/adapter-static';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  kit: {
    adapter: adapter({
      pages: 'build',
      assets: 'build',
      fallback: 'index.html', // SPA mode: all 404s serve index.html
    }),
  },
};

export default config;
```

- [ ] **Step 3: Configure Vite dev proxy in vite.config.ts**

```typescript
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:3000',
        changeOrigin: true,
      },
    },
  },
});
```

- [ ] **Step 4: Set up app.html**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <link rel="icon" href="%sveltekit.assets%/favicon.png" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    %sveltekit.head%
  </head>
  <body data-sveltekit-preload-data="hover">
    <div style="display: contents">%sveltekit.body%</div>
  </body>
</html>
```

- [ ] **Step 5: Add global CSS variables in app.css**

```css
:root {
  --color-bg: #0f1117;
  --color-surface: #1a1d27;
  --color-border: #2a2d3a;
  --color-text: #e2e8f0;
  --color-text-muted: #8892a4;
  --color-accent: #7c6af7;
  --color-success: #22c55e;
  --color-warning: #f59e0b;
  --color-error: #ef4444;
  --color-info: #3b82f6;

  /* Task status colors */
  --status-pending: #6b7280;
  --status-in_progress: var(--color-info);
  --status-pr_created: var(--color-success);
  --status-needs_human: var(--color-warning);
  --status-failed: var(--color-error);
  --status-skipped: #4b5563;

  font-family: system-ui, -apple-system, sans-serif;
  font-size: 14px;
}

*, *::before, *::after { box-sizing: border-box; }

body {
  margin: 0;
  background: var(--color-bg);
  color: var(--color-text);
  min-height: 100vh;
}

a { color: var(--color-accent); text-decoration: none; }
a:hover { text-decoration: underline; }

button {
  cursor: pointer;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 6px 14px;
  background: var(--color-surface);
  color: var(--color-text);
  font-size: 13px;
}

button:hover { background: var(--color-border); }
button.primary { background: var(--color-accent); border-color: var(--color-accent); color: white; }
button.danger { background: var(--color-error); border-color: var(--color-error); color: white; }

input, select, textarea {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-text);
  padding: 6px 10px;
  font-size: 13px;
  width: 100%;
}

input:focus, select:focus, textarea:focus {
  outline: 2px solid var(--color-accent);
  outline-offset: 1px;
}
```

- [ ] **Step 6: Verify dev server starts**

Run: `npm run dev`
Expected: server starts at `http://localhost:5173` with no errors

- [ ] **Step 7: Verify static build works**

Run: `npm run build`
Expected: `web/build/` directory created with `index.html`

- [ ] **Step 8: Commit**

```bash
git add web/
git commit -m "chore: SvelteKit project setup with adapter-static"
```

---

### Task 2: TypeScript Types and API Client

**Files:**
- Create: `web/src/lib/types.ts`
- Create: `web/src/lib/api.ts`

- [ ] **Step 1: Create types.ts mirroring the Rust models**

```typescript
// web/src/lib/types.ts

export type TaskStatus =
  | 'pending'
  | 'in_progress'
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
  pr_url: string | null;
  pr_number: number | null;
  error_message: string | null;
  claude_session_id: string | null;
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
  system_prompt_append: string | null;
  allow_browser_automation: boolean;
  extra_flags: string[];
}
```

- [ ] **Step 2: Create api.ts with typed fetch wrappers**

```typescript
// web/src/lib/api.ts
import type {
  AgentState, ClaudeSettings, GitHubAuthStatus,
  Repo, Task, TaskEvent, TaskWithEvents,
} from './types';

const API_KEY = import.meta.env.VITE_API_KEY ?? 'changeme';

async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const res = await fetch(`/api/v1${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${API_KEY}`,
      ...options.headers,
    },
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? `HTTP ${res.status}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

// ── Repos ─────────────────────────────────────────────────────────────────

export const repos = {
  list: () => apiFetch<Repo[]>('/repos'),
  create: (owner: string, name: string, label_filter?: string) =>
    apiFetch<Repo>('/repos', {
      method: 'POST',
      body: JSON.stringify({ owner, name, label_filter }),
    }),
  update: (id: number, data: Partial<Pick<Repo, 'enabled' | 'label_filter'>>) =>
    apiFetch<Repo>(`/repos/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
  delete: (id: number) => apiFetch<void>(`/repos/${id}`, { method: 'DELETE' }),
};

// ── Tasks ─────────────────────────────────────────────────────────────────

export const tasks = {
  list: (params?: { status?: string; repo_id?: number; limit?: number; offset?: number }) => {
    const q = new URLSearchParams();
    if (params?.status) q.set('status', params.status);
    if (params?.repo_id !== undefined) q.set('repo_id', String(params.repo_id));
    if (params?.limit !== undefined) q.set('limit', String(params.limit));
    if (params?.offset !== undefined) q.set('offset', String(params.offset));
    return apiFetch<Task[]>(`/tasks?${q}`);
  },
  get: (id: number) => apiFetch<TaskWithEvents>(`/tasks/${id}`),
  updateStatus: (id: number, data: Partial<Task>) =>
    apiFetch<Task>(`/tasks/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
  addEvent: (id: number, event_type: string, payload: Record<string, unknown>) =>
    apiFetch<TaskEvent>(`/tasks/${id}/events`, {
      method: 'POST',
      body: JSON.stringify({ event_type, payload }),
    }),
};

// ── Agent ─────────────────────────────────────────────────────────────────

export const agent = {
  getState: () => apiFetch<AgentState>('/agent/state'),
};

// ── Auth ──────────────────────────────────────────────────────────────────

export const auth = {
  status: () => apiFetch<GitHubAuthStatus>('/auth/github/status'),
  beginOAuth: () => apiFetch<{ url: string }>('/auth/github/begin'),
  disconnect: () => apiFetch<void>('/auth/github', { method: 'DELETE' }),
};

// ── Settings ─────────────────────────────────────────────────────────────

export const settings = {
  get: () => apiFetch<ClaudeSettings>('/claude-settings'),
  update: (data: Partial<ClaudeSettings>) =>
    apiFetch<ClaudeSettings>('/claude-settings', { method: 'PUT', body: JSON.stringify(data) }),
};
```

- [ ] **Step 3: Verify TypeScript compilation**

Run: `npm run check`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/
git commit -m "feat: TypeScript types and API client"
```

---

### Task 3: Shared Stores

**Files:**
- Create: `web/src/lib/stores.ts`

```typescript
// web/src/lib/stores.ts
import { writable } from 'svelte/store';
import type { AgentState, GitHubAuthStatus } from './types';

export const agentState = writable<AgentState | null>(null);
export const githubStatus = writable<GitHubAuthStatus | null>(null);

// Refresh agent state every 10 seconds when page is visible
export function startPolling(fetchFn: () => Promise<void>) {
  let interval: ReturnType<typeof setInterval>;
  const start = () => { interval = setInterval(fetchFn, 10_000); };
  const stop = () => clearInterval(interval);

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', () => {
      document.hidden ? stop() : start();
    });
    start();
  }

  return stop;
}
```

- [ ] **Step 1: Commit**

```bash
git add web/src/lib/stores.ts
git commit -m "feat: shared Svelte stores for agent and auth state"
```

---

### Task 4: Shared Components

**Files:**
- Create: `web/src/lib/components/StatusBadge.svelte`
- Create: `web/src/lib/components/AgentStatusCard.svelte`
- Create: `web/src/lib/components/TaskCard.svelte`
- Create: `web/src/lib/components/ScreenshotViewer.svelte`
- Create: `web/src/lib/components/ConfirmDialog.svelte`

- [ ] **Step 1: StatusBadge.svelte**

```svelte
<!-- web/src/lib/components/StatusBadge.svelte -->
<script lang="ts">
  import type { TaskStatus, AgentStatus } from '$lib/types';

  export let status: TaskStatus | AgentStatus;

  const labels: Record<string, string> = {
    pending: 'Pending',
    in_progress: 'In Progress',
    pr_created: 'PR Created',
    needs_human: 'Needs Human',
    failed: 'Failed',
    skipped: 'Skipped',
    idle: 'Idle',
    running: 'Running',
    sleeping: 'Sleeping',
  };
</script>

<span class="badge" style="--badge-color: var(--status-{status}, var(--color-text-muted))">
  {labels[status] ?? status}
</span>

<style>
  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 9999px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--badge-color);
    border: 1px solid var(--badge-color);
    opacity: 0.9;
  }
</style>
```

- [ ] **Step 2: AgentStatusCard.svelte**

```svelte
<!-- web/src/lib/components/AgentStatusCard.svelte -->
<script lang="ts">
  import type { AgentState } from '$lib/types';
  import StatusBadge from './StatusBadge.svelte';

  export let state: AgentState;

  function formatWakeAt(ts: number | null): string {
    if (!ts) return '';
    const date = new Date(ts * 1000);
    const now = Date.now();
    const diff = date.getTime() - now;
    if (diff <= 0) return 'soon';
    const mins = Math.ceil(diff / 60_000);
    if (mins < 60) return `${mins}m`;
    const hrs = Math.floor(mins / 60);
    const rem = mins % 60;
    return rem > 0 ? `${hrs}h ${rem}m` : `${hrs}h`;
  }

  function formatLastRun(ts: number | null): string {
    if (!ts) return 'Never';
    return new Date(ts * 1000).toLocaleString();
  }
</script>

<div class="card">
  <div class="row">
    <span class="label">Agent</span>
    <StatusBadge status={state.status} />
  </div>

  {#if state.status === 'sleeping' && state.wake_at}
    <div class="row">
      <span class="label">Wakes in</span>
      <span class="value">{formatWakeAt(state.wake_at)}</span>
    </div>
    {#if state.usage_note}
      <div class="note">{state.usage_note}</div>
    {/if}
  {/if}

  {#if state.status === 'running' && state.current_task_id}
    <div class="row">
      <span class="label">Working on</span>
      <a href="/tasks/{state.current_task_id}">Task #{state.current_task_id}</a>
    </div>
  {/if}

  <div class="row">
    <span class="label">Last run</span>
    <span class="value muted">{formatLastRun(state.last_run_at)}</span>
  </div>
</div>

<style>
  .card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .label {
    color: var(--color-text-muted);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .value { font-size: 13px; }
  .muted { color: var(--color-text-muted); }
  .note {
    font-size: 12px;
    color: var(--color-warning);
    background: rgba(245, 158, 11, 0.1);
    border-radius: 4px;
    padding: 6px 8px;
  }
</style>
```

- [ ] **Step 3: TaskCard.svelte**

```svelte
<!-- web/src/lib/components/TaskCard.svelte -->
<script lang="ts">
  import type { Task } from '$lib/types';
  import StatusBadge from './StatusBadge.svelte';

  export let task: Task;

  function timeAgo(ts: number): string {
    const diff = Date.now() - ts * 1000;
    const mins = Math.floor(diff / 60_000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    return `${Math.floor(hrs / 24)}d ago`;
  }
</script>

<a class="card" href="/tasks/{task.id}">
  <div class="header">
    <span class="title">{task.issue_title}</span>
    <StatusBadge status={task.status} />
  </div>
  <div class="meta">
    <span class="issue-ref">#{task.issue_number}</span>
    {#if task.pr_number}
      <span class="pr-ref">→ PR #{task.pr_number}</span>
    {/if}
    <span class="time">{timeAgo(task.updated_at)}</span>
  </div>
</a>

<style>
  .card {
    display: block;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 12px 16px;
    text-decoration: none;
    color: inherit;
    transition: border-color 0.15s;
  }
  .card:hover { border-color: var(--color-accent); text-decoration: none; }
  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 6px;
  }
  .title { font-size: 14px; line-height: 1.4; flex: 1; }
  .meta {
    display: flex;
    gap: 10px;
    font-size: 12px;
    color: var(--color-text-muted);
  }
  .pr-ref { color: var(--color-success); }
</style>
```

- [ ] **Step 4: ScreenshotViewer.svelte**

```svelte
<!-- web/src/lib/components/ScreenshotViewer.svelte -->
<script lang="ts">
  export let base64: string;
  export let filename: string = 'screenshot';
</script>

<div class="viewer">
  <div class="label">{filename}</div>
  <img
    src="data:image/png;base64,{base64}"
    alt={filename}
    loading="lazy"
  />
</div>

<style>
  .viewer {
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
  }
  .label {
    background: var(--color-surface);
    padding: 4px 10px;
    font-size: 11px;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
  }
  img {
    display: block;
    width: 100%;
    max-height: 600px;
    object-fit: contain;
    background: #000;
  }
</style>
```

- [ ] **Step 5: ConfirmDialog.svelte**

```svelte
<!-- web/src/lib/components/ConfirmDialog.svelte -->
<script lang="ts">
  export let open = false;
  export let title = 'Are you sure?';
  export let message = '';
  export let confirmLabel = 'Confirm';
  export let onConfirm: () => void;
  export let onCancel: () => void = () => { open = false; };
</script>

{#if open}
  <div class="overlay" on:click={onCancel}>
    <div class="dialog" on:click|stopPropagation>
      <h3>{title}</h3>
      {#if message}<p>{message}</p>{/if}
      <div class="actions">
        <button on:click={onCancel}>Cancel</button>
        <button class="danger" on:click={onConfirm}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.6);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 24px;
    min-width: 300px;
    max-width: 480px;
  }
  h3 { margin: 0 0 12px; font-size: 16px; }
  p { color: var(--color-text-muted); font-size: 13px; margin: 0 0 20px; }
  .actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>
```

- [ ] **Step 6: Verify TypeScript**

Run: `npm run check`
Expected: no errors

- [ ] **Step 7: Commit**

```bash
git add web/src/lib/components/
git commit -m "feat: shared UI components"
```

---

### Task 5: Layout

**Files:**
- Create: `web/src/routes/+layout.svelte`
- Create: `web/src/routes/+layout.ts`

- [ ] **Step 1: Create layout.ts to load shared data**

```typescript
// web/src/routes/+layout.ts
import { agent, auth } from '$lib/api';

export const prerender = false;
export const ssr = false;

export async function load() {
  const [agentState, githubStatus] = await Promise.all([
    agent.getState().catch(() => null),
    auth.status().catch(() => null),
  ]);
  return { agentState, githubStatus };
}
```

- [ ] **Step 2: Create layout.svelte**

```svelte
<!-- web/src/routes/+layout.svelte -->
<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import { agentState, githubStatus, startPolling } from '$lib/stores';
  import { agent, auth } from '$lib/api';
  import { onMount } from 'svelte';

  export let data;

  $: agentState.set(data.agentState);
  $: githubStatus.set(data.githubStatus);

  onMount(() => {
    const stop = startPolling(async () => {
      const [a, g] = await Promise.all([agent.getState().catch(() => null), auth.status().catch(() => null)]);
      agentState.set(a);
      githubStatus.set(g);
    });
    return stop;
  });

  const navItems = [
    { href: '/', label: 'Dashboard' },
    { href: '/tasks', label: 'Tasks' },
    { href: '/repos', label: 'Repos' },
    { href: '/settings', label: 'Settings' },
  ];
</script>

<div class="shell">
  <nav>
    <a class="brand" href="/">🐚 crabbit</a>
    <div class="nav-links">
      {#each navItems as item}
        <a class:active={$page.url.pathname === item.href} href={item.href}>{item.label}</a>
      {/each}
    </div>
    <div class="nav-end">
      {#if $githubStatus?.connected}
        <a class="gh-user" href="/auth">@{$githubStatus.github_login}</a>
      {:else}
        <a class="gh-connect" href="/auth">Connect GitHub</a>
      {/if}
      {#if $agentState}
        <span class="agent-dot" data-status={$agentState.status} title="Agent {$agentState.status}"></span>
      {/if}
    </div>
  </nav>

  <main>
    <slot />
  </main>
</div>

<style>
  .shell { display: flex; flex-direction: column; min-height: 100vh; }
  nav {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 0 24px;
    height: 52px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    position: sticky; top: 0; z-index: 10;
  }
  .brand { font-weight: 700; font-size: 16px; color: var(--color-text); }
  .nav-links { display: flex; gap: 4px; }
  .nav-links a {
    padding: 5px 10px; border-radius: 6px;
    color: var(--color-text-muted); font-size: 13px;
  }
  .nav-links a.active, .nav-links a:hover {
    color: var(--color-text); background: var(--color-border); text-decoration: none;
  }
  .nav-end { margin-left: auto; display: flex; align-items: center; gap: 12px; }
  .gh-user, .gh-connect { font-size: 12px; }
  .gh-connect { color: var(--color-warning); }
  .agent-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-muted);
  }
  .agent-dot[data-status="running"] { background: var(--color-info); animation: pulse 1.5s infinite; }
  .agent-dot[data-status="idle"] { background: var(--color-success); }
  .agent-dot[data-status="sleeping"] { background: var(--color-warning); }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.4; } }
  main { flex: 1; padding: 24px; max-width: 1100px; margin: 0 auto; width: 100%; }
</style>
```

- [ ] **Step 3: Verify layout renders**

Run: `npm run dev`, open browser, check nav renders
Expected: nav with links and GitHub connect visible

- [ ] **Step 4: Commit**

```bash
git add web/src/routes/+layout.svelte web/src/routes/+layout.ts
git commit -m "feat: app layout with nav and agent status indicator"
```

---

### Task 6: Dashboard Page

**Files:**
- Create: `web/src/routes/+page.svelte`

```svelte
<!-- web/src/routes/+page.svelte -->
<script lang="ts">
  import { agentState } from '$lib/stores';
  import AgentStatusCard from '$lib/components/AgentStatusCard.svelte';
  import TaskCard from '$lib/components/TaskCard.svelte';
  import { tasks } from '$lib/api';
  import type { Task } from '$lib/types';
  import { onMount } from 'svelte';

  let recentTasks: Task[] = [];
  let stats = { total: 0, pr_created: 0, needs_human: 0, failed: 0 };
  let loading = true;

  onMount(async () => {
    const [all, recent] = await Promise.all([
      tasks.list({ limit: 200 }),
      tasks.list({ limit: 10 }),
    ]);
    recentTasks = recent;
    stats = {
      total: all.length,
      pr_created: all.filter(t => t.status === 'pr_created').length,
      needs_human: all.filter(t => t.status === 'needs_human').length,
      failed: all.filter(t => t.status === 'failed').length,
    };
    loading = false;
  });
</script>

<h1>Dashboard</h1>

<div class="grid">
  <div class="section">
    <h2>Agent</h2>
    {#if $agentState}
      <AgentStatusCard state={$agentState} />
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </div>

  <div class="section">
    <h2>Stats</h2>
    <div class="stats-grid">
      <div class="stat"><span class="num">{stats.total}</span><span class="lbl">Total tasks</span></div>
      <div class="stat"><span class="num success">{stats.pr_created}</span><span class="lbl">PRs created</span></div>
      <div class="stat"><span class="num warning">{stats.needs_human}</span><span class="lbl">Needs human</span></div>
      <div class="stat"><span class="num error">{stats.failed}</span><span class="lbl">Failed</span></div>
    </div>
  </div>
</div>

<div class="section" style="margin-top: 24px">
  <div class="section-header">
    <h2>Recent Tasks</h2>
    <a href="/tasks">View all</a>
  </div>
  {#if loading}
    <p class="muted">Loading…</p>
  {:else if recentTasks.length === 0}
    <p class="muted">No tasks yet. Add a repo and wait for the agent to run.</p>
  {:else}
    <div class="task-list">
      {#each recentTasks as task}
        <TaskCard {task} />
      {/each}
    </div>
  {/if}
</div>

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  h2 { margin: 0 0 12px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; }
  @media (max-width: 600px) { .grid { grid-template-columns: 1fr; } }
  .section-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .section-header h2 { margin: 0; }
  .stats-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .stat {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    display: flex; flex-direction: column; gap: 4px;
  }
  .num { font-size: 28px; font-weight: 700; }
  .num.success { color: var(--color-success); }
  .num.warning { color: var(--color-warning); }
  .num.error { color: var(--color-error); }
  .lbl { font-size: 11px; color: var(--color-text-muted); text-transform: uppercase; }
  .task-list { display: flex; flex-direction: column; gap: 8px; }
  .muted { color: var(--color-text-muted); font-size: 13px; }
</style>
```

- [ ] **Step 1: Implement as above, verify it renders**
- [ ] **Step 2: Commit**

```bash
git add web/src/routes/+page.svelte
git commit -m "feat: dashboard page with stats and recent tasks"
```

---

### Task 7: Repos Page

**Files:**
- Create: `web/src/routes/repos/+page.svelte`

Key interactions:
- List repos with enable/disable toggle
- Add form: `owner/name` (parsed from either `owner/name` string or separate fields) + optional label
- Delete with `ConfirmDialog`

- [ ] **Step 1: Implement repos page**

```svelte
<script lang="ts">
  import { repos as reposApi } from '$lib/api';
  import type { Repo } from '$lib/types';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { onMount } from 'svelte';

  let repoList: Repo[] = [];
  let addInput = '';   // "owner/name" format
  let addLabel = '';
  let addError = '';
  let deleteTarget: Repo | null = null;

  onMount(async () => { repoList = await reposApi.list(); });

  async function addRepo() {
    addError = '';
    const parts = addInput.trim().split('/');
    if (parts.length !== 2 || !parts[0] || !parts[1]) {
      addError = 'Enter as owner/repo, e.g. "acme/api"';
      return;
    }
    try {
      const r = await reposApi.create(parts[0], parts[1], addLabel || undefined);
      repoList = [...repoList, r];
      addInput = ''; addLabel = '';
    } catch (e: any) {
      addError = e.message;
    }
  }

  async function toggleEnabled(repo: Repo) {
    const updated = await reposApi.update(repo.id, { enabled: !repo.enabled });
    repoList = repoList.map(r => r.id === repo.id ? updated : r);
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    await reposApi.delete(deleteTarget.id);
    repoList = repoList.filter(r => r.id !== deleteTarget!.id);
    deleteTarget = null;
  }
</script>

<h1>Repos</h1>

<div class="add-form">
  <h2>Add Repository</h2>
  <div class="form-row">
    <input bind:value={addInput} placeholder="owner/repo" />
    <input bind:value={addLabel} placeholder="Label filter (optional)" style="max-width: 200px" />
    <button class="primary" on:click={addRepo}>Add</button>
  </div>
  {#if addError}<p class="error">{addError}</p>{/if}
</div>

{#if repoList.length === 0}
  <p class="muted">No repos configured yet.</p>
{:else}
  <table>
    <thead>
      <tr><th>Repository</th><th>Label Filter</th><th>Enabled</th><th></th></tr>
    </thead>
    <tbody>
      {#each repoList as repo}
        <tr>
          <td>
            <a href="https://github.com/{repo.owner}/{repo.name}" target="_blank" rel="noopener">
              {repo.owner}/{repo.name}
            </a>
          </td>
          <td>{repo.label_filter ?? '—'}</td>
          <td>
            <label class="toggle">
              <input type="checkbox" checked={repo.enabled} on:change={() => toggleEnabled(repo)} />
              <span class="slider"></span>
            </label>
          </td>
          <td>
            <button class="danger small" on:click={() => deleteTarget = repo}>Delete</button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<ConfirmDialog
  open={!!deleteTarget}
  title="Delete repo?"
  message="This will also delete all tasks for {deleteTarget?.owner}/{deleteTarget?.name}."
  confirmLabel="Delete"
  onConfirm={confirmDelete}
  onCancel={() => deleteTarget = null}
/>

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  h2 { margin: 0 0 12px; font-size: 13px; color: var(--color-text-muted); text-transform: uppercase; }
  .add-form {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 24px;
  }
  .form-row { display: flex; gap: 8px; align-items: center; }
  .error { color: var(--color-error); font-size: 12px; margin: 6px 0 0; }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--color-border); font-size: 13px; }
  th { color: var(--color-text-muted); font-size: 11px; text-transform: uppercase; }
  button.small { padding: 3px 8px; font-size: 11px; }
  .muted { color: var(--color-text-muted); }
  /* Toggle switch */
  .toggle { position: relative; display: inline-block; width: 36px; height: 20px; }
  .toggle input { opacity: 0; width: 0; height: 0; }
  .slider {
    position: absolute; cursor: pointer; inset: 0;
    background: var(--color-border); border-radius: 20px; transition: 0.2s;
  }
  .slider::before {
    content: ''; position: absolute;
    width: 14px; height: 14px; left: 3px; bottom: 3px;
    background: white; border-radius: 50%; transition: 0.2s;
  }
  input:checked + .slider { background: var(--color-accent); }
  input:checked + .slider::before { transform: translateX(16px); }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add web/src/routes/repos/
git commit -m "feat: repos management page"
```

---

### Task 8: Tasks List Page

**Files:**
- Create: `web/src/routes/tasks/+page.svelte`

Filterable by status. Shows all tasks as `TaskCard` components with a status filter dropdown.

- [ ] **Step 1: Implement tasks list**

```svelte
<script lang="ts">
  import { tasks as tasksApi } from '$lib/api';
  import type { Task, TaskStatus } from '$lib/types';
  import TaskCard from '$lib/components/TaskCard.svelte';
  import { onMount } from 'svelte';

  let allTasks: Task[] = [];
  let filterStatus: TaskStatus | '' = '';

  onMount(async () => { allTasks = await tasksApi.list({ limit: 200 }); });

  $: filtered = filterStatus
    ? allTasks.filter(t => t.status === filterStatus)
    : allTasks;

  const statuses: Array<{ value: TaskStatus | ''; label: string }> = [
    { value: '', label: 'All' },
    { value: 'pending', label: 'Pending' },
    { value: 'in_progress', label: 'In Progress' },
    { value: 'pr_created', label: 'PR Created' },
    { value: 'needs_human', label: 'Needs Human' },
    { value: 'failed', label: 'Failed' },
    { value: 'skipped', label: 'Skipped' },
  ];
</script>

<div class="header">
  <h1>Tasks</h1>
  <select bind:value={filterStatus}>
    {#each statuses as s}
      <option value={s.value}>{s.label} {s.value ? `(${allTasks.filter(t => t.status === s.value).length})` : `(${allTasks.length})`}</option>
    {/each}
  </select>
</div>

{#if filtered.length === 0}
  <p class="muted">No tasks {filterStatus ? `with status "${filterStatus}"` : 'yet'}.</p>
{:else}
  <div class="list">
    {#each filtered as task}
      <TaskCard {task} />
    {/each}
  </div>
{/if}

<style>
  .header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 20px; }
  h1 { margin: 0; font-size: 20px; }
  select { max-width: 180px; }
  .list { display: flex; flex-direction: column; gap: 8px; }
  .muted { color: var(--color-text-muted); }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add web/src/routes/tasks/
git commit -m "feat: tasks list page with status filter"
```

---

### Task 9: Task Detail Page

**Files:**
- Create: `web/src/routes/tasks/[id]/+page.svelte`

Shows: issue title + link, status badge, PR link, full event timeline (claude_output as collapsible, browser_screenshot rendered inline, other events as JSON). Allows manually triggering retry (PATCH to pending).

- [ ] **Step 1: Implement task detail**

```svelte
<script lang="ts">
  import { page } from '$app/stores';
  import { tasks as tasksApi } from '$lib/api';
  import type { TaskWithEvents, TaskEvent } from '$lib/types';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import ScreenshotViewer from '$lib/components/ScreenshotViewer.svelte';
  import { onMount } from 'svelte';

  let task: TaskWithEvents | null = null;
  let error = '';

  onMount(async () => {
    try {
      task = await tasksApi.get(Number($page.params.id));
    } catch (e: any) {
      error = e.message;
    }
  });

  async function retryTask() {
    if (!task) return;
    await tasksApi.updateStatus(task.id, { status: 'pending' });
    task = { ...task, status: 'pending' };
  }

  function formatTime(ts: number) {
    return new Date(ts * 1000).toLocaleString();
  }

  function isScreenshot(event: TaskEvent) {
    return event.event_type === 'browser_screenshot';
  }

  function eventLabel(type: string): string {
    const labels: Record<string, string> = {
      claude_output: 'Claude output',
      status_change: 'Status change',
      comment_posted: 'Comment posted',
      pr_created: 'PR created',
      browser_screenshot: 'Screenshot',
      error: 'Error',
    };
    return labels[type] ?? type;
  }
</script>

{#if error}
  <p class="error">{error}</p>
{:else if !task}
  <p class="muted">Loading…</p>
{:else}
  <div class="breadcrumb">
    <a href="/tasks">Tasks</a> / #{task.issue_number}
  </div>

  <div class="task-header">
    <h1>{task.issue_title}</h1>
    <StatusBadge status={task.status} />
  </div>

  <div class="meta-row">
    <a href={task.issue_url} target="_blank" rel="noopener">View issue ↗</a>
    {#if task.pr_url}
      <a href={task.pr_url} target="_blank" rel="noopener" class="pr-link">PR #{task.pr_number} ↗</a>
    {/if}
    {#if task.status === 'failed' || task.status === 'needs_human'}
      <button on:click={retryTask}>Retry</button>
    {/if}
  </div>

  {#if task.error_message}
    <div class="error-box">{task.error_message}</div>
  {/if}

  <details class="issue-body">
    <summary>Issue body</summary>
    <pre>{task.issue_body}</pre>
  </details>

  <h2>Timeline</h2>
  {#if task.events.length === 0}
    <p class="muted">No events yet.</p>
  {:else}
    <div class="timeline">
      {#each task.events as event}
        <div class="event" data-type={event.event_type}>
          <div class="event-header">
            <span class="event-type">{eventLabel(event.event_type)}</span>
            <span class="event-time">{formatTime(event.created_at)}</span>
          </div>
          {#if isScreenshot(event)}
            <ScreenshotViewer
              base64={event.payload.base64 as string}
              filename={event.payload.filename as string ?? 'screenshot.png'}
            />
          {:else if event.event_type === 'claude_output'}
            <details>
              <summary>Show output</summary>
              <pre class="output">{typeof event.payload.text === 'string' ? event.payload.text : JSON.stringify(event.payload, null, 2)}</pre>
            </details>
          {:else}
            <pre class="payload">{JSON.stringify(event.payload, null, 2)}</pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .breadcrumb { font-size: 12px; color: var(--color-text-muted); margin-bottom: 16px; }
  .task-header { display: flex; align-items: flex-start; gap: 12px; margin-bottom: 12px; }
  h1 { margin: 0; font-size: 20px; flex: 1; }
  h2 { margin: 24px 0 12px; font-size: 14px; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.05em; }
  .meta-row { display: flex; gap: 12px; align-items: center; margin-bottom: 16px; font-size: 13px; }
  .pr-link { color: var(--color-success); }
  .error-box {
    background: rgba(239,68,68,0.1); border: 1px solid var(--color-error);
    border-radius: 6px; padding: 10px 14px; font-size: 13px;
    color: var(--color-error); margin-bottom: 16px;
  }
  .issue-body { margin-bottom: 24px; }
  .issue-body summary { cursor: pointer; font-size: 13px; color: var(--color-text-muted); }
  pre { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 6px; padding: 12px; font-size: 12px; overflow-x: auto; white-space: pre-wrap; }
  .timeline { display: flex; flex-direction: column; gap: 12px; }
  .event {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px; overflow: hidden;
  }
  .event[data-type="error"] { border-color: var(--color-error); }
  .event[data-type="pr_created"] { border-color: var(--color-success); }
  .event-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 8px 12px; border-bottom: 1px solid var(--color-border);
    background: rgba(255,255,255,0.02);
  }
  .event-type { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }
  .event-time { font-size: 11px; color: var(--color-text-muted); }
  .output, .payload { margin: 0; border: none; border-radius: 0; }
  .muted { color: var(--color-text-muted); }
  .error { color: var(--color-error); }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add web/src/routes/tasks/[id]/
git commit -m "feat: task detail page with event timeline"
```

---

### Task 10: Settings Page

**Files:**
- Create: `web/src/routes/settings/+page.svelte`

Form for `ClaudeSettings`. Saves on submit.

- [ ] **Step 1: Implement settings page**

```svelte
<script lang="ts">
  import { settings as settingsApi } from '$lib/api';
  import type { ClaudeSettings } from '$lib/types';
  import { onMount } from 'svelte';

  let s: ClaudeSettings | null = null;
  let saved = false;
  let error = '';

  const MODELS = ['claude-sonnet-4-6', 'claude-opus-4-6', 'claude-haiku-4-5-20251001'];
  const EFFORTS = ['low', 'medium', 'high', 'max'];

  onMount(async () => { s = await settingsApi.get(); });

  async function save() {
    if (!s) return;
    try {
      s = await settingsApi.update(s);
      saved = true;
      setTimeout(() => saved = false, 2000);
    } catch (e: any) {
      error = e.message;
    }
  }
</script>

<h1>Claude Settings</h1>

{#if !s}
  <p class="muted">Loading…</p>
{:else}
  <form on:submit|preventDefault={save}>
    <div class="field">
      <label for="model">Model</label>
      <select id="model" bind:value={s.model}>
        {#each MODELS as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>

    <div class="field">
      <label for="effort">Effort level</label>
      <select id="effort" bind:value={s.effort_level}>
        {#each EFFORTS as e}<option value={e}>{e}</option>{/each}
      </select>
    </div>

    <div class="field">
      <label for="budget">Max budget (USD)</label>
      <input id="budget" type="number" step="0.01" min="0"
        value={s.max_budget_usd ?? ''}
        on:input={e => s!.max_budget_usd = e.currentTarget.value ? Number(e.currentTarget.value) : null}
        placeholder="No limit"
        style="max-width: 140px"
      />
    </div>

    <div class="field">
      <label for="prompt">System prompt append</label>
      <textarea id="prompt" rows="4" bind:value={s.system_prompt_append} placeholder="Additional instructions appended to every Claude session…"></textarea>
    </div>

    <div class="field checkbox">
      <label>
        <input type="checkbox" bind:checked={s.allow_browser_automation} />
        Allow browser automation (Playwright)
      </label>
      <p class="hint">When enabled, Claude can use Playwright to test frontend changes visually.</p>
    </div>

    <div class="actions">
      <button type="submit" class="primary">Save settings</button>
      {#if saved}<span class="saved">✓ Saved</span>{/if}
      {#if error}<span class="error">{error}</span>{/if}
    </div>
  </form>
{/if}

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  form { max-width: 520px; display: flex; flex-direction: column; gap: 20px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field label { font-size: 12px; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .field.checkbox { flex-direction: row; align-items: flex-start; gap: 10px; }
  .field.checkbox label { text-transform: none; font-size: 13px; color: var(--color-text); display: flex; gap: 8px; align-items: center; cursor: pointer; }
  .hint { margin: 4px 0 0; font-size: 12px; color: var(--color-text-muted); }
  .actions { display: flex; align-items: center; gap: 12px; }
  .saved { color: var(--color-success); font-size: 13px; }
  .error { color: var(--color-error); font-size: 13px; }
  .muted { color: var(--color-text-muted); }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add web/src/routes/settings/
git commit -m "feat: Claude settings page"
```

---

### Task 11: Auth Page

**Files:**
- Create: `web/src/routes/auth/+page.svelte`
- Create: `web/src/routes/auth/callback/+page.svelte`

- [ ] **Step 1: Implement auth page**

```svelte
<!-- web/src/routes/auth/+page.svelte -->
<script lang="ts">
  import { githubStatus } from '$lib/stores';
  import { auth as authApi } from '$lib/api';

  async function connectGitHub() {
    const { url } = await authApi.beginOAuth();
    window.location.href = url;
  }

  async function disconnect() {
    await authApi.disconnect();
    githubStatus.set({ connected: false, github_login: null, token_scopes: null, connected_at: null });
  }

  function formatDate(ts: number | null) {
    if (!ts) return '';
    return new Date(ts * 1000).toLocaleDateString();
  }
</script>

<h1>GitHub Connection</h1>

{#if $githubStatus?.connected}
  <div class="connected-card">
    <div class="gh-icon">✓</div>
    <div class="gh-info">
      <div class="gh-login">@{$githubStatus.github_login}</div>
      <div class="gh-meta">
        Connected {formatDate($githubStatus.connected_at)} · Scopes: {$githubStatus.token_scopes}
      </div>
    </div>
    <button class="danger" on:click={disconnect}>Disconnect</button>
  </div>
  <p class="hint">
    The agent uses this GitHub account to read issues, create PRs, and post comments.
    Disconnecting will pause the agent until you reconnect.
  </p>
{:else}
  <div class="connect-card">
    <h2>Connect your GitHub account</h2>
    <p>Crabbit needs access to your GitHub repos to read issues and create pull requests.</p>
    <p><strong>Required scopes:</strong> <code>repo</code>, <code>read:user</code></p>
    <button class="primary" on:click={connectGitHub}>Connect GitHub</button>
  </div>
{/if}

<style>
  h1 { margin: 0 0 24px; font-size: 20px; }
  .connected-card {
    display: flex; align-items: center; gap: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-success);
    border-radius: 8px; padding: 20px;
    max-width: 520px; margin-bottom: 16px;
  }
  .gh-icon {
    width: 40px; height: 40px; border-radius: 50%;
    background: var(--color-success);
    display: flex; align-items: center; justify-content: center;
    font-size: 20px; flex-shrink: 0;
  }
  .gh-info { flex: 1; }
  .gh-login { font-size: 16px; font-weight: 600; }
  .gh-meta { font-size: 12px; color: var(--color-text-muted); margin-top: 2px; }
  .hint { font-size: 12px; color: var(--color-text-muted); max-width: 520px; }
  .connect-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px; padding: 24px;
    max-width: 480px;
  }
  .connect-card h2 { margin: 0 0 12px; font-size: 16px; }
  .connect-card p { font-size: 13px; color: var(--color-text-muted); margin: 0 0 12px; }
  code { background: var(--color-border); padding: 2px 5px; border-radius: 3px; font-size: 12px; }
</style>
```

- [ ] **Step 2: Implement auth callback page**

The OAuth callback URL is handled by the server (`/api/v1/auth/github/callback`), which redirects the browser to `/`. But if we want a nice UX showing "Connected!" we can handle `/auth/callback` in the SvelteKit app:

```svelte
<!-- web/src/routes/auth/callback/+page.svelte -->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { githubStatus } from '$lib/stores';
  import { auth } from '$lib/api';
  import { onMount } from 'svelte';

  let status: 'loading' | 'success' | 'error' = 'loading';
  let message = '';

  onMount(async () => {
    try {
      const s = await auth.status();
      githubStatus.set(s);
      if (s.connected) {
        status = 'success';
        setTimeout(() => goto('/auth'), 1500);
      } else {
        status = 'error';
        message = 'GitHub connection did not complete.';
      }
    } catch (e: any) {
      status = 'error';
      message = e.message;
    }
  });
</script>

{#if status === 'loading'}
  <p>Completing GitHub connection…</p>
{:else if status === 'success'}
  <p class="success">✓ GitHub connected! Redirecting…</p>
{:else}
  <p class="error">Connection failed: {message}</p>
  <a href="/auth">Try again</a>
{/if}

<style>
  .success { color: var(--color-success); }
  .error { color: var(--color-error); }
</style>
```

- [ ] **Step 3: Commit**

```bash
git add web/src/routes/auth/
git commit -m "feat: GitHub auth connect/disconnect page"
```

---

### Task 12: Build Integration with Rust

**Files:**
- Create: `Makefile` (at workspace root)
- Modify: `crates/server/build.rs`

The Rust binary embeds `web/build/`. We need to ensure the web build is fresh before `cargo build`.

- [ ] **Step 1: Create Makefile**

```makefile
.PHONY: build web server clean

build: web server

web:
	cd web && npm run build

server: web
	cargo build --release

dev-server:
	cargo run -p crabbit-server -- --config ~/.config/crabbit/server.toml

dev-web:
	cd web && npm run dev

clean:
	rm -rf web/build target
```

- [ ] **Step 2: Create build.rs to tell cargo to rerun if web changes**

```rust
// crates/server/build.rs
fn main() {
    println!("cargo:rerun-if-changed=../../web/build");
    println!("cargo:rerun-if-changed=../../web/src");
}
```

- [ ] **Step 3: Verify end-to-end build**

```bash
make build
# Then start server and verify UI loads:
./target/release/crabbit-server --config /tmp/test-server.toml &
curl http://localhost:3000/
# Expected: HTML from SvelteKit
kill %1
```

- [ ] **Step 4: Commit**

```bash
git add Makefile crates/server/build.rs
git commit -m "chore: Makefile for web+server build integration"
```

---
