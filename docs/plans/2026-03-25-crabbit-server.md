# Crabbit Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Rust/Axum API server that stores all crabbit state in SQLite, exposes REST endpoints for the orchestrator and frontend, and handles GitHub OAuth.

**Architecture:** Single Axum binary embedding both the REST API (`/api/v1/*`) and the SvelteKit static build. SQLite with WAL mode via `rusqlite` (bundled). GitHub OAuth token stored AES-GCM encrypted. API key auth middleware guards all `/api/v1` routes.

**Tech Stack:** Rust stable, Axum 0.7, Tokio, rusqlite (bundled), serde/serde_json, rust-embed, reqwest, aes-gcm, uuid, clap, tower-http, tracing, axum-test (dev), tempfile (dev), wiremock (dev)

---

## File Map

```
Cargo.toml                              workspace root
crates/common/
  Cargo.toml
  src/
    lib.rs                              re-exports models
    models.rs                           all shared request/response types
crates/server/
  Cargo.toml
  src/
    main.rs                             CLI arg parsing, config load, server start
    lib.rs                              pub mod declarations
    config.rs                           Config struct parsed from server.toml
    state.rs                            AppState (db connection, config)
    error.rs                            ApiError enum → JSON IntoResponse
    crypto.rs                           AES-GCM encrypt/decrypt for token storage
    github.rs                           GitHub REST API client (reqwest)
    embed.rs                            rust-embed Assets + SPA fallback handler
    db/
      mod.rs                            open_db(), run migrations
      schema.sql                        all CREATE TABLE statements
      repos.rs                          CRUD queries for repos table
      tasks.rs                          CRUD + event queries for tasks/task_events
      agent.rs                          get/set agent_state singleton
      auth.rs                           get/set github_auth singleton
      settings.rs                       get/set claude_settings singleton
    routes/
      mod.rs                            assemble full Router, auth middleware
      repos.rs                          GET/POST/PATCH/DELETE /api/v1/repos
      tasks.rs                          GET/POST/PATCH /api/v1/tasks + events
      agent.rs                          GET/PUT /api/v1/agent/state + next-issue
      auth.rs                           GitHub OAuth routes
      settings.rs                       GET/PUT /api/v1/claude-settings
```

---

### Task 1: Workspace Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `crates/common/Cargo.toml`
- Create: `crates/common/src/lib.rs`
- Create: `crates/server/Cargo.toml`
- Create: `crates/server/src/main.rs`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
members = ["crates/common", "crates/server"]
resolver = "2"
```

- [ ] **Step 2: Create crates/common/Cargo.toml**

```toml
[package]
name = "crabbit-common"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 3: Create crates/common/src/lib.rs**

```rust
pub mod models;
```

- [ ] **Step 4: Create crates/server/Cargo.toml**

```toml
[package]
name = "crabbit-server"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "crabbit-server"
path = "src/main.rs"

[dependencies]
crabbit-common = { path = "../common" }
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rust-embed = "8"
reqwest = { version = "0.12", features = ["json"] }
aes-gcm = "0.10"
rand = "0.8"
toml = "0.8"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
uuid = { version = "1", features = ["v4"] }
base64 = "0.22"
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
axum-test = "14"
tempfile = "3"
wiremock = "0.6"
```

- [ ] **Step 5: Create crates/server/src/main.rs**

```rust
fn main() {
    println!("crabbit-server");
}
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/
git commit -m "chore: workspace scaffold"
```

---

### Task 2: Common Models

**Files:**
- Create: `crates/common/src/models.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/common/src/models.rs (bottom of file, in #[cfg(test)])
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_roundtrip() {
        let s = serde_json::to_string(&TaskStatus::PrCreated).unwrap();
        assert_eq!(s, "\"pr_created\"");
        let back: TaskStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, TaskStatus::PrCreated);
    }

    #[test]
    fn create_repo_request_roundtrip() {
        let r = CreateRepoRequest {
            owner: "acme".into(),
            name: "myrepo".into(),
            label_filter: Some("crabbit".into()),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: CreateRepoRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.owner, "acme");
        assert_eq!(back.label_filter, Some("crabbit".into()));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p crabbit-common`
Expected: compile error — types not defined yet

- [ ] **Step 3: Implement models.rs**

```rust
use serde::{Deserialize, Serialize};

// ── Repos ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub enabled: bool,
    pub label_filter: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub owner: String,
    pub name: String,
    pub label_filter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRepoRequest {
    pub enabled: Option<bool>,
    pub label_filter: Option<Option<String>>, // Some(None) = clear it
}

// ── Tasks ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    PrCreated,
    NeedsHuman,
    Failed,
    Skipped,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap();
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(&format!("\"{}\"", s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub repo_id: i64,
    pub issue_number: i64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_body: String,
    pub status: TaskStatus,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub error_message: Option<String>,
    pub claude_session_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub repo_id: i64,
    pub issue_number: i64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_body: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub status: Option<TaskStatus>,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub error_message: Option<String>,
    pub claude_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    pub repo_id: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Task Events ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskEventRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
}

// ── Agent State ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Running,
    Sleeping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub status: AgentStatus,
    pub wake_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub current_task_id: Option<i64>,
    pub usage_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentStateRequest {
    pub status: Option<AgentStatus>,
    pub wake_at: Option<i64>,
    pub current_task_id: Option<i64>,
    pub usage_note: Option<String>,
}

// ── GitHub Auth ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAuthStatus {
    pub connected: bool,
    pub github_login: Option<String>,
    pub token_scopes: Option<String>,
    pub connected_at: Option<i64>,
    /// Only populated when `?include_token=true` is passed by authenticated callers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
}

// ── Claude Settings ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSettings {
    pub model: String,
    pub effort_level: String,
    pub max_budget_usd: Option<f64>,
    pub system_prompt_append: Option<String>,
    pub allow_browser_automation: bool,
    pub extra_flags: Vec<String>,
}

impl Default for ClaudeSettings {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".into(),
            effort_level: "high".into(),
            max_budget_usd: None,
            system_prompt_append: None,
            allow_browser_automation: true,
            extra_flags: vec![],
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateClaudeSettingsRequest {
    pub model: Option<String>,
    pub effort_level: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub system_prompt_append: Option<String>,
    pub allow_browser_automation: Option<bool>,
    pub extra_flags: Option<Vec<String>>,
}

// ── Next Issue ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextIssueResponse {
    pub repo_id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub issue_number: i64,
    pub issue_title: String,
    pub issue_url: String,
    pub issue_body: String,
    pub existing_task_id: Option<i64>, // set if this is a retry of a 'pending' task
}
```

- [ ] **Step 4: Export from lib.rs**

```rust
// crates/common/src/lib.rs
pub mod models;
pub use models::*;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p crabbit-common`
Expected: PASS — `task_status_roundtrip` and `create_repo_request_roundtrip` pass

- [ ] **Step 6: Commit**

```bash
git add crates/common/
git commit -m "feat: common API models"
```

---

### Task 3: Config

**Files:**
- Create: `crates/server/src/config.rs`

- [ ] **Step 1: Write the failing test**

```rust
// bottom of crates/server/src/config.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
            api_key = "secret"
            db_path = "/tmp/crabbit.db"
            encryption_key_hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"

            [github_oauth]
            client_id = "Iv1.abc123"
            client_secret = "ghsec_xyz"
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.api_key, "secret");
        assert_eq!(cfg.bind, "127.0.0.1:3000"); // default
        assert_eq!(cfg.github_oauth.client_id, "Iv1.abc123");
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server config`
Expected: compile error

