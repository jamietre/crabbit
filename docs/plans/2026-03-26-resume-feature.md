# Resume Feature Plan

**Goal:** When Claude is interrupted mid-run (rate limit, server restart, crash), the task
resumes where it left off using `claude --resume <session_id>` rather than starting cold.

---

## Current Behaviour

1. Orchestrator invokes `claude --print ... < prompt_file`
2. Claude writes `outcome.json` on completion; orchestrator reads it and patches task status
3. On server restart: `reset_in_progress_tasks()` moves all `in_progress` → `pending`
4. Next orchestrator run picks up the task as if it were brand new; Claude gets the full
   prompt again and starts from scratch
5. `claude_session_id` is already in the schema but is never populated

---

## New Concept: `Interrupted` Status

| Status | Meaning |
|--------|---------|
| `pending` | Waiting for a fresh run; no prior context |
| `in_progress` | Actively running (locked) |
| `interrupted` | Was running; `claude_session_id` preserved; resume with `--resume` |
| `failed` | Terminal failure; retried up to 2× |
| `pr_created` / `needs_human` / `skipped` | Terminal states (unchanged) |

`interrupted` is semantically "pending but with context." The orchestrator treats it
identically to `pending` except it calls `--resume <session_id>` and skips prompt building.

---

## CLAUDE_CONFIG_DIR Change

**Problem:** The orchestrator currently does `rm -rf "$CLAUDE_CONFIG_DIR"` at startup to
prevent stale `.claude.json` state. This also deletes the `projects/` subdirectory where
Claude stores session conversation history — making `--resume` impossible.

**Fix:** Only delete `.claude.json`, not the whole directory:
```sh
rm -f "$CLAUDE_CONFIG_DIR/.claude.json"
mkdir -p "$CLAUDE_CONFIG_DIR"
```

The `projects/` tree is preserved across runs so `--resume <session_id>` can find its
conversation. `.claude.json` (the file that was causing the stale-state bug) is still
wiped fresh every run.

---

## Changes Required

### 1. `crates/common/src/models.rs`
- Add `Interrupted` variant to `TaskStatus`
- Add `claude_session_id: Option<String>` to `NextIssueResponse`

### 2. `crates/server/src/db/tasks.rs`
- `reset_in_progress_tasks()`: split into two queries:
  - Tasks with `claude_session_id IS NOT NULL` → set `interrupted`
  - Tasks with `claude_session_id IS NULL` → set `pending`
- Add `set_claude_session_id(conn, task_id, session_id)` helper

### 3. `crates/server/src/routes/agent.rs` — `next_issue`
New selection priority (in order):

1. **Interrupted tasks** — `status = 'interrupted'` (pick first, they have context)
2. **New GitHub issues** — issues without any task row
3. **Pending tasks** — `status = 'pending'` with no existing session
4. **Failed tasks for retry** — `status = 'failed'` AND `retry_count < 2`

For interrupted tasks, include `claude_session_id` in the response alongside `existing_task_id`.

### 4. `crates/server/src/routes/tasks.rs`
- Existing `PATCH /tasks/:id` already accepts arbitrary status; no change needed
- A new `POST /tasks/:id/start-fresh` endpoint (or reuse DELETE) that:
  - Clears `claude_session_id`
  - Sets status back to `pending`
  (This gives the user a way to discard the interrupted session and restart)

### 5. `orchestrator/run.sh`

**Startup: preserve sessions dir**
```sh
if [ -n "${CLAUDE_CONFIG_DIR:-}" ]; then
    rm -f "$CLAUDE_CONFIG_DIR/.claude.json"   # was: rm -rf
    mkdir -p "$CLAUDE_CONFIG_DIR"
    CLAUDE_CONFIG_DIR="$(cd "$CLAUDE_CONFIG_DIR" && pwd)"
    export CLAUDE_CONFIG_DIR
fi
```

**After `next_issue` response, extract session_id:**
```sh
CLAUDE_SESSION_ID=$(printf '%s\n' "$NEXT_ISSUE" | jq -r '.claude_session_id // empty')
```

**Step 9 (Build prompt): skip if resuming**
```sh
if [ -z "$CLAUDE_SESSION_ID" ]; then
    # ... existing prompt template rendering ...
fi
```

**Step 10 (Build flags): add --resume if session_id present**
```sh
if [ -n "$CLAUDE_SESSION_ID" ]; then
    CLAUDE_FLAGS="--print --verbose --dangerously-skip-permissions"
    CLAUDE_FLAGS="${CLAUDE_FLAGS} --output-format stream-json"
    CLAUDE_FLAGS="${CLAUDE_FLAGS} --resume ${CLAUDE_SESSION_ID}"
    log "Resuming Claude session ${CLAUDE_SESSION_ID}"
else
    # ... existing flag building with model/effort/budget ...
fi
```

When resuming, `--resume` replays the full conversation and continues from the last message.
We provide a brief continuation prompt via stdin:
```
(You were interrupted. Please continue from where you left off.)
```

**Step 11 (Invoke Claude): capture session_id from output**

