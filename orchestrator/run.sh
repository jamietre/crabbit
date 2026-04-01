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

# If the agent env sets CLAUDE_CONFIG_DIR, export it and ensure the directory
# exists so Claude does not load the user's personal settings.json / hooks.
if [ -n "${CLAUDE_CONFIG_DIR:-}" ]; then
    # Preserve .credentials.json so Claude can manage its own token rotation.
    # Remove everything else so personal settings/hooks don't bleed in.
    if [ -d "$CLAUDE_CONFIG_DIR" ] && [ -f "${CLAUDE_CONFIG_DIR}/.credentials.json" ]; then
        _saved_creds=$(cat "${CLAUDE_CONFIG_DIR}/.credentials.json")
        rm -rf "$CLAUDE_CONFIG_DIR"
        mkdir -p "$CLAUDE_CONFIG_DIR"
        printf '%s' "$_saved_creds" > "${CLAUDE_CONFIG_DIR}/.credentials.json"
        unset _saved_creds
    else
        rm -rf "$CLAUDE_CONFIG_DIR"
        mkdir -p "$CLAUDE_CONFIG_DIR"
    fi
    CLAUDE_CONFIG_DIR="$(cd "$CLAUDE_CONFIG_DIR" && pwd)"
    export CLAUDE_CONFIG_DIR
fi

# Validate required variables
for var in CRABBIT_API_URL WORKDIR; do
    eval "val=\${$var:-}"
    if [ -z "$val" ]; then
        echo "ERROR: $var is not set in $CONFIG" >&2
        exit 1
    fi
done

DRY_RUN=0
if [ "${1:-}" = "--dry-run" ]; then DRY_RUN=1; fi

# ── Cleanup trap ─────────────────────────────────────────────────────────────
# On unexpected exit, reset agent state to idle so it doesn't get stuck.
# Set _state_managed=1 before any exit that already handles agent state.
_state_managed=0
cleanup() {
    [ "$_state_managed" = "1" ] && return
    curl -sf -X PUT \
        -H "Content-Type: application/json" \
        -d '{"status": "idle", "current_task_id": null}' \
        "${CRABBIT_API_URL}/api/v1/agent/state" > /dev/null 2>&1 || true
}
trap cleanup EXIT

# ── Helpers ──────────────────────────────────────────────────────────────────

_log_buffer=""
post_log() {
    curl -sf -X POST \
        -H "Content-Type: application/json" \
        -d "{\"event_type\": \"orchestrator_log\", \"payload\": {\"message\": $(printf '%s' "$1" | jq -Rs .)}}" \
        "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null 2>&1 || true
}
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
    if [ -n "${TASK_ID:-}" ]; then
        # Flush any buffered pre-task logs first
        if [ -n "$_log_buffer" ]; then
            printf '%s\n' "$_log_buffer" | while IFS= read -r _msg; do
                [ -n "$_msg" ] && post_log "$_msg"
            done
            _log_buffer=""
        fi
        post_log "$*"
    else
        _log_buffer="${_log_buffer}${_log_buffer:+
}$*"
    fi
}
die() { echo "ERROR: $*" >&2; exit 1; }