- [ ] **Step 3: Implement config.rs**

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Address to bind the HTTP server, e.g. "127.0.0.1:3000"
    #[serde(default = "default_bind")]
    pub bind: String,

    /// Path to the SQLite database file
    pub db_path: String,

    /// Bearer token required on all /api/v1/* requests
    pub api_key: String,

    /// 32-byte AES-GCM key as lowercase hex (64 chars) for encrypting the GitHub token
    pub encryption_key_hex: String,

    pub github_oauth: GitHubOAuthConfig,
}

fn default_bind() -> String {
    "127.0.0.1:3000".into()
}

impl Config {
    pub fn encryption_key(&self) -> anyhow::Result<[u8; 32]> {
        let bytes = hex::decode(&self.encryption_key_hex)
            .map_err(|e| anyhow::anyhow!("invalid encryption_key_hex: {}", e))?;
        bytes.try_into().map_err(|_| anyhow::anyhow!("encryption_key_hex must be 64 hex chars (32 bytes)"))
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("cannot read config {}: {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| anyhow::anyhow!("invalid config TOML: {}", e))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Override the OAuth callback URL (default: http://localhost:{port}/api/v1/auth/github/callback)
    pub callback_url_override: Option<String>,
}
```

- [ ] **Step 4: Add `hex` dependency to Cargo.toml**

```toml
hex = "0.4"
```

- [ ] **Step 5: Declare config module in lib.rs**

```rust
// crates/server/src/lib.rs
pub mod config;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p crabbit-server config`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/server/
git commit -m "feat: server config parsing"
```

---

### Task 4: Database — Schema and Connection

**Files:**
- Create: `crates/server/src/db/mod.rs`
- Create: `crates/server/src/db/schema.sql`

- [ ] **Step 1: Write the failing test**

```rust
// crates/server/src/db/mod.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_all_tables() {
        let conn = open_db(":memory:").unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(tables.contains(&"repos".to_string()));
        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"task_events".to_string()));
        assert!(tables.contains(&"agent_state".to_string()));
        assert!(tables.contains(&"github_auth".to_string()));
        assert!(tables.contains(&"claude_settings".to_string()));
    }

    #[test]
    fn singleton_rows_exist_after_open() {
        let conn = open_db(":memory:").unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_state WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server db`
Expected: compile error

- [ ] **Step 3: Create schema.sql**

```sql
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
    usage_note      TEXT
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
    system_prompt_append     TEXT,
    allow_browser_automation INTEGER NOT NULL DEFAULT 1,
    extra_flags              TEXT
);
INSERT OR IGNORE INTO claude_settings(id) VALUES (1);
```

- [ ] **Step 4: Implement db/mod.rs**

```rust
use anyhow::Context;
use rusqlite::Connection;

pub mod agent;
pub mod auth;
pub mod repos;
pub mod settings;
pub mod tasks;

const SCHEMA: &str = include_str!("schema.sql");

pub fn open_db(path: &str) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("cannot open database at {}", path))?;
    // Enable WAL mode and foreign keys
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .context("failed to set pragmas")?;
    run_schema(&conn)?;
    Ok(conn)
}

fn run_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA).context("failed to run schema")?;
    Ok(())
}
```

- [ ] **Step 5: Add stub modules so it compiles**

Create empty files: `crates/server/src/db/repos.rs`, `tasks.rs`, `agent.rs`, `auth.rs`, `settings.rs` each containing just `// TODO`.

- [ ] **Step 6: Declare in lib.rs**

```rust
pub mod config;
pub mod db;
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p crabbit-server db::mod`
Expected: PASS — both tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/db/
git commit -m "feat: database schema and connection"
```

---

### Task 5: DB Queries — Repos

**Files:**
- Modify: `crates/server/src/db/repos.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// crates/server/src/db/repos.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;

    fn now() -> i64 { 1_700_000_000 }

    #[test]
    fn insert_and_list() {
        let conn = open_db(":memory:").unwrap();
        insert_repo(&conn, "acme", "api", None, now()).unwrap();
        insert_repo(&conn, "acme", "web", Some("crabbit"), now()).unwrap();
        let repos = list_repos(&conn).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].owner, "acme");
        assert_eq!(repos[1].label_filter, Some("crabbit".into()));
    }

    #[test]
    fn insert_duplicate_fails() {
        let conn = open_db(":memory:").unwrap();
        insert_repo(&conn, "acme", "api", None, now()).unwrap();
        assert!(insert_repo(&conn, "acme", "api", None, now()).is_err());
    }

    #[test]
    fn update_repo() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_repo(&conn, "acme", "api", None, now()).unwrap();
        set_repo_enabled(&conn, id, false).unwrap();
        let repo = get_repo(&conn, id).unwrap().unwrap();
        assert!(!repo.enabled);
    }

    #[test]
    fn delete_repo() {
        let conn = open_db(":memory:").unwrap();
        let id = insert_repo(&conn, "acme", "api", None, now()).unwrap();
        delete_repo(&conn, id).unwrap();
        assert!(get_repo(&conn, id).unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server db::repos`
Expected: compile error

- [ ] **Step 3: Implement repos.rs**

```rust
use anyhow::Context;
use crabbit_common::Repo;
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_repo(
    conn: &Connection,
    owner: &str,
    name: &str,
    label_filter: Option<&str>,
    now: i64,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO repos (owner, name, label_filter, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![owner, name, label_filter, now],
    )
    .context("insert_repo")?;
    Ok(conn.last_insert_rowid())
}

pub fn list_repos(conn: &Connection) -> anyhow::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, name, enabled, label_filter, created_at FROM repos ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_repo)?;
    rows.map(|r| r.context("row_to_repo")).collect()
}

pub fn list_enabled_repos(conn: &Connection) -> anyhow::Result<Vec<Repo>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, name, enabled, label_filter, created_at FROM repos WHERE enabled = 1 ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_repo)?;
    rows.map(|r| r.context("row_to_repo")).collect()
}

pub fn get_repo(conn: &Connection, id: i64) -> anyhow::Result<Option<Repo>> {
    conn.query_row(
        "SELECT id, owner, name, enabled, label_filter, created_at FROM repos WHERE id = ?1",
        params![id],
        row_to_repo,
    )
    .optional()
    .context("get_repo")
}

pub fn set_repo_enabled(conn: &Connection, id: i64, enabled: bool) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE repos SET enabled = ?1 WHERE id = ?2",
        params![enabled as i64, id],
    )
    .context("set_repo_enabled")?;
    Ok(())
}

pub fn set_repo_label_filter(
    conn: &Connection,
    id: i64,
    label_filter: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE repos SET label_filter = ?1 WHERE id = ?2",
        params![label_filter, id],
    )
    .context("set_repo_label_filter")?;
    Ok(())
}

pub fn delete_repo(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM repos WHERE id = ?1", params![id])
        .context("delete_repo")?;
    Ok(())
}

fn row_to_repo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        owner: row.get(1)?,
        name: row.get(2)?,
        enabled: row.get::<_, i64>(3)? != 0,
        label_filter: row.get(4)?,
        created_at: row.get(5)?,
    })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p crabbit-server db::repos`
Expected: PASS — all 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/db/repos.rs
git commit -m "feat: repos db queries"
```

---

### Task 6: DB Queries — Tasks and Events

