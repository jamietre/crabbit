#!/usr/bin/env bash
# Crabbit container entrypoint.
# Receives all task context as environment variables set by the host server.
set -euo pipefail

: "${CRABBIT_API_URL:?}"
: "${CRABBIT_TASK_ID:?}"
: "${GH_TOKEN:?}"
: "${CRABBIT_REPO_OWNER:?}"
: "${CRABBIT_REPO_NAME:?}"
: "${CRABBIT_ISSUE_NUMBER:?}"
: "${CRABBIT_ISSUE_TITLE:?}"
: "${CRABBIT_ISSUE_URL:?}"
: "${CRABBIT_ISSUE_BODY:?}"

export GITHUB_TOKEN="$GH_TOKEN"

WORKDIR="$(mktemp -d /tmp/crabbit-XXXXXX)"
REPO_DIR="${WORKDIR}/repo"
OUTCOME_FILE="${WORKDIR}/outcome.json"
SCREENSHOTS_DIR="${WORKDIR}/screenshots"
CLAUDE_LOG="${WORKDIR}/claude.jsonl"
SESSION_FILE="${WORKDIR}/session.json"
mkdir -p "$SCREENSHOTS_DIR"

log()  { echo "[crabbit] $*" >&2; }
die()  { log "ERROR: $*"; exit 1; }

api_get()   { curl -sf "${CRABBIT_API_URL}/api/v1${1}"; }
api_patch() { curl -sf -X PATCH -H 'Content-Type: application/json' -d "$2" "${CRABBIT_API_URL}/api/v1${1}" > /dev/null || true; }
api_post()  { curl -sf -X POST  -H 'Content-Type: application/json' -d "$2" "${CRABBIT_API_URL}/api/v1${1}" > /dev/null || true; }

post_event() {
    local type="$1" payload="$2"
    curl -sf -X POST -H 'Content-Type: application/json' \
        -d "{\"event_type\": \"${type}\", \"payload\": ${payload}}" \
        "${CRABBIT_API_URL}/api/v1/tasks/${CRABBIT_TASK_ID}/events" > /dev/null || true
}

fail_task() {
    local msg="$1"
    log "$msg"
    api_patch "/tasks/${CRABBIT_TASK_ID}" \
        "{\"status\": \"failed\", \"error_message\": $(printf '%s' "$msg" | jq -Rs .)}"
    exit 0
}

# ── Clone repo ────────────────────────────────────────────────────────────────

log "Cloning ${CRABBIT_REPO_OWNER}/${CRABBIT_REPO_NAME}..."
GH_ERR="${WORKDIR}/gh-err.txt"
if ! gh repo clone "${CRABBIT_REPO_OWNER}/${CRABBIT_REPO_NAME}" "$REPO_DIR" -- --quiet 2>"$GH_ERR"; then
    fail_task "Clone failed: $(cat "$GH_ERR")"
fi
log "Repo ready at ${REPO_DIR}"

# ── Fetch Claude settings ─────────────────────────────────────────────────────

SETTINGS=$(api_get "/claude-settings" || echo '{}')
CLAUDE_MODEL=$(printf '%s' "$SETTINGS" | jq -r '.model // "claude-sonnet-4-6"')
CLAUDE_EFFORT=$(printf '%s' "$SETTINGS" | jq -r '.effort_level // "high"')
CLAUDE_BUDGET=$(printf '%s' "$SETTINGS" | jq -r '.max_budget_usd // empty')

# ── Build prompt ──────────────────────────────────────────────────────────────

PRIOR_CONTEXT="${CRABBIT_PRIOR_CONTEXT:-}"
COMPLETION_PROMPT="${CRABBIT_COMPLETION_PROMPT:-}"

PRIOR_SECTION=""
if [ -n "$PRIOR_CONTEXT" ]; then
    PRIOR_SECTION="## Prior Context

${PRIOR_CONTEXT}

"
fi

COMPLETION_SECTION=""
if [ -n "$COMPLETION_PROMPT" ]; then
    COMPLETION_SECTION="## Repository-specific instructions

${COMPLETION_PROMPT}

"
fi

PROMPT="# Crabbit Task: Resolve GitHub Issue

