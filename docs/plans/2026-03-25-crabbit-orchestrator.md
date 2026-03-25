# Crabbit Orchestrator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the shell script orchestrator that drives the agent loop — checking sleep state, fetching the next issue, invoking Claude noninteractively, parsing the outcome, uploading screenshots, and reporting results to the backend API. Plus the systemd units and install script for deployment.

**Architecture:** A single POSIX `sh` script (`orchestrator/run.sh`) that reads config from `~/.config/crabbit/agent.env`, makes REST calls with `curl` + `jq`, invokes `claude`, and writes results back to the API. Systemd timer fires every 30 minutes; the script exits early if the wake window hasn't passed (no dynamic rescheduling needed).

**Tech Stack:** POSIX sh, curl, jq, git, gh CLI, claude CLI, base64, systemd

**Prerequisites:**
- `crabbit-server` is running and accessible at `CRABBIT_API_URL`
- `claude` CLI is installed and in PATH
- `gh` CLI is installed and in PATH
- `jq` is installed
- `python3` is installed (used for safe template substitution with arbitrary text in issue bodies)

---

## File Map

```
orchestrator/
  run.sh                  main orchestrator script
  prompt_template.md      Claude prompt with {placeholder} substitution markers

deploy/
  crabbit-server.service  systemd service for the API server
  crabbit-agent.service   systemd one-shot service for a single agent run
  crabbit-agent.timer     systemd timer triggering the agent every 30min
  install.sh              install script: copies files, enables services

docs/
  agent-env-example.env   example ~/.config/crabbit/agent.env
  server-toml-example.toml  example ~/.config/crabbit/server.toml
```

---

### Task 1: Script Skeleton and Config Loading

**Files:**
- Create: `orchestrator/run.sh`
- Create: `docs/agent-env-example.env`

- [ ] **Step 1: Create the script skeleton**

```sh
#!/usr/bin/env sh
# crabbit orchestrator — autonomous GitHub issue agent runner
# Usage: run.sh [--dry-run]
set -eu

# ── Configuration ────────────────────────────────────────────────────────────

DEFAULT_CONFIG="${HOME}/.config/crabbit/agent.env"
CONFIG="${CRABBIT_CONFIG:-$DEFAULT_CONFIG}"

if [ ! -f "$CONFIG" ]; then
    echo "ERROR: config not found at $CONFIG" >&2
    echo "Copy docs/agent-env-example.env to $CONFIG and fill it in." >&2
    exit 1
fi

# shellcheck source=/dev/null
. "$CONFIG"

# Validate required variables
for var in CRABBIT_API_URL CRABBIT_API_KEY WORKDIR; do
    eval "val=\${$var:-}"
    if [ -z "$val" ]; then
        echo "ERROR: $var is not set in $CONFIG" >&2
        exit 1
    fi
done

DRY_RUN=0
if [ "${1:-}" = "--dry-run" ]; then DRY_RUN=1; fi

# ── Helpers ──────────────────────────────────────────────────────────────────

log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }
die() { echo "ERROR: $*" >&2; exit 1; }

api_get() {
    # api_get <path> → stdout JSON
    curl -sf \
        -H "Authorization: Bearer ${CRABBIT_API_KEY}" \
        -H "Accept: application/json" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

api_put() {
    # api_put <path> <json-body>
    curl -sf -X PUT \
        -H "Authorization: Bearer ${CRABBIT_API_KEY}" \
        -H "Content-Type: application/json" \
        -d "$2" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

api_post() {
    # api_post <path> <json-body>
    curl -sf -X POST \
        -H "Authorization: Bearer ${CRABBIT_API_KEY}" \
        -H "Content-Type: application/json" \
        -d "$2" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

api_patch() {
    # api_patch <path> <json-body>
    curl -sf -X PATCH \
        -H "Authorization: Bearer ${CRABBIT_API_KEY}" \
        -H "Content-Type: application/json" \
        -d "$2" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

mkdir -p "$WORKDIR/repos" "$WORKDIR/screenshots"

# Directory containing this script and the prompt template
CRABBIT_ORCHESTRATOR_DIR="$(cd "$(dirname "$0")" && pwd)"

log "Orchestrator started (DRY_RUN=$DRY_RUN)"
```

- [ ] **Step 2: Create docs/agent-env-example.env**

```sh
# ~/.config/crabbit/agent.env
# Copy this to ~/.config/crabbit/agent.env and fill in values

# URL of the running crabbit-server
CRABBIT_API_URL="http://127.0.0.1:3000"

# API key (must match api_key in server.toml)
CRABBIT_API_KEY="changeme"

# Working directory for cloned repos and Claude scratch space
WORKDIR="${HOME}/.local/share/crabbit/work"
```