**Files:**
- Modify: `crates/server/src/db/tasks.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_db, repos::insert_repo};
    use crabbit_common::TaskStatus;

    fn setup() -> (rusqlite::Connection, i64) {
        let conn = open_db(":memory:").unwrap();
        let repo_id = insert_repo(&conn, "acme", "api", None, 1_700_000_000).unwrap();
        (conn, repo_id)
    }

    #[test]
    fn insert_and_get_task() {
        let (conn, repo_id) = setup();
        let id = insert_task(&conn, repo_id, 42, "Fix bug", "https://gh/42", "body", 1_700_000_000).unwrap();
        let t = get_task(&conn, id).unwrap().unwrap();
        assert_eq!(t.issue_number, 42);
        assert_eq!(t.status, TaskStatus::Pending);
    }

    #[test]
    fn update_task_status() {
        let (conn, repo_id) = setup();
        let id = insert_task(&conn, repo_id, 1, "t", "u", "b", 1_700_000_000).unwrap();
        update_task_status(&conn, id, &TaskStatus::InProgress, 1_700_000_001).unwrap();
        let t = get_task(&conn, id).unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::InProgress);
    }

    #[test]
    fn list_tasks_by_status() {
        let (conn, repo_id) = setup();
        insert_task(&conn, repo_id, 1, "t1", "u1", "b", 1_700_000_000).unwrap();
        let id2 = insert_task(&conn, repo_id, 2, "t2", "u2", "b", 1_700_000_000).unwrap();
        update_task_status(&conn, id2, &TaskStatus::PrCreated, 1_700_000_001).unwrap();
        let pending = list_tasks(&conn, Some(&TaskStatus::Pending), None, 100, 0).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].issue_number, 1);
    }

    #[test]
    fn insert_and_list_events() {
        let (conn, repo_id) = setup();
        let task_id = insert_task(&conn, repo_id, 1, "t", "u", "b", 1_700_000_000).unwrap();
        insert_task_event(&conn, task_id, "claude_output", &serde_json::json!({"text": "hello"}), 1_700_000_001).unwrap();
        let events = list_task_events(&conn, task_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "claude_output");
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server db::tasks`
Expected: compile error

- [ ] **Step 3: Implement tasks.rs**

```rust
use anyhow::Context;
use crabbit_common::{Task, TaskEvent, TaskStatus};
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_task(
    conn: &Connection,
    repo_id: i64,
    issue_number: i64,
    issue_title: &str,
    issue_url: &str,
    issue_body: &str,
    now: i64,
) -> anyhow::Result<i64> {
    conn.execute(
        "INSERT INTO tasks (repo_id, issue_number, issue_title, issue_url, issue_body, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![repo_id, issue_number, issue_title, issue_url, issue_body, now],
    )
    .context("insert_task")?;
    Ok(conn.last_insert_rowid())
}

pub fn get_task(conn: &Connection, id: i64) -> anyhow::Result<Option<Task>> {
    conn.query_row(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, pr_url, pr_number, error_message, claude_session_id,
                created_at, updated_at, started_at, completed_at
         FROM tasks WHERE id = ?1",
        params![id],
        row_to_task,
    )
    .optional()
    .context("get_task")
}

pub fn get_task_by_issue(conn: &Connection, repo_id: i64, issue_number: i64) -> anyhow::Result<Option<Task>> {
    conn.query_row(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, pr_url, pr_number, error_message, claude_session_id,
                created_at, updated_at, started_at, completed_at
         FROM tasks WHERE repo_id = ?1 AND issue_number = ?2",
        params![repo_id, issue_number],
        row_to_task,
    )
    .optional()
    .context("get_task_by_issue")
}

pub fn list_tasks(
    conn: &Connection,
    status: Option<&TaskStatus>,
    repo_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<Task>> {
    let status_str = status.map(|s| s.to_string());
    let mut stmt = conn.prepare(
        "SELECT id, repo_id, issue_number, issue_title, issue_url, issue_body,
                status, pr_url, pr_number, error_message, claude_session_id,
                created_at, updated_at, started_at, completed_at
         FROM tasks
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR repo_id = ?2)
         ORDER BY created_at DESC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt.query_map(params![status_str, repo_id, limit, offset], row_to_task)?;
    rows.map(|r| r.context("row_to_task")).collect()
}

pub fn update_task_status(
    conn: &Connection,
    id: i64,
    status: &TaskStatus,
    now: i64,
) -> anyhow::Result<()> {
    let started_at_set = matches!(status, TaskStatus::InProgress);
    let completed_at_set = matches!(
        status,
        TaskStatus::PrCreated | TaskStatus::NeedsHuman | TaskStatus::Failed | TaskStatus::Skipped
    );
    conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2,
                started_at = CASE WHEN ?3 THEN ?2 ELSE started_at END,
                completed_at = CASE WHEN ?4 THEN ?2 ELSE completed_at END
         WHERE id = ?5",
        params![status.to_string(), now, started_at_set, completed_at_set, id],
    )
    .context("update_task_status")?;
    Ok(())
}

pub fn update_task_outcome(
    conn: &Connection,
    id: i64,
    status: &TaskStatus,
    pr_url: Option<&str>,
    pr_number: Option<i64>,
    error_message: Option<&str>,
    claude_session_id: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE tasks SET status = ?1, pr_url = ?2, pr_number = ?3,
                error_message = ?4, claude_session_id = ?5, updated_at = ?6,
                completed_at = ?6
         WHERE id = ?7",
        params![status.to_string(), pr_url, pr_number, error_message, claude_session_id, now, id],
    )
    .context("update_task_outcome")?;
    Ok(())
}

pub fn insert_task_event(
    conn: &Connection,
    task_id: i64,
    event_type: &str,
    payload: &serde_json::Value,
    now: i64,
) -> anyhow::Result<i64> {
    let payload_str = serde_json::to_string(payload)?;
    conn.execute(
        "INSERT INTO task_events (task_id, event_type, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, event_type, payload_str, now],
    )
    .context("insert_task_event")?;
    Ok(conn.last_insert_rowid())
}

pub fn list_task_events(conn: &Connection, task_id: i64) -> anyhow::Result<Vec<TaskEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, event_type, payload, created_at
         FROM task_events WHERE task_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map(params![task_id], |row| {
        let payload_str: String = row.get(3)?;
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, payload_str, row.get(4)?))
    })?;
    rows.map(|(id, task_id, event_type, payload_str, created_at): rusqlite::Result<(i64, i64, String, String, i64)>| {
        let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
        Ok(TaskEvent { id: id?, task_id: task_id?, event_type: event_type?, payload, created_at: created_at? })
    }).collect()
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let status_str: String = row.get(6)?;
    let status: TaskStatus = status_str.parse().unwrap_or(TaskStatus::Pending);
    Ok(Task {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        issue_number: row.get(2)?,
        issue_title: row.get(3)?,
        issue_url: row.get(4)?,
        issue_body: row.get(5)?,
        status,
        pr_url: row.get(7)?,
        pr_number: row.get(8)?,
        error_message: row.get(9)?,
        claude_session_id: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        started_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p crabbit-server db::tasks`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/db/tasks.rs
git commit -m "feat: tasks and events db queries"
```

---

### Task 7: DB Queries — Singletons (Agent, Auth, Settings)

**Files:**
- Modify: `crates/server/src/db/agent.rs`
- Modify: `crates/server/src/db/auth.rs`
- Modify: `crates/server/src/db/settings.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// db/agent.rs tests
#[test]
fn default_agent_state_is_idle() {
    let conn = open_db(":memory:").unwrap();
    let state = get_agent_state(&conn).unwrap();
    assert_eq!(state.status, AgentStatus::Idle);
    assert!(state.wake_at.is_none());
}

