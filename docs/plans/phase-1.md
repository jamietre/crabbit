# Phase 1 Implementation Plan — Deployable and self-recovering

## Goal

Crabbit should run unattended on a Proxmox LXC container (Debian/Ubuntu) without requiring SSH intervention to recover from common failures. Phase 1 delivers five things: an install script, headless Claude auth via desktop credential sync, stuck task recovery on startup, GitHub token expiry detection, and Claude auth failure detection.

---

## Implementation order

Items are ordered by dependency and risk. Each can be merged independently.

1. **1.3** Stuck task recovery — pure server change, zero risk, immediate reliability win
2. **1.5** Claude auth failure detection — orchestrator only, self-contained
3. **1.4** GitHub token expiry detection — server + orchestrator
4. **1.2** Credential sync daemon — new API surface + DB + desktop script
5. **1.1** Install script — wraps everything up, last because it depends on all of the above being correct

---

## 1.3 Stuck task recovery on startup

**Problem**: If the server or orchestrator crashes while a task is `in_progress`, that task stays stuck forever. The agent state may also be left as `running`, blocking future runs.

**Changes**:

`crates/server/src/db/tasks.rs` — add:
```rust
pub fn reset_in_progress_tasks(conn: &Connection) -> anyhow::Result<u64> {
    let n = conn.execute(
        "UPDATE tasks SET status = 'pending', updated_at = strftime('%s','now')
         WHERE status = 'in_progress'",
        [],
    )?;
    Ok(n as u64)
}
```

`crates/server/src/db/agent.rs` — add:
```rust
pub fn recover_agent_state(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE agent_state SET status = 'idle', current_task_id = NULL
         WHERE status = 'running'",
        [],
    )?;
    Ok(())
}
```

`crates/server/src/main.rs` — call both after `open_db`, before `axum::serve`:
```rust
let recovered = db::tasks::reset_in_progress_tasks(&conn)?;
db::agent::recover_agent_state(&conn)?;
if recovered > 0 {
    tracing::warn!("recovered {} stuck in_progress task(s) to pending", recovered);
}
```

**Tests**: add unit tests for both DB functions using an in-memory DB.

---

## 1.5 Claude auth failure detection

**Problem**: When the `claude` CLI is not authenticated, the task fails with an opaque error and the cause is unclear. The orchestrator also loses the claude exit code due to piping.

**Changes**:

`orchestrator/run.sh` — two fixes:

**Fix 1**: Capture the real exit code from the piped claude invocation. The current `|| CLAUDE_EXIT=$?` catches the `while` loop's exit, not claude's. Use a temp file:

```sh
CLAUDE_EXIT_FILE="${WORKDIR}/claude-exit-code"
rm -f "$CLAUDE_EXIT_FILE"

claude $CLAUDE_FLAGS < "$PROMPT_FILE" 2>&1 \
    | tee "$CLAUDE_LOG" \
    | while IFS= read -r line; do
        curl -sf -X POST \
            -H "Content-Type: application/json" \
            -d "{\"event_type\": \"claude_output\", \"payload\": $(printf '%s\n' "$line" | jq -c '{line: .}' 2>/dev/null || echo '{"line": null}')}" \
            "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null 2>&1 || true
    done
# Exit code of first command in pipe (claude) written by pipefail workaround:
CLAUDE_EXIT=$(cat "$CLAUDE_EXIT_FILE" 2>/dev/null || echo "0")
rm -f "$CLAUDE_EXIT_FILE"
```

Actually simpler — enable `pipefail` just for this section:

```sh
# Capture claude's exit code through the pipe
CLAUDE_EXIT=0
(
    set -o pipefail 2>/dev/null || true  # pipefail where supported
    claude $CLAUDE_FLAGS < "$PROMPT_FILE" 2>&1 \
        | tee "$CLAUDE_LOG" \
        | while IFS= read -r line; do
            curl -sf -X POST \
                -H "Content-Type: application/json" \
                -d "{\"event_type\": \"claude_output\", \"payload\": $(printf '%s\n' "$line" | jq -c '{line: .}' 2>/dev/null || echo '{"line": null}')}" \
                "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null 2>&1 || true
        done
) || CLAUDE_EXIT=$?
```

**Fix 2**: After Claude exits with non-zero, check the log for auth error patterns before reporting a generic failure:

