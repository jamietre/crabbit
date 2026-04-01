PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS repos (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    owner        TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    label_filter TEXT,
    created_at   INTEGER NOT NULL,
    UNIQUE(owner, name)
);

CREATE TABLE IF NOT EXISTS tasks (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id           INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    issue_number      INTEGER NOT NULL,
    issue_title       TEXT    NOT NULL,
    issue_url         TEXT    NOT NULL,
    issue_body        TEXT    NOT NULL,
    status            TEXT    NOT NULL DEFAULT 'pending',
    pr_url            TEXT,
    pr_number         INTEGER,
    error_message     TEXT,
    claude_session_id TEXT,
    retry_count       INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    started_at        INTEGER,
    completed_at      INTEGER,
    UNIQUE(repo_id, issue_number)
);

CREATE INDEX IF NOT EXISTS idx_tasks_status  ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_repo_id ON tasks(repo_id);

CREATE TABLE IF NOT EXISTS task_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    event_type TEXT    NOT NULL,
    payload    TEXT    NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_events_task_id ON task_events(task_id);

CREATE TABLE IF NOT EXISTS agent_state (
    id              INTEGER PRIMARY KEY CHECK(id = 1),
    status          TEXT    NOT NULL DEFAULT 'idle',
    wake_at         INTEGER,
    last_run_at     INTEGER,
    current_task_id INTEGER REFERENCES tasks(id),
    usage_note      TEXT,
    usage_pct_7d    REAL,
    usage_reset_at  INTEGER
);
INSERT OR IGNORE INTO agent_state(id, status) VALUES (1, 'idle');

CREATE TABLE IF NOT EXISTS github_auth (
    id           INTEGER PRIMARY KEY CHECK(id = 1),
    access_token TEXT,
    token_scopes TEXT,
    github_login TEXT,
    connected_at INTEGER
);
INSERT OR IGNORE INTO github_auth(id) VALUES (1);

CREATE TABLE IF NOT EXISTS claude_settings (
    id                       INTEGER PRIMARY KEY CHECK(id = 1),
    model                    TEXT    NOT NULL DEFAULT 'claude-sonnet-4-6',
    effort_level             TEXT    NOT NULL DEFAULT 'high',
    max_budget_usd           REAL,
    usage_limit_pct          REAL,
    system_prompt_append     TEXT,
    allow_browser_automation INTEGER NOT NULL DEFAULT 1,
    extra_flags              TEXT
);
INSERT OR IGNORE INTO claude_settings(id) VALUES (1);

CREATE TABLE IF NOT EXISTS prompts (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    category   TEXT    NOT NULL,
    label      TEXT    NOT NULL DEFAULT '',
    name       TEXT    NOT NULL,
    content    TEXT    NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_prompts_category ON prompts(category);