#[test]
fn set_agent_sleeping() {
    let conn = open_db(":memory:").unwrap();
    set_agent_state(&conn, &UpdateAgentStateRequest {
        status: Some(AgentStatus::Sleeping),
        wake_at: Some(9_999_999_999),
        current_task_id: None,
        usage_note: Some("hit limit".into()),
    }).unwrap();
    let state = get_agent_state(&conn).unwrap();
    assert_eq!(state.status, AgentStatus::Sleeping);
    assert_eq!(state.wake_at, Some(9_999_999_999));
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server db::agent`
Expected: compile error

- [ ] **Step 3: Implement db/agent.rs**

```rust
use anyhow::Context;
use crabbit_common::{AgentState, AgentStatus, UpdateAgentStateRequest};
use rusqlite::{params, Connection};

pub fn get_agent_state(conn: &Connection) -> anyhow::Result<AgentState> {
    conn.query_row(
        "SELECT status, wake_at, last_run_at, current_task_id, usage_note
         FROM agent_state WHERE id = 1",
        [],
        |row| {
            let status_str: String = row.get(0)?;
            let status = match status_str.as_str() {
                "running" => AgentStatus::Running,
                "sleeping" => AgentStatus::Sleeping,
                _ => AgentStatus::Idle,
            };
            Ok(AgentState {
                status,
                wake_at: row.get(1)?,
                last_run_at: row.get(2)?,
                current_task_id: row.get(3)?,
                usage_note: row.get(4)?,
            })
        },
    )
    .context("get_agent_state")
}

pub fn set_agent_state(conn: &Connection, req: &UpdateAgentStateRequest) -> anyhow::Result<()> {
    let status_str = req.status.as_ref().map(|s| format!("{:?}", s).to_lowercase());
    conn.execute(
        "UPDATE agent_state SET
            status          = COALESCE(?1, status),
            wake_at         = CASE WHEN ?1 IS NOT NULL THEN ?2 ELSE wake_at END,
            current_task_id = CASE WHEN ?3 IS NOT NULL THEN ?3 ELSE current_task_id END,
            usage_note      = CASE WHEN ?4 IS NOT NULL THEN ?4 ELSE usage_note END,
            last_run_at     = CASE WHEN ?1 = 'running' THEN strftime('%s','now') ELSE last_run_at END
         WHERE id = 1",
        params![status_str, req.wake_at, req.current_task_id, req.usage_note],
    )
    .context("set_agent_state")?;
    Ok(())
}
```

- [ ] **Step 4: Implement db/auth.rs**

```rust
use anyhow::Context;
use crabbit_common::GitHubAuthStatus;
use rusqlite::{params, Connection};

pub fn get_github_auth_status(conn: &Connection) -> anyhow::Result<GitHubAuthStatus> {
    conn.query_row(
        "SELECT access_token, token_scopes, github_login, connected_at FROM github_auth WHERE id = 1",
        [],
        |row| {
            let token: Option<String> = row.get(0)?;
            Ok(GitHubAuthStatus {
                connected: token.is_some(),
                token_scopes: row.get(1)?,
                github_login: row.get(2)?,
                connected_at: row.get(3)?,
            })
        },
    )
    .context("get_github_auth_status")
}

pub fn get_github_token(conn: &Connection) -> anyhow::Result<Option<String>> {
    conn.query_row(
        "SELECT access_token FROM github_auth WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .context("get_github_token")
}

pub fn set_github_auth(
    conn: &Connection,
    encrypted_token: &str,
    scopes: &str,
    login: &str,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE github_auth SET access_token = ?1, token_scopes = ?2, github_login = ?3, connected_at = ?4 WHERE id = 1",
        params![encrypted_token, scopes, login, now],
    )
    .context("set_github_auth")?;
    Ok(())
}

pub fn clear_github_auth(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE github_auth SET access_token = NULL, token_scopes = NULL, github_login = NULL, connected_at = NULL WHERE id = 1",
        [],
    )
    .context("clear_github_auth")?;
    Ok(())
}
```

- [ ] **Step 5: Implement db/settings.rs**

```rust
use anyhow::Context;
use crabbit_common::{ClaudeSettings, UpdateClaudeSettingsRequest};
use rusqlite::{params, Connection};

pub fn get_claude_settings(conn: &Connection) -> anyhow::Result<ClaudeSettings> {
    conn.query_row(
        "SELECT model, effort_level, max_budget_usd, system_prompt_append, allow_browser_automation, extra_flags
         FROM claude_settings WHERE id = 1",
        [],
        |row| {
            let extra_flags_json: Option<String> = row.get(5)?;
            let extra_flags = extra_flags_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            Ok(ClaudeSettings {
                model: row.get(0)?,
                effort_level: row.get(1)?,
                max_budget_usd: row.get(2)?,
                system_prompt_append: row.get(3)?,
                allow_browser_automation: row.get::<_, i64>(4)? != 0,
                extra_flags,
            })
        },
    )
    .context("get_claude_settings")
}

pub fn update_claude_settings(
    conn: &Connection,
    req: &UpdateClaudeSettingsRequest,
) -> anyhow::Result<()> {
    let extra_flags_json = req.extra_flags.as_ref().map(|f| serde_json::to_string(f).unwrap());
    conn.execute(
        "UPDATE claude_settings SET
            model                    = COALESCE(?1, model),
            effort_level             = COALESCE(?2, effort_level),
            max_budget_usd           = COALESCE(?3, max_budget_usd),
            system_prompt_append     = COALESCE(?4, system_prompt_append),
            allow_browser_automation = COALESCE(?5, allow_browser_automation),
            extra_flags              = COALESCE(?6, extra_flags)
         WHERE id = 1",
        params![
            req.model, req.effort_level, req.max_budget_usd,
            req.system_prompt_append,
            req.allow_browser_automation.map(|b| b as i64),
            extra_flags_json
        ],
    )
    .context("update_claude_settings")?;
    Ok(())
}
```

- [ ] **Step 6: Run all db tests**

Run: `cargo test -p crabbit-server db::`
Expected: PASS — all db tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/db/
git commit -m "feat: singleton db queries (agent, auth, settings)"
```

---

### Task 8: Crypto — Token Encryption

**Files:**
- Create: `crates/server/src/crypto.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] { [0u8; 32] }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "ghp_supersecrettoken123";
        let ciphertext = encrypt(plaintext, &key).unwrap();
        assert_ne!(ciphertext, plaintext);
        let recovered = decrypt(&ciphertext, &key).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn different_encryptions_of_same_plaintext_differ() {
        let key = test_key();
        let a = encrypt("token", &key).unwrap();
        let b = encrypt("token", &key).unwrap();
        // different nonces → different ciphertext
        assert_ne!(a, b);
        // but both decrypt correctly
        assert_eq!(decrypt(&a, &key).unwrap(), "token");
        assert_eq!(decrypt(&b, &key).unwrap(), "token");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key = test_key();
        let ct = encrypt("secret", &key).unwrap();
        let wrong_key = [1u8; 32];
        assert!(decrypt(&ct, &wrong_key).is_err());
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server crypto`
Expected: compile error

- [ ] **Step 3: Implement crypto.rs**