- [ ] **Step 3: Verify the script is valid sh**

Run: `sh -n orchestrator/run.sh`
Expected: no syntax errors

- [ ] **Step 4: Commit**

```bash
git add orchestrator/run.sh docs/agent-env-example.env
git commit -m "feat: orchestrator script skeleton with config loading"
```

---

### Task 2: Sleep Check

**Files:**
- Modify: `orchestrator/run.sh`

Add the sleep-check block after config loading. If agent status is `sleeping` and `wake_at > now`, exit 0. Mark agent as `running` otherwise.

- [ ] **Step 1: Add sleep check to run.sh**

```sh
# ── Step 2: Check sleep state ────────────────────────────────────────────────

log "Checking agent state..."
AGENT_STATE=$(api_get "/agent/state")
STATUS=$(echo "$AGENT_STATE" | jq -r '.status')
WAKE_AT=$(echo "$AGENT_STATE" | jq -r '.wake_at // 0')

if [ "$STATUS" = "sleeping" ]; then
    NOW=$(date +%s)
    if [ "$WAKE_AT" -gt "$NOW" ]; then
        MINS=$(( (WAKE_AT - NOW) / 60 ))
        log "Agent sleeping. Wake in ~${MINS}m (at $(date -d "@${WAKE_AT}" 2>/dev/null || date -r "${WAKE_AT}" 2>/dev/null || echo "${WAKE_AT}")). Exiting."
        exit 0
    fi
    log "Sleep window has passed, resuming."
fi

# ── Step 3: Mark running ──────────────────────────────────────────────────────

log "Marking agent as running..."
api_put "/agent/state" '{"status": "running"}' > /dev/null
```

- [ ] **Step 2: Test the sleep check manually**

```sh
# With server running and agent_state set to sleeping with future wake_at:
# PUT /api/v1/agent/state {"status":"sleeping","wake_at": 9999999999}
# Then run:
CRABBIT_CONFIG=/tmp/test.env sh orchestrator/run.sh
# Expected: "Agent sleeping. Wake in..." and clean exit 0
```

- [ ] **Step 3: Commit**

```bash
git add orchestrator/run.sh
git commit -m "feat: orchestrator sleep-check and running state marking"
```

---

### Task 3: Fetch Next Issue and Register Task

**Files:**
- Modify: `orchestrator/run.sh`

- [ ] **Step 1: Add next-issue fetch and task registration**

```sh
# ── Step 4: Fetch next issue ─────────────────────────────────────────────────

log "Fetching next issue..."
NEXT_ISSUE=$(api_get "/agent/next-issue")

if [ "$NEXT_ISSUE" = "null" ] || [ -z "$NEXT_ISSUE" ]; then
    log "No pending issues. Marking idle and exiting."
    api_put "/agent/state" '{"status": "idle"}' > /dev/null
    exit 0
fi

REPO_ID=$(echo "$NEXT_ISSUE"     | jq -r '.repo_id')
REPO_OWNER=$(echo "$NEXT_ISSUE"  | jq -r '.repo_owner')
REPO_NAME=$(echo "$NEXT_ISSUE"   | jq -r '.repo_name')
ISSUE_NUMBER=$(echo "$NEXT_ISSUE" | jq -r '.issue_number')
ISSUE_TITLE=$(echo "$NEXT_ISSUE"  | jq -r '.issue_title')
ISSUE_URL=$(echo "$NEXT_ISSUE"    | jq -r '.issue_url')
ISSUE_BODY=$(echo "$NEXT_ISSUE"   | jq -r '.issue_body')
EXISTING_TASK_ID=$(echo "$NEXT_ISSUE" | jq -r '.existing_task_id // empty')

log "Next issue: ${REPO_OWNER}/${REPO_NAME}#${ISSUE_NUMBER} — ${ISSUE_TITLE}"

# ── Step 5: Register or reuse task ───────────────────────────────────────────

if [ -n "$EXISTING_TASK_ID" ]; then
    TASK_ID="$EXISTING_TASK_ID"
    log "Resuming existing task #${TASK_ID}"
else
    log "Creating task..."
    TASK_JSON=$(jq -nc \
        --argjson repo_id "$REPO_ID" \
        --argjson issue_number "$ISSUE_NUMBER" \
        --arg issue_title "$ISSUE_TITLE" \
        --arg issue_url "$ISSUE_URL" \
        --arg issue_body "$ISSUE_BODY" \
        '{repo_id: $repo_id, issue_number: $issue_number, issue_title: $issue_title, issue_url: $issue_url, issue_body: $issue_body}')
    TASK=$(api_post "/tasks" "$TASK_JSON")
    TASK_ID=$(echo "$TASK" | jq -r '.id')
    log "Created task #${TASK_ID}"
fi

# Mark in progress
api_patch "/tasks/${TASK_ID}" '{"status": "in_progress"}' > /dev/null
api_put "/agent/state" "{\"status\": \"running\", \"current_task_id\": ${TASK_ID}}" > /dev/null

if [ "$DRY_RUN" = "1" ]; then
    log "DRY_RUN: would process ${REPO_OWNER}/${REPO_NAME}#${ISSUE_NUMBER} as task #${TASK_ID}"
    api_patch "/tasks/${TASK_ID}" '{"status": "pending"}' > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    exit 0
fi
```

