# Crabbit - Architecture Plan

Autonomous GitHub issue processing system. A systemd-driven agent that periodically
invokes Claude noninteractively to resolve GitHub issues, create PRs, or ask questions.

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| Backend API | Rust + Axum + SQLite (rusqlite) |
| Frontend UI | SvelteKit + TypeScript (adapter-static, embedded in binary via rust-embed) |
| Orchestrator | POSIX shell script |
| Browser automation | Playwright (optional, for frontend issue resolution) |
| GitHub auth | OAuth App flow (web-based) + stored token |

---

## Project Structure

```
crabbit/
├── Cargo.toml                        # Workspace root
├── crates/
│   ├── common/                       # Shared API types (serde)
│   └── server/                       # Axum HTTP server binary
│       └── src/
│           ├── db/schema.sql
│           ├── routes/
│           │   ├── repos.rs
│           │   ├── tasks.rs
│           │   ├── agent.rs
│           │   ├── settings.rs
│           │   └── auth.rs           # GitHub OAuth callback
│           └── embed.rs              # rust-embed web assets
├── web/                              # SvelteKit frontend
│   └── src/routes/
│       ├── +page.svelte              # Dashboard
│       ├── repos/+page.svelte
│       ├── tasks/[id]/+page.svelte
│       ├── settings/+page.svelte
│       └── auth/+page.svelte         # OAuth connect UI
├── orchestrator/
│   ├── run.sh
│   └── prompt_template.md
└── deploy/
    ├── crabbit-server.service
    ├── crabbit-agent.service
    └── crabbit-agent.timer
```

---

## Data Model (SQLite)

```sql
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- Configured GitHub repositories
CREATE TABLE repos (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    owner        TEXT NOT NULL,
    name         TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    label_filter TEXT,                          -- only process issues with this label (optional)
    created_at   INTEGER NOT NULL,
    UNIQUE(owner, name)
);

-- One row per GitHub issue processed or in-progress
CREATE TABLE tasks (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id           INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    issue_number      INTEGER NOT NULL,
    issue_title       TEXT NOT NULL,
    issue_url         TEXT NOT NULL,
    issue_body        TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'pending',
    -- pending | in_progress | pr_created | needs_human | failed | skipped
    pr_url            TEXT,
    pr_number         INTEGER,
    error_message     TEXT,
    claude_session_id TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    started_at        INTEGER,
    completed_at      INTEGER,
    UNIQUE(repo_id, issue_number)
);

-- Structured log of events per task
CREATE TABLE task_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    -- comment_posted | pr_created | claude_output | browser_screenshot | error | status_change
    payload    TEXT NOT NULL,  -- JSON blob
    created_at INTEGER NOT NULL
);

-- Singleton agent runtime state
CREATE TABLE agent_state (
    id              INTEGER PRIMARY KEY CHECK(id = 1),
    status          TEXT NOT NULL DEFAULT 'idle',
    -- idle | running | sleeping
    wake_at         INTEGER,
    last_run_at     INTEGER,
    current_task_id INTEGER REFERENCES tasks(id),
    usage_note      TEXT
);
INSERT OR IGNORE INTO agent_state(id, status) VALUES (1, 'idle');

-- GitHub OAuth token storage (singleton)
CREATE TABLE github_auth (
    id           INTEGER PRIMARY KEY CHECK(id = 1),
    access_token TEXT,        -- encrypted at rest
    token_scopes TEXT,        -- e.g. "repo,read:user"
    github_login TEXT,        -- authenticated username
    connected_at INTEGER
);
INSERT OR IGNORE INTO github_auth(id) VALUES (1);

-- Claude invocation settings (singleton)
CREATE TABLE claude_settings (
    id                      INTEGER PRIMARY KEY CHECK(id = 1),
    model                   TEXT NOT NULL DEFAULT 'claude-sonnet-4-6',
    effort_level            TEXT NOT NULL DEFAULT 'high',
    max_budget_usd          REAL,
    system_prompt_append    TEXT,
    allow_browser_automation INTEGER NOT NULL DEFAULT 1,
    extra_flags             TEXT  -- JSON array of extra CLI flags
);
INSERT OR IGNORE INTO claude_settings(id) VALUES (1);
```

---

## API Endpoints

All endpoints require `Authorization: Bearer <api_key>` (configured in server.toml).

### Repos
- `GET    /api/v1/repos`
- `POST   /api/v1/repos`              — `{ owner, name, label_filter? }`
- `PATCH  /api/v1/repos/:id`
- `DELETE /api/v1/repos/:id`          — cascades to tasks

