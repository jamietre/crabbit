# Session Resumption via Docker Volumes
_2026-04-02_

## Problem

Claude's `--resume <session_id>` flag resumes a prior conversation, but two things must survive container exit for this to work:

1. **Workspace state**: the repo clone, git branch, uncommitted changes, node_modules, etc.  
   Currently lost — each run creates a random `/tmp/crabbit-XXXXXX` dir that disappears with the container.

2. **Claude session state**: `~/.claude/projects/` contains conversation history, tool call results, and session metadata that `--resume` reads.  
   Currently impossible to persist — `~/.claude` is mounted **read-only** from the host.

There is a third concern that complicates the solution:

3. **Claude credentials**: `~/.claude/claude.json` and related auth files must be injected into each container, but should be taken fresh each run so credential revocation takes effect immediately.

Currently all three are conflated into a single read-only host volume mount (`-v {claude_config_dir}:/root/.claude:ro`). This needs to be split apart.

---

## Solution

Use a **named Docker volume per task** mounted at `/workspace`. This volume stores both the repo clone and Claude's session state. Credentials are injected separately (read-only) and copied into the volume at startup on each run.

### Volume layout

```
/workspace/
  repo/               ← git clone of the target repo
  .claude/            ← Claude's config dir (session state + injected creds)
    claude.json       ← copied fresh from host creds on every container start
    projects/         ← session history; preserved across runs by the volume
    settings.json     ← copied fresh from host creds on every container start
  logs/               ← claude.jsonl and other ephemeral logs
```

### Docker run changes

**Before:**
```
-v {claude_config_dir}:/root/.claude:ro
```

**After:**
```
-v crabbit-task-{task_id}:/workspace
-v {claude_config_dir}:/creds:ro
```

The host `~/.claude` is now mounted at `/creds` (read-only) for credential injection only. The writable session state lives exclusively in the task volume.

### Credential injection in entrypoint.sh

At container startup, before running Claude:

```bash
# Set up .claude dir in the workspace volume
mkdir -p /workspace/.claude

# Always copy credentials fresh (so revocation takes effect)
cp /creds/claude.json /workspace/.claude/claude.json 2>/dev/null || true
cp /creds/settings.json /workspace/.claude/settings.json 2>/dev/null || true
# Copy any other top-level auth files (not the projects/ dir)
for f in /creds/*.json; do
    fname=$(basename "$f")
    cp "$f" "/workspace/.claude/$fname" 2>/dev/null || true
done

# Tell Claude to use /workspace/.claude instead of ~/.claude
export CLAUDE_CONFIG_DIR=/workspace/.claude
```

The `projects/` directory is intentionally NOT copied from host — it starts empty on first run, then accumulates session state that persists in the volume across subsequent runs.

### Repo clone logic in entrypoint.sh

```bash
WORKDIR=/workspace
REPODIR="$WORKDIR/repo"
mkdir -p "$WORKDIR"

if [ -d "$REPODIR/.git" ]; then
    # Resume path: fetch latest remote state but stay on current branch
    git -C "$REPODIR" fetch origin 2>/dev/null || true
else
    # First run: clone fresh
    gh repo clone "$CRABBIT_REPO_OWNER/$CRABBIT_REPO_NAME" "$REPODIR" \
        2>"$WORKDIR/logs/gh-err.txt" || fail_task "Failed to clone repo: $(cat $WORKDIR/logs/gh-err.txt)"
fi
```

---

## Volume Lifecycle

| Event | Volume action |
|---|---|
| First `docker run` for a task | Docker creates volume automatically (named `crabbit-task-{task_id}`) |
| Container exits, task → `needs_human` | Keep volume — agent will resume this task |
| Container exits, task → `pr_created`, `failed`, `skipped` | Delete volume: `docker volume rm crabbit-task-{task_id}` |
| Server restarts with orphaned volumes | Volumes persist until explicitly removed — tolerable for now |

Volume cleanup in `agent.rs` after the container exits:

```rust
// After docker exits, check task status and clean up if terminal
let is_terminal = s.with_db(|c| {
    crate::db::tasks::get_task(c, task.id)
        .map(|opt| opt.map(|t| t.is_terminal()).unwrap_or(false))
}).unwrap_or(false);

if is_terminal {
    let _ = tokio::process::Command::new("docker")
        .args(["volume", "rm", &format!("crabbit-task-{}", task.id)])
        .status()
        .await;
}
```