```rust
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use anyhow::Context;

/// Encrypt `plaintext` with AES-256-GCM. Returns base64(nonce || ciphertext).
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key).context("invalid key length")?;
    let nonce_bytes: [u8; 12] = {
        use rand::RngCore;
        let mut n = [0u8; 12];
        OsRng.fill_bytes(&mut n);
        n
    };
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypt a value produced by `encrypt`.
pub fn decrypt(encoded: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let combined = B64.decode(encoded).context("base64 decode failed")?;
    anyhow::ensure!(combined.len() > 12, "ciphertext too short");
    let (nonce_bytes, ct) = combined.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key).context("invalid key length")?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))?;
    String::from_utf8(plaintext).context("plaintext is not valid UTF-8")
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p crabbit-server crypto`
Expected: PASS — all 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/crypto.rs
git commit -m "feat: AES-GCM token encryption"
```

---

### Task 9: AppState and Error Types

**Files:**
- Create: `crates/server/src/state.rs`
- Create: `crates/server/src/error.rs`

- [ ] **Step 1: Write the failing test**

```rust
// error.rs tests
#[test]
fn not_found_error_serializes_to_json() {
    use axum::response::IntoResponse;
    use axum::http::StatusCode;
    let err = ApiError::NotFound("task".into());
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Implement error.rs**

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Unauthorized,
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(what) => (StatusCode::NOT_FOUND, format!("{} not found", what)),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            ApiError::Internal(e) => {
                tracing::error!("internal error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error".into())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e)
    }
}

pub type ApiResult<T> = Result<axum::Json<T>, ApiError>;
```

- [ ] **Step 3: Implement state.rs**

```rust
use crate::config::Config;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(conn: Connection, config: Config) -> Self {
        Self {
            db: Arc::new(Mutex::new(conn)),
            config: Arc::new(config),
        }
    }

    pub fn with_db<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        f(&conn)
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test -p crabbit-server error`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/error.rs
git commit -m "feat: AppState and ApiError types"
```

---

### Task 10: Router Skeleton and Auth Middleware

**Files:**
- Create: `crates/server/src/routes/mod.rs`
- Create stub files: `routes/repos.rs`, `routes/tasks.rs`, `routes/agent.rs`, `routes/auth.rs`, `routes/settings.rs`

- [ ] **Step 1: Write the failing test**

```rust
// routes/mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use crate::{db::open_db, state::AppState};
    use crate::config::{Config, GitHubOAuthConfig};

    fn test_state() -> AppState {
        let conn = open_db(":memory:").unwrap();
        let config = Config {
            bind: "127.0.0.1:3000".into(),
            db_path: ":memory:".into(),
            api_key: "test-key".into(),
            encryption_key_hex: "0".repeat(64),
            github_oauth: GitHubOAuthConfig {
                client_id: "id".into(),
                client_secret: "secret".into(),
                callback_url_override: None,
            },
        };
        AppState::new(conn, config)
    }

    #[tokio::test]
    async fn unauthenticated_request_returns_401() {
        let server = TestServer::new(build_router(test_state())).unwrap();
        let response = server.get("/api/v1/repos").await;
        assert_eq!(response.status_code(), 401);
    }

    #[tokio::test]
    async fn authenticated_request_reaches_handler() {
        let server = TestServer::new(build_router(test_state())).unwrap();
        let response = server
            .get("/api/v1/repos")
            .add_header("Authorization", "Bearer test-key")
            .await;
        // stub returns 200 with empty array
        assert_eq!(response.status_code(), 200);
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server routes::mod`
Expected: compile error

- [ ] **Step 3: Implement routes/mod.rs with auth middleware and stub routes**

```rust
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    Router,
};
use serde_json::json;
use crate::state::AppState;

pub mod agent;
pub mod auth;
pub mod repos;
pub mod settings;
pub mod tasks;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .nest("/repos", repos::router())
        .nest("/tasks", tasks::router())
        .nest("/agent", agent::router())
        .nest("/auth", auth::router())
        .nest("/claude-settings", settings::router())
        .layer(middleware::from_fn_with_state(state.clone(), require_api_key))
        .with_state(state.clone());

    Router::new()
        .nest("/api/v1", api)
        // Static assets will be added in Task 18
        .with_state(state)
}

async fn require_api_key(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let provided = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(key) if key == state.config.api_key => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response(),
    }
}
```

- [ ] **Step 4: Implement stub route files**

Each of `repos.rs`, `tasks.rs`, `agent.rs`, `auth.rs`, `settings.rs`:

```rust
// repos.rs stub
use axum::{routing::get, Json, Router};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_repos))
}

async fn list_repos() -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}
```

(Create equivalent stubs for each module with appropriate routes returning empty/stub responses.)

- [ ] **Step 5: Declare modules in lib.rs**