api_get() {
    # api_get <path> → stdout JSON
    curl -sf \
        -H "Accept: application/json" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

api_put() {
    # api_put <path> <json-body>
    curl -sf -X PUT \
        -H "Content-Type: application/json" \
        -d "$2" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

api_post() {
    # api_post <path> <json-body>
    curl -sf -X POST \
        -H "Content-Type: application/json" \
        -d "$2" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

api_patch() {
    # api_patch <path> <json-body>
    curl -sf -X PATCH \
        -H "Content-Type: application/json" \
        -d "$2" \
        "${CRABBIT_API_URL}/api/v1${1}"
}

mkdir -p "$WORKDIR/repos" "$WORKDIR/screenshots"

# Directory containing this script and the prompt template
CRABBIT_ORCHESTRATOR_DIR="$(cd "$(dirname "$0")" && pwd)"

log "Orchestrator started (DRY_RUN=$DRY_RUN)"

# ── Step 2: Check sleep state ────────────────────────────────────────────────

log "Checking agent state..."
AGENT_STATE=$(api_get "/agent/state")
STATUS=$(printf '%s\n' "$AGENT_STATE" | jq -r '.status')
WAKE_AT=$(printf '%s\n' "$AGENT_STATE" | jq -r '.wake_at // 0')

if [ "$STATUS" = "sleeping" ]; then
    NOW=$(date +%s)
    if [ "$WAKE_AT" -gt "$NOW" ]; then
        MINS=$(( (WAKE_AT - NOW) / 60 ))
        log "Agent sleeping. Wake in ~${MINS}m (at $(date -d "@${WAKE_AT}" 2>/dev/null || date -r "${WAKE_AT}" 2>/dev/null || printf '%s\n' "${WAKE_AT}")). Exiting."
        _state_managed=1; exit 0
    fi
    log "Sleep window has passed, resuming."
fi

# ── Step 3: Mark running ──────────────────────────────────────────────────────

log "Marking agent as running..."
api_put "/agent/state" '{"status": "running"}' > /dev/null

# ── Step 4: Fetch next issue ─────────────────────────────────────────────────

log "Fetching next issue..."
NEXT_ISSUE=$(api_get "/agent/next-issue")

if [ "$NEXT_ISSUE" = "null" ] || [ -z "$NEXT_ISSUE" ]; then
    log "No pending issues. Marking idle and exiting."
    api_put "/agent/state" '{"status": "idle"}' > /dev/null
    _state_managed=1; exit 0
fi

REPO_ID=$(printf '%s\n' "$NEXT_ISSUE"      | jq -r '.repo_id')
REPO_OWNER=$(printf '%s\n' "$NEXT_ISSUE"   | jq -r '.repo_owner')
REPO_NAME=$(printf '%s\n' "$NEXT_ISSUE"    | jq -r '.repo_name')
ISSUE_NUMBER=$(printf '%s\n' "$NEXT_ISSUE" | jq -r '.issue_number')
ISSUE_TITLE=$(printf '%s\n' "$NEXT_ISSUE"  | jq -r '.issue_title')
ISSUE_URL=$(printf '%s\n' "$NEXT_ISSUE"    | jq -r '.issue_url')
ISSUE_BODY=$(printf '%s\n' "$NEXT_ISSUE"   | jq -r '.issue_body')
EXISTING_TASK_ID=$(printf '%s\n' "$NEXT_ISSUE" | jq -r '.existing_task_id // empty')

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
    TASK_ID=$(printf '%s\n' "$TASK" | jq -r '.id')
    log "Created task #${TASK_ID}"
fi

# Mark in progress
api_patch "/tasks/${TASK_ID}" '{"status": "in_progress"}' > /dev/null
api_put "/agent/state" "{\"status\": \"running\", \"current_task_id\": ${TASK_ID}}" > /dev/null

if [ "$DRY_RUN" = "1" ]; then
    log "DRY_RUN: would process ${REPO_OWNER}/${REPO_NAME}#${ISSUE_NUMBER} as task #${TASK_ID}"
    api_patch "/tasks/${TASK_ID}" '{"status": "pending"}' > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    _state_managed=1; exit 0
fi

# ── Step 6: Fetch GitHub token ────────────────────────────────────────────────

log "Fetching GitHub token..."
AUTH_STATUS=$(api_get "/auth/github/status")
GH_CONNECTED=$(printf '%s\n' "$AUTH_STATUS" | jq -r '.connected')

if [ "$GH_CONNECTED" != "true" ]; then
    log "GitHub not connected. Marking task failed."
    api_patch "/tasks/${TASK_ID}" '{"status": "failed", "error_message": "GitHub account not connected. Visit the crabbit UI to connect."}' > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    _state_managed=1; exit 1
fi

# The server returns the token decrypted when called with ?include_token=true
# (only available to bearer token holders — protected by API key middleware).
GH_TOKEN=$(api_get "/auth/github/status?include_token=true" | jq -r '.access_token // empty')

if [ -z "$GH_TOKEN" ]; then
    die "Could not retrieve GitHub token from API"
fi

export GH_TOKEN
export GITHUB_TOKEN="$GH_TOKEN"  # gh CLI reads either

# Determine if we need to work via a fork.
# If the authenticated bot account is not the repo owner, we fork the repo so
# we can push branches and open PRs without needing direct collaborator access.
GH_LOGIN=$(printf '%s\n' "$AUTH_STATUS" | jq -r '.github_login // empty')
FORK_OWNER="$REPO_OWNER"
if [ -n "$GH_LOGIN" ] && [ "$GH_LOGIN" != "$REPO_OWNER" ]; then
    FORK_OWNER="$GH_LOGIN"
    log "Bot login ($GH_LOGIN) differs from repo owner ($REPO_OWNER) — will use fork ${FORK_OWNER}/${REPO_NAME}"
    # Create the fork if it doesn't exist yet (idempotent, exits 0 if already forked)
    GH_ERR="${WORKDIR}/gh-error.txt"
    if ! gh repo fork "${REPO_OWNER}/${REPO_NAME}" --clone=false 2>"$GH_ERR"; then
        log "WARNING: gh repo fork failed (may already exist): $(cat "$GH_ERR")"
    fi
fi

# ── Step 7: Clone / update repo ───────────────────────────────────────────────

REPO_DIR="${WORKDIR}/repos/${REPO_OWNER}/${REPO_NAME}"

_gh_auth_check() {
    # Call after any gh/git failure to detect expired GitHub token
    local stderr_file="$1"
    if grep -qiE "401|authentication|credentials|token|unauthorized" "$stderr_file" 2>/dev/null; then
        log "GitHub authentication failed — token may have expired. Reconnect via the Crabbit UI."
        api_patch "/tasks/${TASK_ID}" \
            '{"status": "failed", "error_message": "GitHub authentication failed — token expired. Reconnect via the Crabbit UI."}' \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        _state_managed=1; exit 0
    fi
}

GH_ERR="${WORKDIR}/gh-error.txt"
if [ -d "${REPO_DIR}/.git" ]; then
    log "Updating existing clone at ${REPO_DIR}..."
    if [ "$FORK_OWNER" != "$REPO_OWNER" ]; then
        # Ensure upstream remote is set, then sync from it
        git -C "$REPO_DIR" remote set-url upstream "https://github.com/${REPO_OWNER}/${REPO_NAME}.git" 2>/dev/null \
            || git -C "$REPO_DIR" remote add upstream "https://github.com/${REPO_OWNER}/${REPO_NAME}.git" 2>/dev/null || true
        if ! git -C "$REPO_DIR" fetch --quiet upstream 2>"$GH_ERR"; then
            _gh_auth_check "$GH_ERR"
        fi
        git -C "$REPO_DIR" checkout --quiet main 2>/dev/null \
            || git -C "$REPO_DIR" checkout --quiet master 2>/dev/null \
            || true
        git -C "$REPO_DIR" reset --quiet --hard upstream/HEAD 2>/dev/null || true
    else
        if ! git -C "$REPO_DIR" fetch --quiet origin 2>"$GH_ERR"; then
            _gh_auth_check "$GH_ERR"
        fi
        git -C "$REPO_DIR" checkout --quiet main 2>/dev/null \
            || git -C "$REPO_DIR" checkout --quiet master 2>/dev/null \
            || true
        git -C "$REPO_DIR" reset --quiet --hard origin/HEAD 2>/dev/null || true
    fi
else
    mkdir -p "$(dirname "$REPO_DIR")"
    if [ "$FORK_OWNER" != "$REPO_OWNER" ]; then
        log "Cloning fork ${FORK_OWNER}/${REPO_NAME}..."
        if ! gh repo clone "${FORK_OWNER}/${REPO_NAME}" "$REPO_DIR" -- --quiet 2>"$GH_ERR"; then
            _gh_auth_check "$GH_ERR"
            die "gh repo clone (fork) failed"
        fi
        # Add upstream so we can sync and so gh pr create targets the right repo
        git -C "$REPO_DIR" remote add upstream "https://github.com/${REPO_OWNER}/${REPO_NAME}.git"
    else
        log "Cloning ${REPO_OWNER}/${REPO_NAME}..."
        if ! gh repo clone "${REPO_OWNER}/${REPO_NAME}" "$REPO_DIR" -- --quiet 2>"$GH_ERR"; then
            _gh_auth_check "$GH_ERR"
            die "gh repo clone failed"
        fi
    fi
fi

log "Repo ready at ${REPO_DIR}"

# ── Step 8: Fetch Claude settings ────────────────────────────────────────────

CLAUDE_SETTINGS=$(api_get "/claude-settings")
CLAUDE_MODEL=$(printf '%s\n' "$CLAUDE_SETTINGS"  | jq -r '.model')
CLAUDE_EFFORT=$(printf '%s\n' "$CLAUDE_SETTINGS" | jq -r '.effort_level')
CLAUDE_BUDGET=$(printf '%s\n' "$CLAUDE_SETTINGS" | jq -r '.max_budget_usd // empty')
CLAUDE_USAGE_LIMIT=$(printf '%s\n' "$CLAUDE_SETTINGS" | jq -r '.usage_limit_pct // empty')
CLAUDE_PROMPT_APPEND=$(printf '%s\n' "$CLAUDE_SETTINGS" | jq -r '.system_prompt_append // empty')
ALLOW_BROWSER=$(printf '%s\n' "$CLAUDE_SETTINGS" | jq -r '.allow_browser_automation')

# ── Step 8c: Check Claude Pro usage percentage ────────────────────────────────

# Fetch usage from Anthropic API using the OAuth token.
# Store the result in agent state and enforce usage_limit_pct if configured.
CLAUDE_USAGE_PCT=""
CLAUDE_USAGE_RESET=""

get_oauth_token() {
    # Try CLAUDE_CONFIG_DIR first (written by step 8b), then fallback to env var or global
    for creds in "${CLAUDE_CONFIG_DIR:-}/.credentials.json" "${HOME}/.claude/.credentials.json"; do
        [ -f "$creds" ] || continue
        tok=$(jq -r '.claudeAiOauth.accessToken // empty' "$creds" 2>/dev/null)
        [ -n "$tok" ] && [ "$tok" != "null" ] && { printf '%s' "$tok"; return 0; }
    done
    [ -n "${CLAUDE_CODE_OAUTH_TOKEN:-}" ] && { printf '%s' "$CLAUDE_CODE_OAUTH_TOKEN"; return 0; }
    return 1
}

OAUTH_TOKEN=$(get_oauth_token 2>/dev/null || true)
if [ -n "$OAUTH_TOKEN" ]; then
    USAGE_RESP=$(curl -sf --max-time 10 \
        -H "Accept: application/json" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${OAUTH_TOKEN}" \
        -H "anthropic-beta: oauth-2025-04-20" \
        -H "User-Agent: crabbit/1.0" \
        "https://api.anthropic.com/api/oauth/usage" 2>/dev/null || true)

    if [ -n "$USAGE_RESP" ] && printf '%s\n' "$USAGE_RESP" | jq -e . > /dev/null 2>&1; then
        CLAUDE_USAGE_PCT=$(printf '%s\n' "$USAGE_RESP" | jq -r '.seven_day.utilization // empty')
        USAGE_RESET_ISO=$(printf '%s\n' "$USAGE_RESP" | jq -r '.seven_day.resets_at // empty')
        # Convert ISO timestamp to unix epoch
        if [ -n "$USAGE_RESET_ISO" ]; then
            CLAUDE_USAGE_RESET=$(date -d "$USAGE_RESET_ISO" +%s 2>/dev/null || true)
        fi

        log "Claude Pro 7-day usage: ${CLAUDE_USAGE_PCT}%"

        # Store in agent state
        UPDATE_BODY=$(jq -nc \
            --argjson pct "${CLAUDE_USAGE_PCT:-null}" \
            --argjson reset "${CLAUDE_USAGE_RESET:-null}" \
            '{usage_pct_7d: $pct, usage_reset_at: $reset}')
        api_put "/agent/state" "$UPDATE_BODY" > /dev/null 2>&1 || true

        # Enforce limit if configured
        if [ -n "$CLAUDE_USAGE_LIMIT" ] && [ -n "$CLAUDE_USAGE_PCT" ]; then
            # Compare as integers (floor)
            PCT_INT=$(printf '%s\n' "$CLAUDE_USAGE_PCT" | awk '{printf "%.0f", $1}')
            LIMIT_INT=$(printf '%s\n' "$CLAUDE_USAGE_LIMIT" | awk '{printf "%.0f", $1}')
            if [ "$PCT_INT" -ge "$LIMIT_INT" ]; then
                log "7-day usage ${PCT_INT}% >= limit ${LIMIT_INT}%. Resetting task to pending and sleeping."
                WAKE_AT="${CLAUDE_USAGE_RESET:-0}"
                if [ -z "$WAKE_AT" ] || [ "$WAKE_AT" = "0" ]; then
                    # Default: sleep 24 hours
                    WAKE_AT=$(( $(date +%s) + 86400 ))
                fi
                api_patch "/tasks/${TASK_ID}" '{"status": "pending"}' > /dev/null
                api_put "/agent/state" \
                    "{\"status\": \"sleeping\", \"wake_at\": ${WAKE_AT}, \"current_task_id\": null, \"usage_note\": \"7-day usage ${PCT_INT}% >= limit ${LIMIT_INT}%\"}" \
                    > /dev/null
                _state_managed=1; exit 0
            fi
        fi
    else
        log "Could not fetch Claude Pro usage (API unavailable or not a Pro account)."
    fi
else
    log "No OAuth token found; skipping usage check."
fi

# ── Step 8d: Fetch enabled prompts ───────────────────────────────────────────
# Retrieve all enabled prompts from the DB (via API) and concatenate their
# content into a single block that will be appended to the system prompt.

PROMPTS_JSON=$(api_get "/prompts" 2>/dev/null || echo "[]")
PROMPT_GUIDANCE=$(printf '%s\n' "$PROMPTS_JSON" \
    | jq -r '
        [.[] | select(.enabled == true)]
        | group_by(.category)
        | .[]
        | "## " + (.[0].category | ascii_upcase) + " GUIDANCE\n\n"
          + (map("### " + .name + (if .label != "" then " (" + .label + ")" else "" end) + "\n\n" + .content) | join("\n\n"))
    ' 2>/dev/null | tr -d '\r' || true)

if [ -n "$PROMPT_GUIDANCE" ]; then
    log "Fetched $(printf '%s\n' "$PROMPTS_JSON" | jq '[.[] | select(.enabled == true)] | length') enabled prompt(s) from database."
else
    log "No enabled prompts found in database."
fi

# ── Step 9: Build prompt ──────────────────────────────────────────────────────

PROMPT_FILE="${WORKDIR}/prompt.md"
OUTCOME_FILE="${WORKDIR}/outcome.json"
SCREENSHOTS_DIR="${WORKDIR}/screenshots"
TEMPLATE="${CRABBIT_ORCHESTRATOR_DIR}/prompt_template.md"

if [ ! -f "$TEMPLATE" ]; then
    die "prompt_template.md not found at $TEMPLATE"
fi

rm -f "$OUTCOME_FILE"
rm -rf "$SCREENSHOTS_DIR"
mkdir -p "$SCREENSHOTS_DIR"

# Fetch prior context summary if this is a resumed task (existing_task_id was set)
PRIOR_CONTEXT=""
if [ -n "$EXISTING_TASK_ID" ]; then
    log "Fetching prior context for task #${TASK_ID}..."
    TASK_EVENTS=$(api_get "/tasks/${TASK_ID}/events" 2>/dev/null || true)
    if [ -n "$TASK_EVENTS" ]; then
        # Extract the most recent context_summary event payload
        PRIOR_CONTEXT=$(printf '%s\n' "$TASK_EVENTS" \
            | jq -r '[.[] | select(.event_type == "context_summary")] | last | .payload.content // empty' \
            2>/dev/null || true)
    fi
    if [ -n "$PRIOR_CONTEXT" ]; then
        log "Prior context found ($(printf '%s' "$PRIOR_CONTEXT" | wc -c) chars). Injecting into prompt."
    else
        log "No prior context found for this task."
    fi
fi

# Render template by substituting CRABBIT_* placeholders
# Use Python for safe substitution (avoids sed issues with special chars in issue body)
python3 - <<PYEOF
import sys, re

with open("$TEMPLATE") as f:
    template = f.read()

prior_context = """${PRIOR_CONTEXT}"""

# Build the prior context section (injected where CRABBIT_PRIOR_CONTEXT_SECTION placeholder is)
if prior_context.strip():
    prior_context_section = (
        "## Prior Context\n\n"
        "This task was previously paused. Here is the context summary from when it was paused:\n\n"
        "---\n"
        + prior_context.strip()
        + "\n---\n\n"
        "Use this to understand the previous state and continue from where the agent left off.\n\n"
    )
else:
    prior_context_section = ""

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
    "CRABBIT_PRIOR_CONTEXT_SECTION": prior_context_section,
}

# Remove the browser testing section if allow_browser_automation is false
allow_browser = """${ALLOW_BROWSER}""" == "true"
if not allow_browser:
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

# ── Step 10: Build Claude flags ───────────────────────────────────────────────

# Build extra flags from settings
CLAUDE_FLAGS="--print --verbose --dangerously-skip-permissions"
CLAUDE_FLAGS="${CLAUDE_FLAGS} --model ${CLAUDE_MODEL}"
CLAUDE_FLAGS="${CLAUDE_FLAGS} --effort ${CLAUDE_EFFORT}"
CLAUDE_FLAGS="${CLAUDE_FLAGS} --output-format stream-json"

if [ -n "$CLAUDE_BUDGET" ]; then
    CLAUDE_FLAGS="${CLAUDE_FLAGS} --max-budget-usd ${CLAUDE_BUDGET}"
fi

if [ -n "$CLAUDE_PROMPT_APPEND" ] || [ -n "$PROMPT_GUIDANCE" ]; then
    APPEND_FILE="${WORKDIR}/system-append.txt"
    # Write prompt guidance first, then user-configured append
    : > "$APPEND_FILE"
    if [ -n "$PROMPT_GUIDANCE" ]; then
        printf '%s\n' "$PROMPT_GUIDANCE" >> "$APPEND_FILE"
    fi
    if [ -n "$CLAUDE_PROMPT_APPEND" ]; then
        [ -n "$PROMPT_GUIDANCE" ] && printf '\n' >> "$APPEND_FILE"
        printf '%s' "$CLAUDE_PROMPT_APPEND" >> "$APPEND_FILE"
    fi
    CLAUDE_FLAGS="${CLAUDE_FLAGS} --append-system-prompt @${APPEND_FILE}"
fi

if [ "$ALLOW_BROWSER" = "false" ]; then
    log "Browser automation disabled in settings — prompt will not mention Playwright."
fi

# Append any extra_flags from settings (stored as JSON array, convert to space-separated)
EXTRA_FLAGS_JSON=$(printf '%s\n' "$CLAUDE_SETTINGS" | jq -r '.extra_flags // []')
EXTRA_FLAGS=$(printf '%s\n' "$EXTRA_FLAGS_JSON" | jq -r '.[]' | tr '\n' ' ')
if [ -n "$EXTRA_FLAGS" ]; then
    CLAUDE_FLAGS="${CLAUDE_FLAGS} ${EXTRA_FLAGS}"
fi

# ── Step 11: Invoke Claude ────────────────────────────────────────────────────

log "Invoking Claude (model=${CLAUDE_MODEL}, effort=${CLAUDE_EFFORT})..."
log "Claude binary: $(command -v claude 2>/dev/null || echo 'NOT FOUND')"
log "CLAUDE_CONFIG_DIR: ${CLAUDE_CONFIG_DIR:-<unset>}"
log "Creds file exists: $([ -f "${CLAUDE_CONFIG_DIR:-}/.credentials.json" ] && echo yes || echo no)"
log "CLAUDE_* env vars: $(env | grep -i '^CLAUDE' | tr '\n' ' ' || echo none)"
# shellcheck disable=SC2086
log "Claude command: claude ${CLAUDE_FLAGS} < ${PROMPT_FILE}"

CLAUDE_EXIT=0
CLAUDE_LOG="${WORKDIR}/claude-output.jsonl"
CLAUDE_STDERR="${WORKDIR}/claude-stderr.txt"
: > "$CLAUDE_LOG"   # Clear before run so stale output doesn't confuse auth checks
: > "$CLAUDE_STDERR"

# Unset session-specific vars that leak from an outer Claude Code / CCS session
# and cause --print mode to fail with exit code 2.
unset CLAUDE_CODE_SSE_PORT
unset CLAUDE_CODE_RESTART_TOKEN
unset CLAUDE_CODE_PIPE_TIMEOUT

# Run Claude, capturing output to a file, then stream events from the log.
# Using file redirect (not pipe) avoids pipefail/dash interaction issues.
# shellcheck disable=SC2086
CLAUDE_EXIT=0
claude $CLAUDE_FLAGS < "$PROMPT_FILE" > "$CLAUDE_LOG" 2>"$CLAUDE_STDERR" || CLAUDE_EXIT=$?

# Post each output line as a task event
if [ -s "$CLAUDE_LOG" ]; then
    while IFS= read -r line; do
        curl -sf -X POST \
            -H "Content-Type: application/json" \
            -d "{\"event_type\": \"claude_output\", \"payload\": $(printf '%s\n' "$line" | jq -c '{line: .}' 2>/dev/null || echo '{"line": null}')}" \
            "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null 2>&1 || true
    done < "$CLAUDE_LOG"
fi

log "Claude exited with code ${CLAUDE_EXIT}"
if [ -s "$CLAUDE_STDERR" ]; then
    STDERR_CONTENT=$(cat "$CLAUDE_STDERR")
    log "Claude stderr: ${STDERR_CONTENT}"
    # Append stderr to the jsonl log so auth checks can scan it
    printf '%s\n' "$STDERR_CONTENT" >> "$CLAUDE_LOG"
fi

# ── Step 11: Read outcome ─────────────────────────────────────────────────────

# Check for Claude authentication failure before treating as a generic error.
if [ "$CLAUDE_EXIT" -ne 0 ] || grep -qiE "authentication_failed|oauth token has expired|not logged in|failed to authenticate" "$CLAUDE_LOG" 2>/dev/null; then
    if grep -qiE "oauth token has expired|token.*expired|expired.*token" "$CLAUDE_LOG" 2>/dev/null; then
        log "Claude OAuth token has expired — re-sync credentials via the crabbit UI or run 'mise run sync:claude-creds'."
        api_patch "/tasks/${TASK_ID}" \
            '{"status": "failed", "error_message": "Claude OAuth token has expired — re-sync credentials via '\''mise run sync:claude-creds'\'' then retry."}' \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        _state_managed=1; exit 0
    elif grep -qiE "not authenticated|authentication required|please log in|invalid credentials|not logged in|failed to authenticate|authentication_failed" "$CLAUDE_LOG" 2>/dev/null; then
        log "Claude CLI is not authenticated."
        api_patch "/tasks/${TASK_ID}" \
            '{"status": "failed", "error_message": "Claude CLI not authenticated — push credentials via the desktop sync daemon or run '\''claude login'\'' on the server."}' \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        _state_managed=1; exit 0
    fi
fi

if [ ! -f "$OUTCOME_FILE" ]; then
    log "WARNING: Claude did not write outcome.json. Marking as failed."
    api_patch "/tasks/${TASK_ID}" \
        "{\"status\": \"failed\", \"error_message\": \"Claude did not produce outcome.json (exit code: ${CLAUDE_EXIT})\"}" \
        > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    _state_managed=1; exit 0
fi

OUTCOME=$(cat "$OUTCOME_FILE")
RESULT=$(printf '%s\n' "$OUTCOME" | jq -r '.result // "failed"')
log "Outcome: ${RESULT}"

# ── Step 12: Upload screenshots ───────────────────────────────────────────────

for screenshot in "${SCREENSHOTS_DIR}"/*.png "${SCREENSHOTS_DIR}"/*.jpg; do
    [ -f "$screenshot" ] || continue
    FILENAME=$(basename "$screenshot")
    B64=$(base64 < "$screenshot" | tr -d '\n')
    curl -sf -X POST \
        -H "Content-Type: application/json" \
        -d "{\"event_type\": \"browser_screenshot\", \"payload\": {\"filename\": \"${FILENAME}\", \"base64\": \"${B64}\"}}" \
        "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}/events" > /dev/null \
        && log "Uploaded screenshot: ${FILENAME}" \
        || log "WARNING: failed to upload screenshot: ${FILENAME}"
done

# ── Step 13: Report outcome ───────────────────────────────────────────────────

case "$RESULT" in
    pr_created)
        PR_URL=$(printf '%s\n' "$OUTCOME"    | jq -r '.pr_url // ""')
        PR_NUMBER=$(printf '%s\n' "$OUTCOME" | jq -r '.pr_number // null')
        MESSAGE=$(printf '%s\n' "$OUTCOME"   | jq -r '.message // ""')
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

    question_asked)
        QUESTION=$(printf '%s\n' "$OUTCOME"        | jq -r '.question // ""')
        CONTEXT_SUMMARY=$(printf '%s\n' "$OUTCOME" | jq -r '.context_summary // ""')
        MESSAGE=$(printf '%s\n' "$OUTCOME"         | jq -r '.message // "Question posted to issue thread"')

        # Post the question as a comment on the GitHub issue
        if [ -n "$QUESTION" ]; then
            log "Posting question to ${REPO_OWNER}/${REPO_NAME}#${ISSUE_NUMBER}..."
            if gh issue comment "$ISSUE_NUMBER" \
                    --repo "${REPO_OWNER}/${REPO_NAME}" \
                    --body "$QUESTION" > /dev/null 2>&1; then
                log "Question posted to issue thread."
                api_post "/tasks/${TASK_ID}/events" \
                    "{\"event_type\": \"comment_posted\", \"payload\": {\"comment\": $(printf '%s\n' "$QUESTION" | jq -Rs .)}}" \
                    > /dev/null 2>&1 || true
            else
                log "WARNING: failed to post question to GitHub issue."
            fi
        fi

        # Save context summary as a task event so it can be loaded on resume
        if [ -n "$CONTEXT_SUMMARY" ]; then
            api_post "/tasks/${TASK_ID}/events" \
                "{\"event_type\": \"context_summary\", \"payload\": {\"content\": $(printf '%s\n' "$CONTEXT_SUMMARY" | jq -Rs .)}}" \
                > /dev/null 2>&1 || true
            log "Context summary saved."
        fi

        api_patch "/tasks/${TASK_ID}" \
            "{\"status\": \"needs_human\", \"error_message\": $(printf '%s\n' "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        log "Question asked: ${MESSAGE}"
        ;;

    needs_human)
        MESSAGE=$(printf '%s\n' "$OUTCOME" | jq -r '.message // "Human input required"')
        api_patch "/tasks/${TASK_ID}" \
            "{\"status\": \"needs_human\", \"error_message\": $(printf '%s\n' "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        log "Needs human: ${MESSAGE}"
        ;;

    failed)
        MESSAGE=$(printf '%s\n' "$OUTCOME" | jq -r '.message // "Unknown failure"')
        api_patch "/tasks/${TASK_ID}" \
            "{\"status\": \"failed\", \"error_message\": $(printf '%s\n' "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
        log "Failed: ${MESSAGE}"
        ;;

    usage_limit)
        WAKE_AT=$(printf '%s\n' "$OUTCOME" | jq -r '.wake_at // 0')
        MESSAGE=$(printf '%s\n' "$OUTCOME" | jq -r '.message // "Usage limit hit"')
        # Reset task to pending so it will be retried after wake
        api_patch "/tasks/${TASK_ID}" '{"status": "pending"}' > /dev/null
        api_put "/agent/state" \
            "{\"status\": \"sleeping\", \"wake_at\": ${WAKE_AT}, \"current_task_id\": null, \"usage_note\": $(printf '%s\n' "$MESSAGE" | jq -Rs .)}" \
            > /dev/null
        log "Usage limit hit. Sleeping until $(date -d "@${WAKE_AT}" 2>/dev/null || printf '%s\n' "${WAKE_AT}")."
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
_state_managed=1
exit 0
