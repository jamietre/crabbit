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

# ── Step 4: Fetch next issue ─────────────────────────────────────────────────

log "Fetching next issue..."
NEXT_ISSUE=$(api_get "/agent/next-issue")

if [ "$NEXT_ISSUE" = "null" ] || [ -z "$NEXT_ISSUE" ]; then
    log "No pending issues. Marking idle and exiting."
    api_put "/agent/state" '{"status": "idle"}' > /dev/null
    exit 0
fi

REPO_ID=$(echo "$NEXT_ISSUE"      | jq -r '.repo_id')
REPO_OWNER=$(echo "$NEXT_ISSUE"   | jq -r '.repo_owner')
REPO_NAME=$(echo "$NEXT_ISSUE"    | jq -r '.repo_name')
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

# ── Step 6: Fetch GitHub token ────────────────────────────────────────────────

log "Fetching GitHub token..."
AUTH_STATUS=$(api_get "/github/status")
GH_CONNECTED=$(echo "$AUTH_STATUS" | jq -r '.connected')

if [ "$GH_CONNECTED" != "true" ]; then
    log "GitHub not connected. Marking task failed."
    api_patch "/tasks/${TASK_ID}" '{"status": "failed", "error_message": "GitHub account not connected. Visit the crabbit UI to connect."}' > /dev/null
    api_put "/agent/state" '{"status": "idle", "current_task_id": null}' > /dev/null
    exit 1
fi

# The server returns the token decrypted when called with ?include_token=true
# (only available to bearer token holders — protected by API key middleware).
GH_TOKEN=$(api_get "/github/status?include_token=true" | jq -r '.access_token // empty')

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

# ── Step 8: Build prompt ──────────────────────────────────────────────────────

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