```rust
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p crabbit-server routes::mod`
Expected: PASS — 401 and 200 tests both pass

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/routes/
git commit -m "feat: router scaffold with API key auth middleware"
```

---

### Task 10b: Test Infrastructure Helpers

**Files:**
- Modify: `crates/server/src/routes/mod.rs`

All route tests rely on `test_state()`, `test_server()`, and `add_auth()`. These must be in place before any route tests can run. This task creates that shared test scaffolding.

- [ ] **Step 1: Add test module to routes/mod.rs**

```rust
// At the bottom of crates/server/src/routes/mod.rs
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum_test::TestServer;
    use crate::{
        config::{Config, GitHubOAuthConfig},
        db::open_db,
        state::AppState,
    };

    pub fn test_config() -> Config {
        Config {
            bind: "127.0.0.1:0".into(),
            db_path: ":memory:".into(),
            api_key: "test".into(),
            encryption_key_hex: "a".repeat(64),
            github_oauth: GitHubOAuthConfig {
                client_id: "id".into(),
                client_secret: "sec".into(),
                callback_url_override: None,
            },
        }
    }

    pub fn test_state() -> AppState {
        let conn = open_db(":memory:").unwrap();
        AppState::new(conn, test_config())
    }

    pub fn test_server() -> TestServer {
        TestServer::new(build_router(test_state())).unwrap()
    }

    pub trait AddAuth {
        fn add_auth(self) -> Self;
    }

    impl AddAuth for axum_test::TestRequest {
        fn add_auth(self) -> Self {
            self.add_header("Authorization", "Bearer test")
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo test -p crabbit-server routes::tests`
Expected: compiles with no errors (no test functions yet — that's fine)

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/routes/mod.rs
git commit -m "test: route test infrastructure (test_state, test_server, add_auth)"
```

---

### Task 11: Repos Routes

**Files:**
- Modify: `crates/server/src/routes/repos.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use crabbit_common::Repo;
    use crate::routes::tests::test_server; // shared helper

    #[tokio::test]
    async fn list_repos_empty() {
        let server = test_server();
        let r = server.get("/api/v1/repos").add_auth().await;
        assert_eq!(r.status_code(), 200);
        let repos: Vec<Repo> = r.json();
        assert!(repos.is_empty());
    }

    #[tokio::test]
    async fn create_and_list_repo() {
        let server = test_server();
        let r = server
            .post("/api/v1/repos")
            .add_auth()
            .json(&serde_json::json!({"owner": "acme", "name": "api"}))
            .await;
        assert_eq!(r.status_code(), 201);
        let created: Repo = r.json();
        assert_eq!(created.owner, "acme");

        let r2 = server.get("/api/v1/repos").add_auth().await;
        let repos: Vec<Repo> = r2.json();
        assert_eq!(repos.len(), 1);
    }

    #[tokio::test]
    async fn delete_repo() {
        let server = test_server();
        let r = server.post("/api/v1/repos").add_auth()
            .json(&serde_json::json!({"owner": "x", "name": "y"})).await;
        let repo: Repo = r.json();
        let r2 = server.delete(&format!("/api/v1/repos/{}", repo.id)).add_auth().await;
        assert_eq!(r2.status_code(), 204);
    }
}
```

- [ ] **Step 2: Add test helper to routes/mod.rs**

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum_test::{TestServer, TestServerConfig};
    use crate::{config::{Config, GitHubOAuthConfig}, db::open_db, state::AppState};

    pub fn test_state() -> AppState {
        let conn = open_db(":memory:").unwrap();
        let config = Config {
            bind: "127.0.0.1:0".into(),
            db_path: ":memory:".into(),
            api_key: "test".into(),
            encryption_key_hex: "a".repeat(64),
            github_oauth: GitHubOAuthConfig {
                client_id: "id".into(),
                client_secret: "sec".into(),
                callback_url_override: None,
            },
        };
        AppState::new(conn, config)
    }

    pub fn test_server() -> TestServer {
        TestServer::new(build_router(test_state())).unwrap()
    }

    pub trait AddAuth {
        fn add_auth(self) -> Self;
    }
    impl AddAuth for axum_test::TestRequest {
        fn add_auth(self) -> Self {
            self.add_header("Authorization", "Bearer test")
        }
    }
}
```

- [ ] **Step 3: Implement repos.rs**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use crabbit_common::{CreateRepoRequest, Repo, UpdateRepoRequest};
use crate::{db::repos as db, error::{ApiError, ApiResult}, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", patch(update).delete(remove))
}

async fn list(State(s): State<AppState>) -> ApiResult<Vec<Repo>> {
    let repos = s.with_db(|c| db::list_repos(c))?;
    Ok(Json(repos))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<(StatusCode, Json<Repo>), ApiError> {
    let now = unix_now();
    let id = s.with_db(|c| db::insert_repo(c, &req.owner, &req.name, req.label_filter.as_deref(), now))?;
    let repo = s.with_db(|c| db::get_repo(c, id))?.ok_or_else(|| ApiError::Internal(anyhow::anyhow!("inserted but not found")))?;
    Ok((StatusCode::CREATED, Json(repo)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateRepoRequest>,
) -> Result<Json<Repo>, ApiError> {
    s.with_db(|c| db::get_repo(c, id))?.ok_or_else(|| ApiError::NotFound("repo".into()))?;
    if let Some(enabled) = req.enabled {
        s.with_db(|c| db::set_repo_enabled(c, id, enabled))?;
    }
    if let Some(label) = req.label_filter {
        s.with_db(|c| db::set_repo_label_filter(c, id, label.as_deref()))?;
    }
    let repo = s.with_db(|c| db::get_repo(c, id))?.unwrap();
    Ok(Json(repo))
}

async fn remove(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.with_db(|c| db::get_repo(c, id))?.ok_or_else(|| ApiError::NotFound("repo".into()))?;
    s.with_db(|c| db::delete_repo(c, id))?;
    Ok(StatusCode::NO_CONTENT)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p crabbit-server routes::repos`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/routes/repos.rs
git commit -m "feat: repos CRUD routes"
```

---

### Task 12: Tasks Routes

**Files:**
- Modify: `crates/server/src/routes/tasks.rs`

Implement the full tasks handler (list, get single with events, create, patch, post event). Follow the same pattern as repos.rs. Key handler signatures:

```rust
async fn list(State(s): State<AppState>, Query(q): Query<ListTasksQuery>) -> ApiResult<Vec<Task>>
async fn get_one(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<TaskWithEvents>
async fn create(State(s): State<AppState>, Json(req): Json<CreateTaskRequest>) -> Result<(StatusCode, Json<Task>), ApiError>
async fn update(State(s): State<AppState>, Path(id): Path<i64>, Json(req): Json<UpdateTaskRequest>) -> ApiResult<Task>
async fn add_event(State(s): State<AppState>, Path(id): Path<i64>, Json(req): Json<CreateTaskEventRequest>) -> Result<(StatusCode, Json<TaskEvent>), ApiError>
```

Add `TaskWithEvents` to `crabbit-common/src/models.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskWithEvents {
    #[serde(flatten)]
    pub task: Task,
    pub events: Vec<TaskEvent>,
}
```

- [ ] **Step 1: Write tests for tasks routes**

```rust
#[tokio::test]
async fn create_task_requires_valid_repo() {
    let server = test_server();
    let r = server.post("/api/v1/tasks").add_auth()
        .json(&serde_json::json!({
            "repo_id": 9999, "issue_number": 1,
            "issue_title": "t", "issue_url": "u", "issue_body": "b"
        })).await;
    assert_eq!(r.status_code(), 400);
}

#[tokio::test]
async fn create_and_get_task_with_events() {
    let server = test_server();
    // create a repo first
    let repo: serde_json::Value = server.post("/api/v1/repos").add_auth()
        .json(&serde_json::json!({"owner": "x", "name": "y"})).await.json();
    let repo_id = repo["id"].as_i64().unwrap();

    let r = server.post("/api/v1/tasks").add_auth()
        .json(&serde_json::json!({
            "repo_id": repo_id, "issue_number": 5,
            "issue_title": "Fix it", "issue_url": "https://gh/5", "issue_body": "body"
        })).await;
    assert_eq!(r.status_code(), 201);
    let task: serde_json::Value = r.json();
    let task_id = task["id"].as_i64().unwrap();

    // add event
    server.post(&format!("/api/v1/tasks/{}/events", task_id)).add_auth()
        .json(&serde_json::json!({"event_type": "status_change", "payload": {"from": "pending"}}))
        .await;

    // get with events
    let r2 = server.get(&format!("/api/v1/tasks/{}", task_id)).add_auth().await;
    let full: serde_json::Value = r2.json();
    assert_eq!(full["events"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: Implement tasks.rs** (following same pattern as repos.rs)

- [ ] **Step 3: Run tests**

Run: `cargo test -p crabbit-server routes::tasks`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/routes/tasks.rs crates/common/src/models.rs
git commit -m "feat: tasks CRUD routes with events"
```

---

### Task 13: Agent State Routes

**Files:**
- Modify: `crates/server/src/routes/agent.rs`

(Implement `GET /api/v1/agent/state` and `PUT /api/v1/agent/state`. The `next-issue` endpoint comes in Task 16 after the GitHub client is ready.)

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/state", get(get_state).put(update_state))
        .route("/next-issue", get(next_issue)) // stubbed for now
}
```

- [ ] **Step 1: Write test**

```rust
#[tokio::test]
async fn agent_state_default_is_idle() {
    let server = test_server();
    let r = server.get("/api/v1/agent/state").add_auth().await;
    let state: serde_json::Value = r.json();
    assert_eq!(state["status"], "idle");
}

#[tokio::test]
async fn set_agent_sleeping() {
    let server = test_server();
    server.put("/api/v1/agent/state").add_auth()
        .json(&serde_json::json!({"status": "sleeping", "wake_at": 9999999999i64}))
        .await;
    let r = server.get("/api/v1/agent/state").add_auth().await;
    let state: serde_json::Value = r.json();
    assert_eq!(state["status"], "sleeping");
    assert_eq!(state["wake_at"], 9999999999i64);
}
```

- [ ] **Step 2: Implement agent state handlers, stub next-issue returning null**

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat: agent state routes"
```

---

### Task 14: Settings Routes

**Files:**
- Modify: `crates/server/src/routes/settings.rs`

- [ ] **Step 1: Write test**

```rust
#[tokio::test]
async fn get_default_settings() {
    let server = test_server();
    let r = server.get("/api/v1/claude-settings").add_auth().await;
    let s: serde_json::Value = r.json();
    assert_eq!(s["model"], "claude-sonnet-4-6");
    assert_eq!(s["effort_level"], "high");
    assert_eq!(s["allow_browser_automation"], true);
}

#[tokio::test]
async fn update_model() {
    let server = test_server();
    server.put("/api/v1/claude-settings").add_auth()
        .json(&serde_json::json!({"model": "claude-opus-4-6"})).await;
    let r = server.get("/api/v1/claude-settings").add_auth().await;
    let s: serde_json::Value = r.json();
    assert_eq!(s["model"], "claude-opus-4-6");
}
```

- [ ] **Step 2: Implement settings.rs**

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat: claude settings routes"
```

---

### Task 15: GitHub API Client

**Files:**
- Create: `crates/server/src/github.rs`

- [ ] **Step 1: Write the failing tests (using wiremock)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    #[tokio::test]
    async fn list_open_issues_returns_parsed_issues() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/issues"))
            .and(header("Authorization", "Bearer ghp_test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "number": 42,
                    "title": "Fix the bug",
                    "body": "It is broken",
                    "html_url": "https://github.com/acme/api/issues/42",
                    "labels": []
                }
            ])))
            .mount(&server)
            .await;

        let client = GitHubClient::new("ghp_test".into(), server.uri());
        let issues = client.list_open_issues("acme", "api", None).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 42);
        assert_eq!(issues[0].title, "Fix the bug");
    }

    #[tokio::test]
    async fn list_open_issues_filters_by_label() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/api/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"number": 1, "title": "t1", "body": "b", "html_url": "u",
                 "labels": [{"name": "crabbit"}]},
                {"number": 2, "title": "t2", "body": "b", "html_url": "u",
                 "labels": [{"name": "bug"}]}
            ])))
            .mount(&server)
            .await;

        let client = GitHubClient::new("ghp_test".into(), server.uri());
        let issues = client.list_open_issues("acme", "api", Some("crabbit")).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 1);
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p crabbit-server github`
Expected: compile error

- [ ] **Step 3: Implement github.rs**

```rust
use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct GitHubIssue {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct GitHubClient {
    token: String,
    base_url: String,
    client: reqwest::Client,
}

impl GitHubClient {
    pub fn new(token: String, base_url: String) -> Self {
        Self {
            token,
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub fn from_token(token: String) -> Self {
        Self::new(token, "https://api.github.com".into())
    }

    pub async fn list_open_issues(
        &self,
        owner: &str,
        repo: &str,
        label_filter: Option<&str>,
    ) -> anyhow::Result<Vec<GitHubIssue>> {
        let url = format!("{}/repos/{}/{}/issues", self.base_url, owner, repo);
        let mut req = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "crabbit/1.0")
            .header("Accept", "application/vnd.github+json")
            .query(&[("state", "open"), ("per_page", "100")]);

        if let Some(label) = label_filter {
            req = req.query(&[("labels", label)]);
        }

        let items: Vec<RawIssue> = req.send().await
            .context("github request failed")?
            .error_for_status()
            .context("github API error")?
            .json()
            .await
            .context("github response parse error")?;

        let issues = items
            .into_iter()
            .filter(|i| {
                // If label_filter is set but GitHub didn't filter server-side, do it client-side
                if let Some(label) = label_filter {
                    i.labels.iter().any(|l| l.name == label)
                } else {
                    true
                }
            })
            .map(|i| GitHubIssue {
                number: i.number,
                title: i.title,
                body: i.body.unwrap_or_default(),
                html_url: i.html_url,
            })
            .collect();

        Ok(issues)
    }
}

#[derive(Deserialize)]
struct RawIssue {
    number: i64,
    title: String,
    body: Option<String>,
    html_url: String,
    labels: Vec<RawLabel>,
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p crabbit-server github`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/github.rs
git commit -m "feat: GitHub API client"
```

---

### Task 16: Agent next-issue Route

**Files:**
- Modify: `crates/server/src/routes/agent.rs`

The `GET /api/v1/agent/next-issue` logic:
1. Get all enabled repos
2. For each repo, call `GitHubClient::list_open_issues`
3. For each issue, check if a task row exists with status != pending (skip it)
4. Return the first eligible issue as `NextIssueResponse`, or `null` if none

The GitHub client needs the stored token. Since the token is encrypted, this route decrypts it at call time using the config's encryption key.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn next_issue_returns_null_when_no_repos() {
    let server = test_server();
    let r = server.get("/api/v1/agent/next-issue").add_auth().await;
    assert_eq!(r.status_code(), 200);
    let body: serde_json::Value = r.json();
    assert!(body.is_null());
}
```

- [ ] **Step 2: Implement next-issue handler**

```rust
async fn next_issue(State(s): State<AppState>) -> Result<Json<Option<NextIssueResponse>>, ApiError> {
    // Get GitHub token
    let encrypted_token = s.with_db(|c| crate::db::auth::get_github_token(c))?;
    let token = match encrypted_token {
        None => return Ok(Json(None)), // not connected
        Some(enc) => {
            let key = s.config.encryption_key()
                .map_err(|e| ApiError::Internal(e))?;
            crate::crypto::decrypt(&enc, &key)
                .map_err(|e| ApiError::Internal(e))?
        }
    };

    let repos = s.with_db(|c| crate::db::repos::list_enabled_repos(c))?;
    if repos.is_empty() {
        return Ok(Json(None));
    }

    let gh = crate::github::GitHubClient::from_token(token);

    for repo in repos {
        let issues = gh
            .list_open_issues(&repo.owner, &repo.name, repo.label_filter.as_deref())
            .await
            .map_err(|e| ApiError::Internal(e))?;

        for issue in issues {
            let existing = s.with_db(|c| {
                crate::db::tasks::get_task_by_issue(c, repo.id, issue.number)
            })?;
            match existing {
                // No task yet → eligible
                None => {
                    return Ok(Json(Some(NextIssueResponse {
                        repo_id: repo.id,
                        repo_owner: repo.owner,
                        repo_name: repo.name,
                        issue_number: issue.number,
                        issue_title: issue.title,
                        issue_url: issue.html_url,
                        issue_body: issue.body,
                        existing_task_id: None,
                    })));
                }
                // Pending task (e.g. after usage_limit reset) → retry it
                Some(t) if t.status == TaskStatus::Pending => {
                    return Ok(Json(Some(NextIssueResponse {
                        repo_id: repo.id,
                        repo_owner: repo.owner,
                        repo_name: repo.name,
                        issue_number: issue.number,
                        issue_title: issue.title,
                        issue_url: issue.html_url,
                        issue_body: issue.body,
                        existing_task_id: Some(t.id),
                    })));
                }
                // Already handled → skip
                Some(_) => continue,
            }
        }
    }

    Ok(Json(None))
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p crabbit-server routes::agent`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/routes/agent.rs
git commit -m "feat: agent next-issue route"
```

---

### Task 17: GitHub OAuth Routes

**Files:**
- Modify: `crates/server/src/routes/auth.rs`

The OAuth flow:
- `GET /api/v1/auth/github/begin` → stores a UUID `state` nonce in a shared `Arc<Mutex<HashMap<String, i64>>>` (expiry timestamp), returns JSON `{"url": "https://github.com/login/oauth/authorize?..."}`
- `GET /api/v1/auth/github/callback?code=&state=` → validates nonce, POSTs to GitHub token exchange, fetches `/user`, stores encrypted token in DB, redirects browser to `/`
- `GET /api/v1/auth/github/status` → reads DB. When `?include_token=true` query param is present, also decrypts the stored token and populates `access_token` in the response. This is how the orchestrator retrieves `GH_TOKEN` — the field is omitted from responses without the query param so it's never accidentally leaked to the frontend.
- `DELETE /api/v1/auth/github` → clears DB

The nonce store needs to be added to `AppState`:

```rust
// state.rs additions
pub pending_oauth: Arc<Mutex<std::collections::HashMap<String, i64>>>,
```

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn auth_status_not_connected_initially() {
    let server = test_server();
    let r = server.get("/api/v1/auth/github/status").add_auth().await;
    let s: serde_json::Value = r.json();
    assert_eq!(s["connected"], false);
}

#[tokio::test]
async fn begin_returns_github_url() {
    let server = test_server();
    let r = server.get("/api/v1/auth/github/begin").add_auth().await;
    let s: serde_json::Value = r.json();
    let url = s["url"].as_str().unwrap();
    assert!(url.starts_with("https://github.com/login/oauth/authorize"));
    assert!(url.contains("client_id=id")); // from test config
    assert!(url.contains("scope=repo"));
}

#[tokio::test]
async fn disconnect_clears_status() {
    // Set up a connected state directly via DB, then DELETE
    let state = test_state();
    state.with_db(|c| crate::db::auth::set_github_auth(c, "enc_token", "repo", "testuser", 1_700_000_000)).unwrap();
    let server = TestServer::new(build_router(state)).unwrap();
    let r = server.get("/api/v1/auth/github/status").add_auth().await;
    let s: serde_json::Value = r.json();
    assert_eq!(s["connected"], true);
    server.delete("/api/v1/auth/github").add_auth().await;
    let r2 = server.get("/api/v1/auth/github/status").add_auth().await;
    let s2: serde_json::Value = r2.json();
    assert_eq!(s2["connected"], false);
}
```

- [ ] **Step 2: Implement auth.rs**

Key implementation notes:
- `begin`: generate `uuid::Uuid::new_v4().to_string()` as state nonce, store in `AppState::pending_oauth` with expiry `now + 600`, return JSON with GitHub OAuth URL
- `callback`: validate state nonce exists and not expired, POST to `https://github.com/login/oauth/access_token` with `client_id`, `client_secret`, `code`, parse token, GET `https://api.github.com/user` for login, encrypt token, store in DB, redirect to `/`
- The callback is browser-facing (redirects), not JSON. Use `Redirect::to("/")` from axum

- [ ] **Step 3: Run tests**

Run: `cargo test -p crabbit-server routes::auth`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/routes/auth.rs crates/server/src/state.rs
git commit -m "feat: GitHub OAuth routes"
```

---

### Task 18: Static Asset Embedding

**Files:**
- Create: `crates/server/src/embed.rs`

This requires `web/build/` to exist. Gate the build-time embed on a feature flag so tests work without the web build. Or use `RustEmbed` with a fallback.

- [ ] **Step 1: Implement embed.rs**

```rust
use axum::{
    body::Body,
    http::{header, Response, StatusCode, Uri},
    response::IntoResponse,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/build/"]
#[prefix = "/"]
pub struct WebAssets;

pub async fn serve_static(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // SPA fallback: serve index.html for unknown paths
            match WebAssets::get("index.html") {
                Some(index) => Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(index.data))
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("not found"))
                    .unwrap(),
            }
        }
    }
}
```

- [ ] **Step 2: Add mime_guess dependency**

```toml
mime_guess = "2"
```

- [ ] **Step 3: Wire into router in routes/mod.rs**

```rust
// In build_router:
Router::new()
    .nest("/api/v1", api)
    .fallback(crate::embed::serve_static)
    .with_state(state)
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/embed.rs
git commit -m "feat: static web asset embedding with SPA fallback"
```

---

### Task 19: Main Binary and Server Startup

**Files:**
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Implement main.rs**

```rust
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "crabbit-server", about = "Crabbit GitHub agent server")]
struct Args {
    #[arg(short, long, default_value = "~/.config/crabbit/server.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "crabbit_server=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let config_path = expand_tilde(&args.config);
    let config = crabbit_server::config::Config::load(&config_path)?;