Parse the `init` event (first line of stream-json output) to extract the session ID, then
immediately PATCH the task so it's preserved even if the run is interrupted again:
```sh
# Stream Claude output line-by-line; capture session_id from first init event
SESSION_CAPTURED=0
while IFS= read -r line; do
    if [ "$SESSION_CAPTURED" = "0" ]; then
        SID=$(printf '%s\n' "$line" | jq -r 'select(.type=="system" and .subtype=="init") | .session_id // empty' 2>/dev/null || true)
        if [ -n "$SID" ]; then
            api_patch "/tasks/${TASK_ID}" "{\"claude_session_id\": $(printf '%s' "$SID" | jq -Rs .)}" > /dev/null || true
            SESSION_CAPTURED=1
            CAPTURED_SESSION_ID="$SID"
            log "Captured Claude session ID: ${SID}"
        fi
    fi
    # post event as before
    printf '%s\n' "$line" >> "$CLAUDE_LOG"
done < <(claude $CLAUDE_FLAGS < "$PROMPT_FILE" 2>"$CLAUDE_STDERR"; echo $? > "$EXIT_CODE_FILE")
CLAUDE_EXIT=$(cat "$EXIT_CODE_FILE")
```

> Note: `< <(...)` process substitution requires bash. The shebang should stay `#!/usr/bin/env bash`
> (currently `sh`). If we must stay sh-compatible, an alternative is: run Claude to file, then
> parse the file for session_id and patch. Simpler to just read the file after.

**Simpler alternative (sh-compatible):** Run Claude → file as now, then parse session_id
from the log file before doing anything else:
```sh
claude $CLAUDE_FLAGS < "$PROMPT_FILE" > "$CLAUDE_LOG" 2>"$CLAUDE_STDERR" || CLAUDE_EXIT=$?

# Extract and persist session ID immediately after run
CAPTURED_SESSION_ID=$(jq -r 'select(.type=="system" and .subtype=="init") | .session_id // empty' \
    "$CLAUDE_LOG" 2>/dev/null | head -1)
if [ -n "$CAPTURED_SESSION_ID" ]; then
    api_patch "/tasks/${TASK_ID}" \
        "{\"claude_session_id\": $(printf '%s' "$CAPTURED_SESSION_ID" | jq -Rs .)}" \
        > /dev/null || true
    log "Session ID captured: ${CAPTURED_SESSION_ID}"
fi
```

**Cleanup trap: set interrupted instead of just resetting agent**

The cleanup trap fires on unexpected exit (SIGTERM, crash). It should leave the task as
`interrupted` (not abandoned) if we have a session ID:
```sh
cleanup() {
    [ "$_state_managed" = "1" ] && return
    if [ -n "${TASK_ID:-}" ] && [ -n "${CAPTURED_SESSION_ID:-}" ]; then
        curl -sf -X PATCH ... "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}" \
            -d "{\"status\": \"interrupted\"}" > /dev/null 2>&1 || true
    elif [ -n "${TASK_ID:-}" ]; then
        curl -sf -X PATCH ... "${CRABBIT_API_URL}/api/v1/tasks/${TASK_ID}" \
            -d '{"status": "pending"}' > /dev/null 2>&1 || true
    fi
    curl -sf -X PUT ... '{"status": "idle", "current_task_id": null}' ... > /dev/null 2>&1 || true
}
```

**`usage_limit` outcome: set interrupted instead of pending**

When Claude itself signals `usage_limit` (via outcome.json), we currently reset to `pending`.
With session capture, we can set `interrupted` so the conversation continues after wake:
```sh
usage_limit)
    api_patch "/tasks/${TASK_ID}" '{"status": "interrupted"}' > /dev/null
    api_put "/agent/state" "{\"status\": \"sleeping\", \"wake_at\": ${WAKE_AT}, ...}" > /dev/null
    ;;
```

### 6. Frontend (`web/`)

**`src/lib/types.ts`**
- Add `'interrupted'` to `TaskStatus` union
- Add `claude_session_id?: string` to `Task`

**`src/routes/tasks/[id]/+page.svelte`**
- New amber badge: `interrupted` status → "interrupted — will resume"
- New button: "Start Fresh" for interrupted tasks (calls `start-fresh` endpoint or DELETE)
  - Use case: session was corrupted, or user wants Claude to retry from a clean slate
- Existing Reset button: keep for `failed` / `needs_human`

---

## Open Questions

1. **`--resume` stdin prompt**: What should we send when resuming? Options:
   - Empty stdin (Claude just continues autonomously)
   - `"Please continue from where you left off."`
   - Nothing (omit stdin redirect entirely — Claude's `--resume` may not need it)

2. **Session expiry**: Claude sessions may expire after some time. If `--resume` fails
   because the session is gone, we should detect the error and fall back to `pending`
   (clear `claude_session_id`, set status to `pending`). Detection: exit code or stderr
   message like "session not found".

3. **Prompt template on resume**: When resuming, Claude already has all the context from the
   first run. We may want to include a short addendum noting the interruption cause
   (e.g. "You were interrupted due to a usage limit — your weekly quota has now reset.").

4. **`retry_count` vs `interrupted`**: Currently, interrupted tasks don't increment
   `retry_count`. That seems correct — a rate-limit interruption isn't a failure to retry.
   But if a resumed task then fails, the `retry_count` logic still applies.

---

## Implementation Order

1. CLAUDE_CONFIG_DIR fix (rm -f .claude.json instead of rm -rf)
2. `Interrupted` status + DB migration
3. Session ID capture in orchestrator → PATCH task
4. `reset_in_progress_tasks` splits on session_id presence
5. Cleanup trap update
6. `usage_limit` outcome → interrupted
7. `next_issue` prioritizes interrupted + returns session_id
8. Orchestrator `--resume` path (skip prompt, add flag)
9. Frontend interrupted badge + Start Fresh button
10. Session-not-found fallback handling