You are an autonomous agent resolving a GitHub issue. Work methodically.
Use the \`gh\` CLI for all GitHub operations (GH_TOKEN is already set in your environment).

## Issue Details

- **Repository**: ${CRABBIT_REPO_OWNER}/${CRABBIT_REPO_NAME}
- **Issue number**: ${CRABBIT_ISSUE_NUMBER}
- **Title**: ${CRABBIT_ISSUE_TITLE}
- **URL**: ${CRABBIT_ISSUE_URL}

### Issue Body

${CRABBIT_ISSUE_BODY}

## Working Directory

The repository is cloned at: ${REPO_DIR}

Work only within this directory. Do not modify files outside it.

${PRIOR_SECTION}${COMPLETION_SECTION}## Objective

1. Read and understand the issue. Read the relevant source files.
2. Implement a fix on a new feature branch.
3. Write or update tests if applicable. Run them.
4. Push your branch and open a pull request:
   \`\`\`
   git push origin <branch-name>
   gh pr create --title \"...\" --body \"...\" --base main
   \`\`\`
5. If you cannot resolve the issue without human input, post a comment and set your outcome to \`needs_human\`.

## Reporting Your Outcome

Write your outcome to: ${OUTCOME_FILE}

If you created a PR:
{ \"result\": \"pr_created\", \"pr_url\": \"https://github.com/...\", \"pr_number\": 42, \"message\": \"Brief summary\" }

If you asked a question in the issue:
{ \"result\": \"question_asked\", \"question\": \"The exact question\", \"context_summary\": \"## Context\\n...\", \"message\": \"Brief description\" }

If you need human input:
{ \"result\": \"needs_human\", \"message\": \"What you need\" }

If the issue cannot be resolved:
{ \"result\": \"failed\", \"message\": \"Why\" }

## Reporting Events (optional)

curl -s -X POST ${CRABBIT_API_URL}/api/v1/tasks/${CRABBIT_TASK_ID}/events \\
  -H 'Content-Type: application/json' \\
  -d '{\"event_type\": \"progress\", \"payload\": {\"message\": \"...\"}}'

## Constraints

- Work only within ${REPO_DIR}
- Create a feature branch before making changes
- Run the project's test suite before creating a PR
- Do not push directly to main or master
"

# ── Run Claude ────────────────────────────────────────────────────────────────

CLAUDE_ARGS=(
    --model "$CLAUDE_MODEL"
    --dangerously-skip-permissions
    --output-format stream-json
    --verbose
)
[ -n "$CLAUDE_BUDGET" ] && CLAUDE_ARGS+=(--max-budget-tokens "$CLAUDE_BUDGET")

api_patch "/tasks/${CRABBIT_TASK_ID}" '{"status": "in_progress"}'

# Stream output to log and forward to API in background
stream_output() {
    local last=0
    while sleep 1; do
        [ -f "$CLAUDE_LOG" ] || continue
        local count
        count=$(wc -l < "$CLAUDE_LOG" 2>/dev/null || echo 0)
        if [ "$count" -gt "$last" ]; then
            while IFS= read -r line; do
                [ -z "$line" ] && continue
                post_event "claude_output" "{\"line\": $(printf '%s' "$line" | jq -Rs .)}"
            done < <(tail -n "+$((last + 1))" "$CLAUDE_LOG" 2>/dev/null)
            last=$count
        fi
    done
}
stream_output &
STREAM_PID=$!
trap 'kill "$STREAM_PID" 2>/dev/null || true' EXIT

# Resume from prior session if available
if [ -n "${CRABBIT_SESSION_ID:-}" ]; then
    CLAUDE_ARGS+=(--resume "$CRABBIT_SESSION_ID")
fi

claude "${CLAUDE_ARGS[@]}" -p "$PROMPT" > "$CLAUDE_LOG" 2>&1 || true

kill "$STREAM_PID" 2>/dev/null || true

# Extract and report session ID
SESSION_ID=$(grep -o '"session_id":"[^"]*"' "$CLAUDE_LOG" 2>/dev/null | tail -1 | cut -d'"' -f4 || true)
if [ -n "$SESSION_ID" ]; then
    post_event "claude_session_start" "{\"session_id\": \"${SESSION_ID}\"}"
fi

# ── Process outcome ───────────────────────────────────────────────────────────

if [ ! -f "$OUTCOME_FILE" ]; then
    fail_task "Agent did not write an outcome file"
fi

RESULT=$(jq -r '.result // "unknown"' "$OUTCOME_FILE")

case "$RESULT" in
    pr_created)
        PR_URL=$(jq -r '.pr_url // ""' "$OUTCOME_FILE")
        PR_NUM=$(jq -r '.pr_number // 0' "$OUTCOME_FILE")
        api_patch "/tasks/${CRABBIT_TASK_ID}" \
            "{\"status\": \"pr_created\", \"pr_url\": $(printf '%s' "$PR_URL" | jq -Rs .), \"pr_number\": ${PR_NUM}}"
        ;;
    question_asked)
        MSG=$(jq -c '.question // "question asked"' "$OUTCOME_FILE")
        api_patch "/tasks/${CRABBIT_TASK_ID}" \
            "{\"status\": \"needs_human\", \"error_message\": ${MSG}}"
        ;;
    needs_human)
        MSG=$(jq -c '.message // "needs human"' "$OUTCOME_FILE")
        api_patch "/tasks/${CRABBIT_TASK_ID}" \
            "{\"status\": \"needs_human\", \"error_message\": ${MSG}}"
        ;;
    failed)
        MSG=$(jq -c '.message // "failed"' "$OUTCOME_FILE")
        api_patch "/tasks/${CRABBIT_TASK_ID}" \
            "{\"status\": \"failed\", \"error_message\": ${MSG}}"
        ;;
    usage_limit)
        WAKE_AT=$(jq -r '.wake_at // 0' "$OUTCOME_FILE")
        api_patch "/tasks/${CRABBIT_TASK_ID}" '{"status": "pending"}'
        api_patch "/agent/state" "{\"status\": \"sleeping\", \"wake_at\": ${WAKE_AT}, \"current_task_id\": null}"
        ;;
    *)
        fail_task "Unknown result: ${RESULT}"
        ;;
esac

log "Done — result: ${RESULT}"