```sh
log "Claude exited with code ${CLAUDE_EXIT}"

if [ "$CLAUDE_EXIT" -ne 0 ]; then
    # Check for authentication failure patterns in the output
    if grep -qi "not authenticated\|authentication\|login\|unauthorized\|auth.*failed" "$CLAUDE_LOG" 2>/dev/null; then
        log "Claude CLI is not authenticated. Re-auth required."
        api_patch "/tasks/${TASK_ID}" \
            '{"status": "failed", "error_message": "Claude CLI not authenticated — run '\''claude login'\'' on the desktop and ensure the credential sync daemon is running."}' \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        _state_managed=1; exit 0
    fi
fi
```

---

## 1.4 GitHub token expiry detection

**Problem**: When the stored GitHub OAuth token has been revoked or expired, every `gh` CLI call and GitHub API call fails with a 401. Currently this produces opaque errors and the agent keeps consuming task slots.

**Changes**:

`crates/server/src/github.rs` — distinguish 401 from other errors:

```rust
use reqwest::StatusCode;

// In list_open_issues, replace error_for_status with:
let resp = client.get(&url)
    // ... headers ...
    .send().await.context("github request failed")?;

if resp.status() == StatusCode::UNAUTHORIZED {
    return Err(anyhow::anyhow!("GITHUB_AUTH_EXPIRED"));
}
resp.error_for_status().context("github API error")?;
```

`crates/server/src/routes/agent.rs` — in the `next_issue` handler, catch auth errors:

```rust
let issues = match gh.list_open_issues(...).await {
    Ok(issues) => issues,
    Err(e) if e.to_string().contains("GITHUB_AUTH_EXPIRED") => {
        // Mark the token as expired so the UI shows a warning
        s.with_db(|c| crate::db::auth::clear_github_auth(c))?;
        tracing::warn!("GitHub token expired — cleared auth");
        return Ok(Json(None));  // treat as no issues, let UI surface the problem
    }
    Err(e) => return Err(ApiError::Internal(e)),
};
```

`crates/server/src/db/auth.rs` — add a softer "mark disconnected" that preserves login info but clears the token, so the UI can show a helpful message rather than just "not connected":

```rust
pub fn expire_github_token(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE github_auth SET access_token = NULL WHERE id = 1",
        [],
    ).context("expire_github_token")?;
    Ok(())
}
```

Use `expire_github_token` (not `clear_github_auth`) in the route handler — this preserves the `github_login` name so the UI can show "Token for @username expired — reconnect".

`web/src/routes/+layout.svelte` (or the dashboard) — the GitHub status card already polls. No UI change needed; when the token is cleared, the existing "not connected" state will show. Optionally add a more specific "token expired" message to `GitHubAuthStatus` (add an `expired: bool` field) to distinguish from "never connected".

`orchestrator/run.sh` — the `gh` CLI calls (clone, etc.) will fail with 401 after the token is cleared. Add a check after `gh repo clone` fails:

```sh
if ! gh repo clone "${REPO_OWNER}/${REPO_NAME}" "$REPO_DIR" -- --quiet 2>"$WORKDIR/gh-error.txt"; then
    if grep -qi "401\|authentication\|credentials" "$WORKDIR/gh-error.txt" 2>/dev/null; then
        log "GitHub auth failed during clone. Token may have expired."
        api_patch "/tasks/${TASK_ID}" \
            '{"status": "failed", "error_message": "GitHub authentication failed — token expired. Reconnect via the Crabbit UI."}' \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        _state_managed=1; exit 0
    fi
    die "gh repo clone failed"
fi
```

---

## 1.2 Credential sync daemon

**Problem**: The `claude` CLI requires browser-based OAuth, which cannot run headlessly. The preferred solution is a watcher on the desktop machine that automatically pushes the OAuth token to the server whenever it changes.

### 1.2a Server: store Claude OAuth token

`crates/server/src/db/schema.sql` — new table:
```sql
CREATE TABLE IF NOT EXISTS claude_auth (
    id          INTEGER PRIMARY KEY CHECK(id = 1),
    oauth_token TEXT,
    updated_at  INTEGER
);
INSERT OR IGNORE INTO claude_auth(id) VALUES (1);
```