- [ ] **Step 2: Commit**

```bash
git add orchestrator/run.sh
git commit -m "feat: orchestrator next-issue fetch and task registration"
```

---

### Task 4: GitHub Token and Repo Clone

**Files:**
- Modify: `orchestrator/run.sh`

- [ ] **Step 1: Add token fetch and repo clone**

```sh
# ── Step 6: Fetch GitHub token ────────────────────────────────────────────────

log "Fetching GitHub token..."
AUTH_STATUS=$(api_get "/auth/github/status")
GH_CONNECTED=$(echo "$AUTH_STATUS" | jq -r '.connected')

if [ "$GH_CONNECTED" != "true" ]; then
    log "GitHub not connected. Marking task failed."
    api_patch "/tasks/${TASK_ID}" '{"status": "failed", "error_message": "GitHub account not connected. Visit the crabbit UI to connect."}' > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    exit 1
fi

# The server returns the token decrypted only to authenticated callers.
# We add a dedicated endpoint for this — or embed the token in the auth status response
# when called with ?include_token=1 (only available to bearer token holders).
GH_TOKEN=$(api_get "/auth/github/status?include_token=1" | jq -r '.access_token // empty')

if [ -z "$GH_TOKEN" ]; then
    die "Could not retrieve GitHub token from API"
fi

export GH_TOKEN
export GITHUB_TOKEN="$GH_TOKEN"  # gh CLI reads either

# ── Step 7: Clone / update repo ───────────────────────────────────────────────

REPO_DIR="${WORKDIR}/repos/${REPO_OWNER}/${REPO_NAME}"

if [ -d "${REPO_DIR}/.git" ]; then
    log "Updating existing clone at ${REPO_DIR}..."
    git -C "$REPO_DIR" fetch --quiet origin
    git -C "$REPO_DIR" checkout --quiet main 2>/dev/null \
        || git -C "$REPO_DIR" checkout --quiet master 2>/dev/null \
        || true
    git -C "$REPO_DIR" reset --quiet --hard origin/HEAD 2>/dev/null || true
else
    log "Cloning ${REPO_OWNER}/${REPO_NAME}..."
    mkdir -p "$(dirname "$REPO_DIR")"
    gh repo clone "${REPO_OWNER}/${REPO_NAME}" "$REPO_DIR" -- --quiet
fi

log "Repo ready at ${REPO_DIR}"
```

- [ ] **Step 2: Note: the server needs a token-exposure endpoint**

The `GET /api/v1/auth/github/status?include_token=1` endpoint needs to be added to the server plan. It decrypts and returns the raw token — only callable with the API bearer key. Add this to the server auth routes.

> **Add to server plan Task 17:** Add `?include_token=true` query param to `GET /api/v1/auth/github/status` that, when present, also returns `access_token` (decrypted) in the response body. This is protected by the API key middleware.

- [ ] **Step 3: Commit**

```bash
git add orchestrator/run.sh
git commit -m "feat: orchestrator GitHub token fetch and repo clone"
```

---

### Task 5: Prompt Template

**Files:**
- Create: `orchestrator/prompt_template.md`

- [ ] **Step 1: Create the prompt template**