### Tasks
- `GET  /api/v1/tasks?status=&repo_id=&limit=&offset=`
- `GET  /api/v1/tasks/:id`            — includes full event log
- `POST /api/v1/tasks`                — register task (called by orchestrator)
- `PATCH /api/v1/tasks/:id`           — update status/outcome
- `POST /api/v1/tasks/:id/events`     — append event `{ event_type, payload }`

### Orchestrator
- `GET /api/v1/agent/next-issue`      — queries GitHub + tasks table, returns next untouched issue or null
- `GET /api/v1/agent/state`
- `PUT /api/v1/agent/state`

### Auth
- `GET    /api/v1/auth/github/status`   — is GitHub connected? returns login + scopes
- `GET    /api/v1/auth/github/begin`    — returns GitHub OAuth authorization URL
- `GET    /api/v1/auth/github/callback?code=&state=` — exchanges code for token, stores encrypted
- `DELETE /api/v1/auth/github`          — disconnect

### Settings
- `GET /api/v1/claude-settings`
- `PUT /api/v1/claude-settings`

---

## GitHub Authentication Flow

The UI walks the user through OAuth — no manual token management required.

1. User clicks "Connect GitHub" in Settings
2. UI calls `GET /api/v1/auth/github/begin` → server returns GitHub OAuth URL (with `state` nonce)
3. User is redirected to GitHub, authorizes, GitHub redirects back to `http://localhost:<port>/api/v1/auth/github/callback?code=&state=`
4. Server exchanges `code` for access token, stores encrypted in `github_auth` table
5. UI shows "Connected as @username"

Required OAuth scopes: `repo` (read issues, create PRs, post comments), `read:user`.

The server uses this token for all GitHub API calls. The orchestrator fetches the token
from the API and injects it as `GH_TOKEN` into Claude's environment so `gh` CLI works.

Register a GitHub OAuth App with callback URL: `http://localhost:<port>/api/v1/auth/github/callback`.
Put `client_id` and `client_secret` in `server.toml`.

---

## Playwright / Browser Automation

Playwright is installed in Claude's working environment for frontend issue resolution.

- Claude can launch a browser, navigate to dev servers, take screenshots, inspect rendered output
- Screenshots saved to `/tmp/crabbit-work/screenshots/`
- Orchestrator uploads them after Claude exits as `browser_screenshot` task events (base64 payload)
- Task detail UI renders screenshots inline
- Toggled via `allow_browser_automation` in claude_settings

The prompt template instructs Claude:
> If the issue involves frontend/UI work, use Playwright to verify your changes visually.
> Save screenshots to /tmp/crabbit-work/screenshots/ — they will be attached to the task log.

---

## Orchestrator Script (`orchestrator/run.sh`) — Step by Step

```
1.  SOURCE CONFIG        ~/.config/crabbit/agent.env (CRABBIT_API_URL, CRABBIT_API_KEY, WORKDIR)
2.  CHECK SLEEP          GET /agent/state → if sleeping and wake_at > now → exit 0
3.  MARK RUNNING         PUT /agent/state { status: "running" }
4.  FETCH NEXT ISSUE     GET /agent/next-issue → null = PUT idle, exit 0
5.  REGISTER TASK        POST /tasks → get task_id; PATCH status=in_progress
6.  FETCH GH TOKEN       GET /auth/github/status → extract token → set GH_TOKEN env var
7.  PRE-CLONE REPO       git clone / git fetch into $WORKDIR/repos/{owner}/{repo}
8.  BUILD PROMPT         render prompt_template.md with issue details, task_id, API URL
9.  INVOKE CLAUDE        claude --print --dangerously-skip-permissions \
                               --model $model --effort $effort \
                               --output-format stream-json \
                         < prompt.md
                         (stream claude_output events to API as they arrive)
10. READ OUTCOME         cat $WORKDIR/outcome.json
11. UPLOAD SCREENSHOTS   POST files in $WORKDIR/screenshots/ as browser_screenshot events
12. REPORT OUTCOME       PATCH /tasks/:id based on result field:
                           pr_created  → status=pr_created, pr_url, pr_number
                           needs_human → status=needs_human, error_message
                           failed      → status=failed, error_message
                           usage_limit → status=pending (reset for retry)
                                         PUT /agent/state { status: sleeping, wake_at }
13. CLEANUP              rm -rf $WORKDIR/screenshots $WORKDIR/outcome.json $WORKDIR/prompt.md
14. EXIT 0
```

---

## Claude Prompt Template (`orchestrator/prompt_template.md`)