    tracing::info!("opening database at {}", config.db_path);
    let conn = crabbit_server::db::open_db(&config.db_path)?;

    let state = crabbit_server::state::AppState::new(conn, config.clone());
    let router = crabbit_server::routes::build_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!("listening on http://{}", config.bind);
    axum::serve(listener, router).await?;
    Ok(())
}

fn expand_tilde(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(&s[2..]);
        }
    }
    path.to_path_buf()
}
```

- [ ] **Step 2: Run the server to verify it starts**

```bash
# Create a minimal config
cat > /tmp/test-server.toml <<EOF
api_key = "test"
db_path = "/tmp/crabbit-test.db"
encryption_key_hex = "$(openssl rand -hex 32)"

[github_oauth]
client_id = "fake"
client_secret = "fake"
EOF

cargo run -p crabbit-server -- --config /tmp/test-server.toml &
sleep 1
curl -s http://127.0.0.1:3000/api/v1/repos -H "Authorization: Bearer test"
# Expected: []
kill %1
```

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/main.rs
git commit -m "feat: main binary with config loading and server startup"
```

---

### Task 20: Integration Test

**Files:**
- Create: `crates/server/tests/integration.rs`

- [ ] **Step 1: Write integration tests covering the happy path**

```rust
// crates/server/tests/integration.rs
use axum_test::TestServer;
use crabbit_server::{db::open_db, routes::build_router, state::AppState};

fn make_server() -> TestServer {
    let conn = open_db(":memory:").unwrap();
    let config = /* ... test config ... */;
    let state = AppState::new(conn, config);
    TestServer::new(build_router(state)).unwrap()
}

#[tokio::test]
async fn full_lifecycle() {
    let server = make_server();
    let auth = ("Authorization", "Bearer test");

    // 1. Create repo
    let r = server.post("/api/v1/repos").add_header(auth.0, auth.1)
        .json(&serde_json::json!({"owner": "acme", "name": "api"})).await;
    assert_eq!(r.status_code(), 201);
    let repo: serde_json::Value = r.json();
    let repo_id = repo["id"].as_i64().unwrap();

    // 2. Create task
    let r = server.post("/api/v1/tasks").add_header(auth.0, auth.1)
        .json(&serde_json::json!({
            "repo_id": repo_id, "issue_number": 1,
            "issue_title": "Fix login", "issue_url": "https://gh/1", "issue_body": "broken"
        })).await;
    assert_eq!(r.status_code(), 201);
    let task: serde_json::Value = r.json();
    let task_id = task["id"].as_i64().unwrap();
    assert_eq!(task["status"], "pending");

    // 3. Mark in progress
    server.patch(&format!("/api/v1/tasks/{}", task_id)).add_header(auth.0, auth.1)
        .json(&serde_json::json!({"status": "in_progress"})).await;

    // 4. Post event
    server.post(&format!("/api/v1/tasks/{}/events", task_id)).add_header(auth.0, auth.1)
        .json(&serde_json::json!({"event_type": "claude_output", "payload": {"text": "Analyzing..."}}))
        .await;

    // 5. Mark pr_created
    server.patch(&format!("/api/v1/tasks/{}", task_id)).add_header(auth.0, auth.1)
        .json(&serde_json::json!({"status": "pr_created", "pr_url": "https://gh/pull/2", "pr_number": 2}))
        .await;

    // 6. Get task with events
    let r = server.get(&format!("/api/v1/tasks/{}", task_id)).add_header(auth.0, auth.1).await;
    let full: serde_json::Value = r.json();
    assert_eq!(full["status"], "pr_created");
    assert_eq!(full["pr_number"], 2);
    assert_eq!(full["events"].as_array().unwrap().len(), 1);

    // 7. Agent state shows idle after nothing running
    let r = server.get("/api/v1/agent/state").add_header(auth.0, auth.1).await;
    let state: serde_json::Value = r.json();
    assert_eq!(state["status"], "idle");
}
```

- [ ] **Step 2: Run integration test**

Run: `cargo test -p crabbit-server --test integration`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/tests/
git commit -m "test: integration test for full task lifecycle"
```

---