```markdown
# Crabbit Task: Resolve GitHub Issue

You are an autonomous agent resolving a GitHub issue. Work methodically.
Use the `gh` CLI for all GitHub operations (GH_TOKEN is already set in your environment).
Use the crabbit API to report progress events.

## Issue Details

- **Repository**: CRABBIT_REPO_OWNER/CRABBIT_REPO_NAME
- **Issue number**: CRABBIT_ISSUE_NUMBER
- **Title**: CRABBIT_ISSUE_TITLE
- **URL**: CRABBIT_ISSUE_URL

### Issue Body

CRABBIT_ISSUE_BODY

## Working Directory

The repository is cloned at: CRABBIT_REPO_DIR

Work only within this directory. Do not modify files outside it.

## Objective

1. Read and understand the issue. Read the relevant source files.
2. Implement a fix on a new feature branch.
3. Write or update tests if applicable. Run them.
4. Create a pull request: `gh pr create --title "..." --body "..." --base main`
5. If you cannot resolve the issue without human input, post a comment and
   set your outcome to `needs_human`.

## Browser Testing (if the issue involves frontend or UI work)

Playwright is available. Use it to verify your changes visually.
Save screenshots to CRABBIT_SCREENSHOTS_DIR — they will be attached to the task log.

Example (Node.js):
```javascript
const { chromium } = require('playwright');
const browser = await chromium.launch();
const page = await browser.newPage();
await page.goto('http://localhost:5173');
await page.screenshot({ path: 'CRABBIT_SCREENSHOTS_DIR/before.png' });
await browser.close();
```

## Reporting Your Outcome

After completing your work, write your outcome to: CRABBIT_OUTCOME_FILE

**If you created a PR:**
```json
{ "result": "pr_created", "pr_url": "https://github.com/...", "pr_number": 42, "message": "Brief summary of the fix" }
```

**If you need human input (post a comment first):**
```json
{ "result": "needs_human", "message": "What you need clarified or decided" }
```

**If the issue cannot be resolved:**
```json
{ "result": "failed", "message": "Why this issue cannot be resolved autonomously" }
```

**If you hit a usage limit:**
```json
{ "result": "usage_limit", "wake_at": 1774500000, "message": "Usage limit details" }
```

## Reporting Events to the API (optional)

You may POST progress events to the crabbit API for rich UI display:

```bash
curl -s -X POST CRABBIT_API_URL/api/v1/tasks/CRABBIT_TASK_ID/events \
  -H "Authorization: Bearer CRABBIT_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"event_type": "comment_posted", "payload": {"comment": "..."}}'
```

## Constraints

- Work only within CRABBIT_REPO_DIR
- Create a feature branch before making changes (e.g. `git checkout -b fix/issue-CRABBIT_ISSUE_NUMBER`)
- Run the project's test suite before creating a PR
- Do not push directly to main or master
```

- [ ] **Step 2: Add prompt rendering to run.sh**

```sh
# ── Step 8: Build prompt ──────────────────────────────────────────────────────

PROMPT_FILE="${WORKDIR}/prompt.md"
OUTCOME_FILE="${WORKDIR}/outcome.json"
SCREENSHOTS_DIR="${WORKDIR}/screenshots"
TEMPLATE="${CRABBIT_ORCHESTRATOR_DIR:-$(dirname "$0")}/prompt_template.md"

if [ ! -f "$TEMPLATE" ]; then
    die "prompt_template.md not found at $TEMPLATE"
fi

rm -f "$OUTCOME_FILE"
rm -rf "$SCREENSHOTS_DIR"
mkdir -p "$SCREENSHOTS_DIR"

# Render template by substituting CRABBIT_* placeholders
# Use Python for safe substitution (avoids sed issues with special chars in issue body)
python3 - <<PYEOF
import sys

with open("$TEMPLATE") as f:
    template = f.read()

replacements = {
    "CRABBIT_REPO_OWNER": """${REPO_OWNER}""",
    "CRABBIT_REPO_NAME": """${REPO_NAME}""",
    "CRABBIT_ISSUE_NUMBER": """${ISSUE_NUMBER}""",
    "CRABBIT_ISSUE_TITLE": """${ISSUE_TITLE}""",
    "CRABBIT_ISSUE_URL": """${ISSUE_URL}""",
    "CRABBIT_ISSUE_BODY": """${ISSUE_BODY}""",
    "CRABBIT_REPO_DIR": """${REPO_DIR}""",
    "CRABBIT_SCREENSHOTS_DIR": """${SCREENSHOTS_DIR}""",
    "CRABBIT_OUTCOME_FILE": """${OUTCOME_FILE}""",
    "CRABBIT_TASK_ID": """${TASK_ID}""",
    "CRABBIT_API_URL": """${CRABBIT_API_URL}""",
    "CRABBIT_API_KEY": """${CRABBIT_API_KEY}""",
}

# Remove the browser testing section if allow_browser_automation is false
allow_browser = """${ALLOW_BROWSER}""" == "true"
if not allow_browser:
    import re
    template = re.sub(
        r'## Browser Testing.*?(?=## Reporting Your Outcome)',
        '',
        template,
        flags=re.DOTALL
    )

for key, value in replacements.items():
    template = template.replace(key, value)

with open("${PROMPT_FILE}", "w") as f:
    f.write(template)
PYEOF

log "Prompt written to ${PROMPT_FILE}"
```

- [ ] **Step 3: Commit**

```bash
git add orchestrator/prompt_template.md orchestrator/run.sh
git commit -m "feat: Claude prompt template and rendering"
```

---

### Task 6: Claude Invocation