```markdown
# Crabbit Task: Resolve GitHub Issue

You are an autonomous agent resolving a GitHub issue. Work methodically.
Use the `gh` CLI for all GitHub operations (GH_TOKEN is set in your environment).
Use the crabbit API to report progress.

## Issue Details

- **Repository**: {repo_owner}/{repo_name}
- **Issue #**: {issue_number}
- **Title**: {issue_title}
- **URL**: {issue_url}

### Issue Body

{issue_body}

## Objective

1. Read and understand the issue. Read relevant source files.
2. Implement a fix. Write or update tests if applicable.
3. Create a pull request: `gh pr create --title "..." --body "..."`
4. If you cannot resolve the issue without human input, post a comment on the
   issue explaining what you need: `gh issue comment {issue_number} --body "..."`

## Browser Testing (if applicable)

If the issue involves frontend/UI work, use Playwright to verify your changes visually.
Save screenshots to /tmp/crabbit-work/screenshots/ — they will be attached to the task log.

## Reporting Outcome

After completing your work, write your outcome to `/tmp/crabbit-work/outcome.json`:

PR created:
{ "result": "pr_created", "pr_url": "https://...", "pr_number": 42, "message": "..." }

Need human input:
{ "result": "needs_human", "message": "What you need clarified" }

Cannot resolve:
{ "result": "failed", "message": "Why it cannot be resolved" }

Usage/rate limit hit:
{ "result": "usage_limit", "wake_at": 1774500000, "message": "Limit details" }

## API Progress Reporting (optional)

POST events to: {api_url}/api/v1/tasks/{task_id}/events
Header: Authorization: Bearer {api_key}
Body: { "event_type": "comment_posted", "payload": { ... } }

## Constraints

- Work only within the repository at {workdir}/repos/{repo_name}
- Do not modify files outside that directory
- Create a feature branch before making changes
- Run tests before creating a PR
```

---

## Systemd Units

### `crabbit-server.service`
```ini
[Unit]
Description=Crabbit - API server and UI
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/crabbit-server --config %h/.config/crabbit/server.toml
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=crabbit_server=info

[Install]
WantedBy=default.target
```

### `crabbit-agent.service`
```ini
[Unit]
Description=Crabbit agent run (one-shot)
After=network.target crabbit-server.service

[Service]
Type=oneshot
EnvironmentFile=%h/.config/crabbit/agent.env
ExecStart=%h/.config/crabbit/orchestrator/run.sh
Restart=no
TimeoutStartSec=3600
Environment=PATH=/usr/local/bin:/usr/bin:/bin:%h/.local/bin
```

### `crabbit-agent.timer`
```ini
[Unit]
Description=Crabbit agent timer
Requires=crabbit-agent.service

[Timer]
OnBootSec=5min
OnUnitInactiveSec=30min
AccuracySec=1min
Unit=crabbit-agent.service

[Install]
WantedBy=timers.target
```

**Sleep/wake mechanism**: The timer fires every 30 minutes regardless. The orchestrator
checks `wake_at` at step 2 and exits in under 1 second if still within the sleep window.
No dynamic timer manipulation needed.

---

## Key Architectural Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Next-issue logic | In API server | Has GitHub token + DB access; cleaner than shell |
| Outcome from Claude | Write `outcome.json` file | More robust than parsing stream-json in shell |
| GitHub auth | OAuth web flow | No manual token management; user-friendly setup |
| Token storage | Encrypted in SQLite | Single source of truth for server + orchestrator |
| Browser automation | Playwright (optional) | Covers frontend issue resolution; screenshots in UI |
| Concurrency | One issue at a time | Safe, observable, appropriate for personal/small-team use |
| Wake scheduling | Fixed timer + early exit | Simpler than dynamic systemd timer manipulation |
| Embedded UI | rust-embed | Single binary deployment, no separate web server |

---

## Flow Summary

```
systemd timer (every 30min)
    └─> crabbit-agent.service
        └─> orchestrator/run.sh
            ├─ GET /api/v1/agent/state       check wake_at, exit if sleeping
            ├─ GET /api/v1/agent/next-issue  server queries GitHub + tasks table
            ├─ POST /api/v1/tasks            register task
            ├─ write prompt.md
            └─> claude --print --dangerously-skip-permissions < prompt.md
                ├─ clones/updates repo
                ├─ reads issue, writes code, runs tests
                ├─ (optional) Playwright browser testing + screenshots
                ├─ gh pr create  OR  gh issue comment
                └─ writes /tmp/crabbit-work/outcome.json
            ├─ read outcome.json
            ├─ upload screenshots as task events
            └─ PATCH /api/v1/tasks/:id  + PUT /api/v1/agent/state

crabbit-server (always running)
    ├─ SQLite DB
    └─ SvelteKit UI (embedded via rust-embed)
```