`crates/common/src/models.rs` — new types:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAuthStatus {
    pub configured: bool,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClaudeAuthRequest {
    pub oauth_token: String,
    pub sync_secret: String,
}
```

`crates/server/src/config.rs` — add optional sync secret:
```rust
pub claude_sync_secret: Option<String>,
```

`crates/server/src/db/` — new file `claude_auth.rs`:
- `get_claude_auth_status()` → `ClaudeAuthStatus`
- `get_claude_oauth_token()` → `Option<String>` (encrypted)
- `set_claude_oauth_token(conn, encrypted_token, now)` → `Result<()>`

`crates/server/src/routes/` — new file `claude_auth.rs`:
- `GET /api/v1/claude-auth/status` → `ClaudeAuthStatus` (is a token stored?)
- `PUT /api/v1/claude-auth` — validates `sync_secret` from request body, encrypts and stores token
- `GET /api/v1/claude-auth/token` → decrypted token (for orchestrator use, same pattern as GitHub token)

`orchestrator/run.sh` — new step before invoking Claude: fetch the stored token and export it:
```sh
CLAUDE_TOKEN=$(api_get "/claude-auth/token" | jq -r '.oauth_token // empty')
if [ -n "$CLAUDE_TOKEN" ]; then
    export CLAUDE_CODE_OAUTH_TOKEN="$CLAUDE_TOKEN"
    log "Claude OAuth token loaded from server."
else
    log "No Claude OAuth token stored. Claude must be authenticated locally."
fi
```

### 1.2b Desktop sync script

`deploy/claude-sync.sh` — a watcher script to run on the desktop:

```sh
#!/usr/bin/env sh
# Watches ~/.claude/.credentials.json and pushes the OAuth token to Crabbit
# when it changes. Run as a background service or in a terminal.
#
# Required env vars (or edit defaults below):
#   CRABBIT_SERVER_URL   e.g. http://192.168.1.50:3000
#   CRABBIT_SYNC_SECRET  must match claude_sync_secret in server.toml

CREDS_FILE="${HOME}/.claude/.credentials.json"
LAST_TOKEN=""

push_token() {
    TOKEN=$(jq -r '.claudeAiOauth.accessToken // empty' "$CREDS_FILE" 2>/dev/null)
    [ -z "$TOKEN" ] || [ "$TOKEN" = "$LAST_TOKEN" ] && return
    RESP=$(curl -sf -X PUT \
        -H "Content-Type: application/json" \
        -d "{\"oauth_token\": $(printf '%s' "$TOKEN" | jq -Rs .), \"sync_secret\": \"${CRABBIT_SYNC_SECRET}\"}" \
        "${CRABBIT_SERVER_URL}/api/v1/claude-auth" 2>&1)
    if [ $? -eq 0 ]; then
        echo "[$(date '+%H:%M:%S')] Token pushed to Crabbit server."
        LAST_TOKEN="$TOKEN"
    else
        echo "[$(date '+%H:%M:%S')] Failed to push token: $RESP" >&2
    fi
}

# Initial push
[ -f "$CREDS_FILE" ] && push_token

# Watch for changes
if command -v inotifywait >/dev/null 2>&1; then
    # Linux
    while inotifywait -q -e close_write,moved_to "$(dirname "$CREDS_FILE")" 2>/dev/null; do
        push_token
    done
elif command -v fswatch >/dev/null 2>&1; then
    # macOS
    fswatch -o "$CREDS_FILE" | while read -r _; do push_token; done
else
    # Fallback: poll every 30 seconds
    echo "inotifywait/fswatch not found — falling back to 30s polling"
    while sleep 30; do push_token; done
fi
```

`deploy/install-desktop-sync.sh` — installs the watcher as a systemd user service on the desktop machine (Linux/WSL):

Sets up `~/.config/systemd/user/crabbit-claude-sync.service` pointing at `claude-sync.sh`, reading `CRABBIT_SERVER_URL` and `CRABBIT_SYNC_SECRET` from `~/.config/crabbit/desktop-sync.env`.

### 1.2c UI additions

- Add a "Claude Auth" status indicator to the dashboard (alongside GitHub status) showing "Token synced N minutes ago" or "No token — run desktop sync".
- Add a `ClaudeAuthStatus` check to the layout polling loop.

---

## 1.1 Linux install script

**Goal**: A single script that takes a fresh Debian/Ubuntu LXC container from zero to a running Crabbit instance.

`deploy/install.sh` already exists but needs significant updates:
- It assumes `mise` is installed and builds from source on the current machine
- It still references `api_key` in config templates
- It does not install system dependencies

### Revised script structure

```
deploy/
  install.sh              # server-side: installs everything on the LXC container
  install-desktop-sync.sh # desktop-side: installs the credential watcher
  claude-sync.sh          # the watcher script itself
```

`install.sh` flow:

**1. Detect and install system dependencies** (apt):
```sh
apt-get update -qq
apt-get install -y git curl jq python3 nodejs npm build-essential pkg-config libssl-dev
```

**2. Install `gh` CLI** from GitHub's official apt repo (not the Ubuntu package, which is often old):
```sh
curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | ...
echo "deb [arch=$(dpkg --print-architecture) ...] https://cli.github.com/packages stable main" | ...
apt-get install -y gh
```

**3. Install `claude` CLI**:
```sh
npm install -g @anthropic-ai/claude-code
```

**4. Install Rust toolchain** (needed to build crabbit-server; once we have release binaries this step becomes a download):
```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
. "$HOME/.cargo/env"
```

**5. Build or download `crabbit-server`**:
- For now: `cargo build --release -p crabbit-server` (slow but correct)
- Future: `curl -L https://github.com/.../releases/latest/download/crabbit-server-linux-x86_64 -o ~/.local/bin/crabbit-server`

**6. Install orchestrator scripts**:
```sh
mkdir -p ~/.config/crabbit/orchestrator
cp orchestrator/run.sh ~/.config/crabbit/orchestrator/
cp orchestrator/prompt_template.md ~/.config/crabbit/orchestrator/
chmod +x ~/.config/crabbit/orchestrator/run.sh
```

**7. Install systemd units** (same as current script, no changes needed here)

**8. Install config templates** if not already present (non-destructive):
- `~/.config/crabbit/server.toml` from `docs/server-toml-example.toml`
- `~/.config/crabbit/agent.env` from `docs/agent-env-example.env`
- Generate a random `encryption_key_hex` automatically: `openssl rand -hex 32`

**9. Print next steps**:
```
✓ Crabbit installed.

Next steps:
  1. Edit ~/.config/crabbit/server.toml
       - Set encryption_key_hex (already generated)
       - Add your GitHub OAuth App client_id and client_secret
         (create one at https://github.com/settings/developers,
          callback URL: http://<this-server-ip>:3000/api/v1/auth/github/callback)
       - Optionally set claude_sync_secret for the desktop sync daemon

  2. Start the server:
       systemctl --user start crabbit-server
       systemctl --user status crabbit-server

  3. Open http://<this-server-ip>:3000 in your browser
       - Go to /auth and connect your GitHub account
       - Go to /repos and add the repos you want Crabbit to watch

  4. On your desktop machine, set up the Claude credential sync:
       scp deploy/claude-sync.sh deploy/install-desktop-sync.sh <server>:
       (or run install-desktop-sync.sh locally — see docs/INSTALL.md)

  5. Start the agent timer:
       systemctl --user start crabbit-agent.timer
```

### Config template updates

`docs/server-toml-example.toml` and `docs/agent-env-example.env` should be updated to remove all `api_key` references (already done in tracked files) and add `claude_sync_secret`.

---

## Files changed summary

| Item | Files |
|------|-------|
| 1.3 | `crates/server/src/main.rs`, `crates/server/src/db/tasks.rs`, `crates/server/src/db/agent.rs` |
| 1.5 | `orchestrator/run.sh` |
| 1.4 | `crates/server/src/github.rs`, `crates/server/src/routes/agent.rs`, `crates/server/src/db/auth.rs`, `orchestrator/run.sh` |
| 1.2 | `crates/common/src/models.rs`, `crates/server/src/config.rs`, `crates/server/src/db/schema.sql`, `crates/server/src/db/claude_auth.rs` (new), `crates/server/src/routes/claude_auth.rs` (new), `orchestrator/run.sh`, `deploy/claude-sync.sh` (new), `deploy/install-desktop-sync.sh` (new), `web/src/` (status indicator) |
| 1.1 | `deploy/install.sh`, `docs/server-toml-example.toml`, `docs/agent-env-example.env` |