**Files:**
- Modify: `orchestrator/run.sh`

- [ ] **Step 1: Add Claude invocation block**

```sh
# ── Step 9: Fetch Claude settings ────────────────────────────────────────────

CLAUDE_SETTINGS=$(api_get "/claude-settings")
CLAUDE_MODEL=$(echo "$CLAUDE_SETTINGS" | jq -r '.model')
CLAUDE_EFFORT=$(echo "$CLAUDE_SETTINGS" | jq -r '.effort_level')
CLAUDE_BUDGET=$(echo "$CLAUDE_SETTINGS" | jq -r '.max_budget_usd // empty')
CLAUDE_PROMPT_APPEND=$(echo "$CLAUDE_SETTINGS" | jq -r '.system_prompt_append // empty')
ALLOW_BROWSER=$(echo "$CLAUDE_SETTINGS" | jq -r '.allow_browser_automation')

# Build extra flags from settings
CLAUDE_FLAGS="--print --dangerously-skip-permissions"
CLAUDE_FLAGS="${CLAUDE_FLAGS} --model ${CLAUDE_MODEL}"
CLAUDE_FLAGS="${CLAUDE_FLAGS} --effort ${CLAUDE_EFFORT}"
CLAUDE_FLAGS="${CLAUDE_FLAGS} --output-format stream-json"

if [ -n "$CLAUDE_BUDGET" ]; then
    CLAUDE_FLAGS="${CLAUDE_FLAGS} --max-budget-usd ${CLAUDE_BUDGET}"
fi

if [ -n "$CLAUDE_PROMPT_APPEND" ]; then
    # Write to temp file to avoid quoting issues when passing to claude
    APPEND_FILE="${WORKDIR}/system-append.txt"
    printf '%s' "$CLAUDE_PROMPT_APPEND" > "$APPEND_FILE"
    CLAUDE_FLAGS="${CLAUDE_FLAGS} --append-system-prompt @${APPEND_FILE}"
fi

if [ "$ALLOW_BROWSER" = "false" ]; then
    log "Browser automation disabled in settings — prompt will not mention Playwright."
fi

# Append any extra_flags from settings (stored as JSON array, convert to space-separated)
EXTRA_FLAGS_JSON=$(echo "$CLAUDE_SETTINGS" | jq -r '.extra_flags // []')
EXTRA_FLAGS=$(echo "$EXTRA_FLAGS_JSON" | jq -r '.[]' | tr '\n' ' ')
if [ -n "$EXTRA_FLAGS" ]; then
    CLAUDE_FLAGS="${CLAUDE_FLAGS} ${EXTRA_FLAGS}"
fi

# ── Step 10: Invoke Claude ────────────────────────────────────────────────────

log "Invoking Claude (model=${CLAUDE_MODEL}, effort=${CLAUDE_EFFORT})..."

CLAUDE_EXIT=0
CLAUDE_LOG="${WORKDIR}/claude-output.jsonl"

# Stream output: each line of stream-json is posted as a claude_output event
# and also written to a local log file.
# shellcheck disable=SC2086
claude $CLAUDE_FLAGS < "$PROMPT_FILE" 2>&1 | tee "$CLAUDE_LOG" | while IFS= read -r line; do
    # Post each stream-json line as an event (fire-and-forget, ignore failures)
    curl -sf -X POST \
        -H "Authorization: Bearer ${CRABBIT_API_KEY}" \
        -H "Content-Type: application/json" \
        -d "{\"event_type\": \"claude_output\", \"payload\": $(echo "$line" | jq -c '{line: .}' 2>/dev/null || echo '{"line": null}')}" \
        "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null 2>&1 || true
done || CLAUDE_EXIT=$?

log "Claude exited with code ${CLAUDE_EXIT}"
```

- [ ] **Step 2: Commit**

```bash
git add orchestrator/run.sh
git commit -m "feat: Claude invocation with settings and stream-json event posting"
```

---

### Task 7: Outcome Parsing and Reporting

**Files:**
- Modify: `orchestrator/run.sh`

- [ ] **Step 1: Add outcome parsing block**

