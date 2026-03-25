# Crabbit Architecture

## Overview

Crabbit has three components that communicate through a REST API:

```
┌─────────────────────────────────────────────────────────┐
│  crabbit-server  (single Rust binary)                   │
│                                                         │
│  ┌──────────────┐   ┌────────────────────────────────┐  │
│  │  Axum HTTP   │   │  SvelteKit SPA                 │  │
│  │  /api/v1/*   │   │  (embedded via rust-embed)     │  │
│  └──────┬───────┘   └────────────────────────────────┘  │
│         │                                               │
│  ┌──────▼───────┐                                       │
│  │  SQLite DB   │  (WAL mode, foreign keys on)          │
│  └──────────────┘                                       │
└─────────────────────────────────────────────────────────┘
          ▲                         ▲
          │ REST (curl)             │ REST (fetch)
          │                        │
┌─────────┴──────────┐    ┌────────┴────────┐
│  orchestrator      │    │  Browser        │
│  (POSIX sh script) │    │  http://...3000 │
│                    │    └─────────────────┘
│  invokes:          │
│  - claude CLI      │
│  - gh CLI          │
│  - git             │
└────────────────────┘
```

---

## Component: crabbit-server

### Source layout

```
crates/
  common/src/
    models.rs       All shared types: Repo, Task, TaskEvent, AgentState,
                    GitHubAuthStatus, ClaudeSettings, request/response types
  server/src/
    main.rs         clap CLI, tracing init, config load, DB open, serve
    config.rs       Config struct, TOML parsing, encryption key accessor
    state.rs        AppState (Arc<Mutex<Connection>>, Arc<Config>, pending_oauth map)
    error.rs        ApiError enum → HTTP responses; ApiResult<T> type alias
    crypto.rs       AES-256-GCM encrypt/decrypt (base64-encoded nonce‖ciphertext)
    embed.rs        rust-embed WebAssets + SPA fallback handler
    db/
      mod.rs        open_db() — opens SQLite, applies schema.sql, sets WAL + FK pragmas
      schema.sql    6 tables: repos, tasks, task_events, agent_state, github_auth,
                              claude_settings
      repos.rs      CRUD for repos
      tasks.rs      CRUD for tasks + task_events
      agent.rs      get/set agent_state (singleton row, id=1)
      auth.rs       get/set/clear github_auth (singleton); decrypt token on read
      settings.rs   get/update claude_settings (singleton)
    routes/
      mod.rs        build_router() — nests all sub-routers; require_api_key middleware
      repos.rs      GET/POST /repos, PATCH/DELETE /repos/:id
      tasks.rs      GET/POST /tasks, GET/PATCH /tasks/:id, POST /tasks/:id/events
      agent.rs      GET/PUT /agent/state, GET /agent/next-issue
      auth.rs       GET /github/status(?include_token=true), GET /github/begin,
                    GET /github/callback, DELETE /github
      settings.rs   GET/PUT /claude-settings
    github.rs       GitHubClient — lists open issues with optional label filter
```

### Database schema

All tables use `INTEGER PRIMARY KEY` with Unix-second timestamps.

| Table | Role |
|---|---|
| `repos` | Watched repositories (owner, name, enabled, label_filter) |
| `tasks` | One row per issue being worked; tracks status and PR outcome |
| `task_events` | Append-only event log per task (claude_output, browser_screenshot, etc.) |
| `agent_state` | Single-row: status, wake_at, current_task_id, usage_note |
| `github_auth` | Single-row: encrypted OAuth token, login, scopes |
| `claude_settings` | Single-row: model, effort, budget, system prompt, flags |

### Authentication

All `/api/v1/*` routes require `Authorization: Bearer <api_key>`. The key is set in `server.toml` and shared with the orchestrator via `agent.env`. The browser UI reads it from `localStorage` under the key `crabbit_api_key`.

### Token security

The GitHub OAuth access token is encrypted with AES-256-GCM before being stored in SQLite. A random 12-byte nonce is prepended and the result base64-encoded. The encryption key (`encryption_key_hex` in `server.toml`) never leaves the server process. The plaintext token is only returned when `GET /github/status?include_token=true` is called by an authenticated client (i.e. the orchestrator).

### next-issue logic

`GET /agent/next-issue`:
1. Lists all enabled repos.
2. For each repo, calls the GitHub API to list open issues with the repo's label filter.
3. Cross-references with the `tasks` table to skip issues that are already `in_progress`, `pr_created`, or `needs_human`.
4. Returns the first unworked issue found (or 404 if none).

---

## Component: SvelteKit frontend

The frontend is a pure SPA (no SSR). `adapter-static` exports it to `web/build/`, which is embedded into the server binary at compile time via `rust-embed`. All routes serve `index.html` (SPA fallback).

### Page structure

```
/               Dashboard — agent status card + task stats + recent tasks
/repos          Add/remove repos, toggle enabled, set label filter
/tasks          Filterable task list (by status)
/tasks/[id]     Task detail — issue metadata, PR link, event timeline
/settings       Claude model, effort, budget, system prompt, browser automation
/auth           GitHub OAuth connect/disconnect
/auth/callback  OAuth redirect handler (checks status, redirects to /auth)
```