`is_terminal()` on `Task` returns true for: `PrCreated`, `Failed`, `Skipped`.  
`NeedsHuman` is NOT terminal — the volume must be kept for resume.

---

## Full Flow

### First run of a task

```
POST /agent/run
  → docker run --rm
      -v crabbit-task-42:/workspace        ← Docker creates volume
      -v ~/.claude:/creds:ro
      -e CRABBIT_TASK_ID=42
      -e CRABBIT_SESSION_ID=               ← empty on first run
      ...

  entrypoint.sh:
    mkdir -p /workspace/.claude /workspace/logs
    cp /creds/*.json → /workspace/.claude/  (credentials, fresh)
    export CLAUDE_CONFIG_DIR=/workspace/.claude
    /workspace/repo/.git not found → gh repo clone → /workspace/repo/
    claude --resume "" -p "$PROMPT" ...
      → writes session to /workspace/.claude/projects/...
      → writes claude.jsonl to /workspace/logs/claude.jsonl
    extract session_id from claude.jsonl
    POST /tasks/42/events {session_id}      ← server stores on task record
    outcome: needs_human
    PATCH /tasks/42 {status: needs_human}
    exit 0

  container removed (--rm), volume crabbit-task-42 KEPT
  agent reset to Idle
```

### Resume after needs_human

```
POST /agent/run
  → docker run --rm
      -v crabbit-task-42:/workspace        ← same volume, state preserved
      -v ~/.claude:/creds:ro
      -e CRABBIT_SESSION_ID=<session_id>   ← populated from task record
      ...

  entrypoint.sh:
    cp /creds/*.json → /workspace/.claude/  (credentials refreshed)
    export CLAUDE_CONFIG_DIR=/workspace/.claude
    /workspace/repo/.git found → git fetch origin (update refs only)
    claude --resume <session_id> -p "$PROMPT" ...
      → resumes conversation from session state in /workspace/.claude/projects/
      → working directory /workspace/repo still has branch + files intact
    ...
```

---

## Changes Required

### `docker/entrypoint.sh`

1. Replace `WORKDIR=$(mktemp -d /tmp/crabbit-XXXXXX)` with `WORKDIR=/workspace`
2. Add credential injection block before running Claude
3. Add `export CLAUDE_CONFIG_DIR=/workspace/.claude`
4. Replace unconditional `gh repo clone` with clone-or-fetch logic
5. Update all paths that assumed the old tmpdir structure (log paths, outcome path, screenshot path)

### `crates/server/src/routes/agent.rs`

1. Replace `-v {claude_config_dir}:/root/.claude:ro` with two volume args:
   - `-v crabbit-task-{task_id}:/workspace`
   - `-v {claude_config_dir}:/creds:ro`
2. Add `is_terminal()` check and `docker volume rm` after container exits

### `crates/common/src/models.rs`

1. Add `is_terminal() -> bool` method to `Task` (or `TaskStatus`)

### `crates/server/src/db/tasks.rs`

1. Verify `get_task(conn, id)` exists and returns the full Task (needed for cleanup check)

---

## Edge Cases

**What if the branch Claude was on was deleted remotely?**  
`git fetch origin` will update refs but leave the local branch intact. Claude can continue working on it. If it tries to push, it'll get a rejection — Claude should handle this as it would any push failure.

**What if the repo had a force-push to main since the last run?**  
The local clone will be behind but not broken. `git fetch` gets new refs. Claude would need to rebase its branch if it tries to merge — this is normal git workflow Claude should handle.

**What if the volume runs out of disk space?**  
Docker will report an error when writing to the volume. Claude will fail. No special handling needed beyond the existing task failure path.

**What if `CLAUDE_CONFIG_DIR` causes issues with the Claude CLI version in use?**  
The `CLAUDE_CONFIG_DIR` env var is confirmed to work (referenced in project memory re: orchestrator env). If it ever breaks, fallback is to change `HOME=/workspace` in entrypoint.sh instead.

**Orphaned volumes on server crash**  
Volumes for terminal tasks won't be cleaned up if the server crashes after the container exits. Acceptable for now — can add a startup sweep of `docker volume ls` filtered by `crabbit-task-*` names against task statuses later.

---

## What's Not Changing

- Container still uses `--rm` (ephemeral container, persistent volume)
- `--network host` unchanged
- All other env vars unchanged
- Toolchain image selection logic unchanged
- Task priority queue unchanged