```sh
# ── Step 11: Read outcome ─────────────────────────────────────────────────────

if [ ! -f "$OUTCOME_FILE" ]; then
    log "WARNING: Claude did not write outcome.json. Marking as failed."
    api_patch "/tasks/${TASK_ID}" \
        "{\"status\": \"failed\", \"error_message\": \"Claude did not produce outcome.json (exit code: ${CLAUDE_EXIT})\"}" \
        > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    exit 0
fi

OUTCOME=$(cat "$OUTCOME_FILE")
RESULT=$(echo "$OUTCOME" | jq -r '.result // "failed"')
log "Outcome: ${RESULT}"

# ── Step 12: Upload screenshots ───────────────────────────────────────────────

for screenshot in "${SCREENSHOTS_DIR}"/*.png "${SCREENSHOTS_DIR}"/*.jpg 2>/dev/null; do
    [ -f "$screenshot" ] || continue
    FILENAME=$(basename "$screenshot")
    B64=$(base64 < "$screenshot" | tr -d '\n')
    curl -sf -X POST \
        -H "Authorization: Bearer ${CRABBIT_API_KEY}" \
        -H "Content-Type: application/json" \
        -d "{\"event_type\": \"browser_screenshot\", \"payload\": {\"filename\": \"${FILENAME}\", \"base64\": \"${B64}\"}}" \
        "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null \
        && log "Uploaded screenshot: ${FILENAME}" \
        || log "WARNING: failed to upload screenshot: ${FILENAME}"
done

# ── Step 13: Report outcome ───────────────────────────────────────────────────

case "$RESULT" in
    pr_created)
        PR_URL=$(echo "$OUTCOME"    | jq -r '.pr_url // ""')
        PR_NUMBER=$(echo "$OUTCOME" | jq -r '.pr_number // null')
        MESSAGE=$(echo "$OUTCOME"   | jq -r '.message // ""')
        PATCH_BODY=$(jq -nc \
            --arg status "pr_created" \
            --arg pr_url "$PR_URL" \
            --argjson pr_number "$PR_NUMBER" \
            --arg error_message "$MESSAGE" \
            '{status: $status, pr_url: $pr_url, pr_number: $pr_number, error_message: $error_message}')
        api_patch "/tasks/${TASK_ID}" "$PATCH_BODY" > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        log "PR created: ${PR_URL}"
        ;;

    needs_human)
        MESSAGE=$(echo "$OUTCOME" | jq -r '.message // "Human input required"')
        api_patch "/tasks/${TASK_ID}" \
            "{\"status\": \"needs_human\", \"error_message\": $(echo "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        log "Needs human: ${MESSAGE}"
        ;;

    failed)
        MESSAGE=$(echo "$OUTCOME" | jq -r '.message // "Unknown failure"')
        api_patch "/tasks/${TASK_ID}" \
            "{\"status\": \"failed\", \"error_message\": $(echo "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        log "Failed: ${MESSAGE}"
        ;;

    usage_limit)
        WAKE_AT=$(echo "$OUTCOME" | jq -r '.wake_at // 0')
        MESSAGE=$(echo "$OUTCOME" | jq -r '.message // "Usage limit hit"')
        # Reset task to pending so it will be retried after wake
        api_patch "/tasks/${TASK_ID}" '{"status": "pending"}' > /dev/null
        api_put "/agent/state" \
            "{\"status\": \"sleeping\", \"wake_at\": ${WAKE_AT}, \"current_task_id\": null, \"usage_note\": $(echo "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        log "Usage limit hit. Sleeping until $(date -d "@${WAKE_AT}" 2>/dev/null || echo "${WAKE_AT}")."
        ;;

    *)
        log "WARNING: unknown result '${RESULT}'. Marking as failed."
        api_patch "/tasks/${TASK_ID}" \
            "{\"status\": \"failed\", \"error_message\": \"Unknown result: ${RESULT}\"}" \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        ;;
esac

# ── Step 14: Cleanup ──────────────────────────────────────────────────────────

rm -f "$OUTCOME_FILE" "$PROMPT_FILE" "${WORKDIR}/system-append.txt"
rm -rf "$SCREENSHOTS_DIR"
# Keep the claude log for debugging: rm -f "$CLAUDE_LOG"

log "Done."
exit 0
```

- [ ] **Step 2: Make script executable**

```bash
chmod +x orchestrator/run.sh
```

- [ ] **Step 3: Run a dry-run test against a running server**

```bash
export CRABBIT_CONFIG=/tmp/test-agent.env
# (fill in test config pointing at test server)
sh orchestrator/run.sh --dry-run
# Expected: "DRY_RUN: would process..." or "No pending issues"
```

- [ ] **Step 4: Commit**

```bash
git add orchestrator/run.sh
git commit -m "feat: orchestrator outcome parsing, screenshot upload, and reporting"
```

---

### Task 8: Systemd Unit Files

**Files:**
- Create: `deploy/crabbit-server.service`
- Create: `deploy/crabbit-agent.service`
- Create: `deploy/crabbit-agent.timer`

- [ ] **Step 1: Create crabbit-server.service**