### API client

`web/src/lib/api.ts` wraps all endpoints with a generic `request<T>()` helper that reads the API key from `localStorage`. Grouped namespaces: `repos`, `tasks`, `agent`, `settings`, `auth`.

### Shared state

Two Svelte stores (`agentState`, `githubStatus`) are populated by `+layout.ts` on initial load and refreshed every 10 seconds by `startPolling()` in `+layout.svelte`. The polling pauses when the page is hidden (Page Visibility API).

---

## Component: Orchestrator

`orchestrator/run.sh` is a POSIX sh script. It runs as a one-shot systemd service triggered by a timer. Each invocation is stateless — all persistent state lives in the API.

### Execution flow

```
start
  │
  ├─ load ~/.config/crabbit/agent.env
  ├─ validate CRABBIT_API_URL, CRABBIT_API_KEY, WORKDIR
  │
  ├─ GET /agent/state
  │     sleeping + wake_at > now  →  exit 0  (timer will retry in 30min)
  │     otherwise                 →  continue
  │
  ├─ PUT /agent/state {status: "running"}
  │
  ├─ GET /agent/next-issue
  │     null  →  PUT idle, exit 0
  │
  ├─ POST /tasks  (or reuse existing_task_id)
  ├─ PATCH /tasks/:id {status: "in_progress"}
  │
  │   --dry-run?  →  reset to pending/idle, exit 0
  │
  ├─ GET /github/status?include_token=true  →  export GH_TOKEN, GITHUB_TOKEN
  ├─ gh repo clone / git fetch + reset
  │
  ├─ GET /settings  →  CLAUDE_MODEL, CLAUDE_EFFORT, ALLOW_BROWSER, ...
  ├─ render prompt_template.md  (Python string substitution)
  │
  ├─ claude --print --dangerously-skip-permissions \
  │         --model $MODEL --effort $EFFORT \
  │         --output-format stream-json < prompt.md
  │         (each output line POSTed to /tasks/:id/events as claude_output)
  │
  ├─ read outcome.json written by Claude
  │   ├─ pr_created    →  PATCH task pr_created + pr_url/number
  │   ├─ needs_human   →  PATCH task needs_human + message
  │   ├─ failed        →  PATCH task failed + error_message
  │   └─ usage_limit   →  PATCH task pending, PUT agent sleeping + wake_at
  │
  ├─ upload screenshots (base64 POST to /tasks/:id/events)
  ├─ cleanup temp files
  └─ exit 0
```

### Prompt template

`orchestrator/prompt_template.md` contains `CRABBIT_*` uppercase placeholders that are substituted by a Python heredoc in `run.sh`. Python is used instead of `sed` to safely handle issue bodies with arbitrary characters (quotes, backslashes, newlines).

The template instructs Claude to:
- Work only in the cloned repo directory
- Create a feature branch before making changes
- Run the existing test suite before opening a PR
- Write `outcome.json` at the end (result + pr_url or message)
- Optionally POST progress events to the API for UI display

---

## Deployment

```
~/.local/bin/crabbit-server          compiled binary
~/.config/crabbit/server.toml        server config (bind, db_path, api_key, keys, OAuth)
~/.config/crabbit/agent.env          orchestrator config (API URL + key, WORKDIR)
~/.config/crabbit/orchestrator/      run.sh + prompt_template.md (copied by install.sh)
~/.local/share/crabbit/crabbit.db    SQLite database
~/.local/share/crabbit/work/         Claude working directory (repos, prompts, logs)
~/.config/systemd/user/              crabbit-server.service, crabbit-agent.{service,timer}
```

### Systemd units

- `crabbit-server.service` — long-running service, restarts on failure, `RUST_LOG` set
- `crabbit-agent.service` — oneshot, 90-minute timeout, PATH includes `~/.cargo/bin`
- `crabbit-agent.timer` — fires 5 min after boot, then 30 min after each completion

---

## Data flow: full issue lifecycle

```
GitHub issue opened
       │
       │  (next timer tick)
       ▼
orchestrator  ──GET /agent/next-issue──►  server calls GitHub API
                                          filters out already-tracked issues
                                          returns first unworked issue
       │
       │  POST /tasks  →  task row created, status=pending
       │  PATCH /tasks/:id  →  status=in_progress
       │
       ▼
claude invoked with rendered prompt
       │
       │  (during run) POST /tasks/:id/events  →  claude_output events
       │
       ▼
outcome.json written by Claude
       │
       ├── pr_created ──────► PATCH /tasks/:id  status=pr_created, pr_url, pr_number
       ├── needs_human ─────► PATCH /tasks/:id  status=needs_human, error_message
       ├── failed ──────────► PATCH /tasks/:id  status=failed, error_message
       └── usage_limit ─────► PATCH /tasks/:id  status=pending (will retry)
                              PUT /agent/state  status=sleeping, wake_at=<future>
```