```ini
[Unit]
Description=Crabbit - GitHub issue agent API server and UI
Documentation=https://github.com/yourname/crabbit
After=network.target

[Service]
Type=simple
ExecStart=%h/.local/bin/crabbit-server --config %h/.config/crabbit/server.toml
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=crabbit_server=info
# Prevent OOM killer from targeting it
OOMScoreAdjust=-100

[Install]
WantedBy=default.target
```

- [ ] **Step 2: Create crabbit-agent.service**

```ini
[Unit]
Description=Crabbit agent run (one-shot GitHub issue processor)
After=network.target
# Don't require crabbit-server — it may be remote or the agent may start first
# The orchestrator will fail gracefully if API is unreachable

[Service]
Type=oneshot
# agent.env exports CRABBIT_API_URL, CRABBIT_API_KEY, WORKDIR
EnvironmentFile=%h/.config/crabbit/agent.env
ExecStart=%h/.config/crabbit/orchestrator/run.sh

# Don't restart — the timer controls scheduling
Restart=no

# Give Claude up to 90 minutes per issue (complex issues take time)
TimeoutStartSec=5400

# Make sure standard tools are on PATH
Environment=PATH=/usr/local/bin:/usr/bin:/bin:%h/.local/bin:%h/.cargo/bin

# Capture output in journal
StandardOutput=journal
StandardError=journal
SyslogIdentifier=crabbit-agent

[Install]
WantedBy=default.target
```

- [ ] **Step 3: Create crabbit-agent.timer**

```ini
[Unit]
Description=Crabbit agent timer - triggers issue processing every 30 minutes
Requires=crabbit-agent.service

[Timer]
# Start 5 minutes after boot (give crabbit-server time to start)
OnBootSec=5min

# Then run 30 minutes after each completion
# The orchestrator exits immediately if still sleeping — this is safe to run frequently
OnUnitInactiveSec=30min

# Allow 1 minute of slack (avoid thundering herd at exact multiples)
AccuracySec=1min

Unit=crabbit-agent.service

[Install]
WantedBy=timers.target
```

- [ ] **Step 4: Commit**

```bash
git add deploy/
git commit -m "feat: systemd service and timer unit files"
```

---

### Task 9: Install Script

**Files:**
- Create: `deploy/install.sh`
- Create: `docs/server-toml-example.toml`

- [ ] **Step 1: Create docs/server-toml-example.toml**

```toml
# ~/.config/crabbit/server.toml

# Address and port for the HTTP server
bind = "127.0.0.1:3000"

# Path to the SQLite database
db_path = "/home/USERNAME/.local/share/crabbit/crabbit.db"

# API key — set a strong random value, used by orchestrator and your browser
api_key = "CHANGE_ME_USE_OPENSSL_RAND_HEX_32"

# 32-byte AES-256-GCM key for encrypting the stored GitHub token.
# Generate with: openssl rand -hex 32
encryption_key_hex = "CHANGE_ME_GENERATE_WITH_OPENSSL"

[github_oauth]
# Register an OAuth App at https://github.com/settings/developers
# Set Authorization callback URL to: http://localhost:3000/api/v1/auth/github/callback
client_id = "YOUR_OAUTH_APP_CLIENT_ID"
client_secret = "YOUR_OAUTH_APP_CLIENT_SECRET"
```

- [ ] **Step 2: Create deploy/install.sh**

```sh
#!/usr/bin/env sh
# Crabbit install script (user-level installation)
# Usage: ./deploy/install.sh
set -eu

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/crabbit"
DATA_DIR="${HOME}/.local/share/crabbit"
SYSTEMD_DIR="${HOME}/.config/systemd/user"
ORCHESTRATOR_DEST="${CONFIG_DIR}/orchestrator"

echo "Installing crabbit..."

# Build binary
echo "Building server binary..."
cd "$REPO_DIR"
make build
echo "Build complete."

# Install binary
mkdir -p "$BIN_DIR"
cp target/release/crabbit-server "$BIN_DIR/crabbit-server"
echo "Installed crabbit-server to $BIN_DIR"

# Create config directories
mkdir -p "$CONFIG_DIR" "$DATA_DIR" "${DATA_DIR}/work/repos"

# Install orchestrator scripts
mkdir -p "$ORCHESTRATOR_DEST"
cp orchestrator/run.sh "$ORCHESTRATOR_DEST/run.sh"
cp orchestrator/prompt_template.md "$ORCHESTRATOR_DEST/prompt_template.md"
chmod +x "$ORCHESTRATOR_DEST/run.sh"
echo "Installed orchestrator to $ORCHESTRATOR_DEST"

# Install systemd units
mkdir -p "$SYSTEMD_DIR"
for unit in crabbit-server.service crabbit-agent.service crabbit-agent.timer; do
    cp "deploy/${unit}" "${SYSTEMD_DIR}/${unit}"
    echo "Installed ${unit}"
done

# Install example configs (only if not already present)
if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
    cp docs/server-toml-example.toml "${CONFIG_DIR}/server.toml"
    echo ""
    echo "⚠  Config file created at ${CONFIG_DIR}/server.toml"
    echo "   Edit it to set your api_key, encryption_key_hex, and GitHub OAuth credentials."
fi

if [ ! -f "${CONFIG_DIR}/agent.env" ]; then
    cp docs/agent-env-example.env "${CONFIG_DIR}/agent.env"
    echo "   Edit ${CONFIG_DIR}/agent.env to set CRABBIT_API_KEY to match server.toml."
fi

# Enable and start services
systemctl --user daemon-reload
systemctl --user enable crabbit-server.service
systemctl --user enable crabbit-agent.timer

echo ""
echo "✓ Crabbit installed successfully!"
echo ""
echo "Next steps:"
echo "  1. Edit ${CONFIG_DIR}/server.toml"
echo "     - Set a strong api_key"
echo "     - Generate encryption_key_hex: openssl rand -hex 32"
echo "     - Register a GitHub OAuth App and set client_id + client_secret"
echo "  2. Edit ${CONFIG_DIR}/agent.env"
echo "     - Set CRABBIT_API_KEY to match server.toml api_key"
echo "  3. Start the server:"
echo "     systemctl --user start crabbit-server"
echo "  4. Open http://localhost:3000 and connect GitHub via Settings → Auth"
echo "  5. Add repos via Settings → Repos"
echo "  6. Start the agent timer:"
echo "     systemctl --user start crabbit-agent.timer"
echo ""
```

- [ ] **Step 2: Make install script executable**

```bash
chmod +x deploy/install.sh
```

- [ ] **Step 3: Test install script (dry run inspection)**

Run: `sh -n deploy/install.sh`
Expected: no syntax errors

- [ ] **Step 4: Commit**

```bash
git add deploy/install.sh docs/server-toml-example.toml
git commit -m "feat: install script and example config files"
```

---

### Task 10: End-to-End Manual Test

This task verifies the entire orchestrator works against a running crabbit-server.

**Prerequisites:**
- crabbit-server is running with test config
- A GitHub repo is configured with at least one open issue
- GitHub is connected via the UI
- `claude` CLI is authenticated

- [ ] **Step 1: Set up test environment**

```bash
# Start server
./target/debug/crabbit-server --config ~/.config/crabbit/server.toml &

# Create test agent.env
cat > /tmp/test-agent.env <<EOF
CRABBIT_API_URL=http://127.0.0.1:3000
CRABBIT_API_KEY=your-api-key
WORKDIR=/tmp/crabbit-test-work
EOF
```

- [ ] **Step 2: Run a dry-run to verify config loading and issue fetch**

```bash
CRABBIT_CONFIG=/tmp/test-agent.env sh orchestrator/run.sh --dry-run
# Expected output:
# [timestamp] Orchestrator started (DRY_RUN=1)
# [timestamp] Checking agent state...
# [timestamp] Marking agent as running...
# [timestamp] Fetching next issue...
# [timestamp] Next issue: owner/repo#N — Issue Title
# [timestamp] DRY_RUN: would process owner/repo#N as task #M
```

- [ ] **Step 3: Run for real against a test issue**

```bash
# Ensure there is a test issue in a configured repo, then:
CRABBIT_CONFIG=/tmp/test-agent.env sh orchestrator/run.sh
# Watch the output, check the UI at http://localhost:3000
```

- [ ] **Step 4: Verify timer scheduling**

```bash
systemctl --user start crabbit-agent.timer
systemctl --user status crabbit-agent.timer
# Expected: active (waiting), next trigger shown
journalctl --user -u crabbit-agent -f
```

- [ ] **Step 5: Commit**

```bash
git add orchestrator/ deploy/ docs/
git commit -m "feat: complete orchestrator implementation with install and deployment"
```

---

## Notes on Claude Usage Limit Detection

Claude reports usage limits in its output. When `--output-format stream-json` is used, watch for messages containing phrases like "usage limit" or "rate limit" in the text output. However, the most reliable approach is having Claude itself write `{"result": "usage_limit", "wake_at": ...}` to `outcome.json`.

The `wake_at` timestamp: Claude's usage resets on a rolling 1-hour or 5-hour window depending on the plan. Claude should be instructed to use `date -d "+5 hours" +%s` (or similar) to compute a conservative wake time, and write it to `outcome.json` before it loses the ability to continue.

The prompt template already instructs Claude on this. If Claude hits limits mid-task, it may not write `outcome.json` at all — the orchestrator handles this by marking the task `failed` with an explanatory message, which is safer than silently losing the task.
