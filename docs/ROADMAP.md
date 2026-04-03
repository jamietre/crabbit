# Crabbit Roadmap

## Deployment context

Crabbit runs on a personal Proxmox server inside an LXC container on a private network. It is not exposed to the public internet. Security hardening, multi-user support, and web UI authentication are explicitly out of scope until the system is proven reliable enough to warrant them.

The target deployment is a plain Linux environment (Debian/Ubuntu). Agent tasks run inside Docker containers (one per task), which provide isolation and a clean toolchain environment without requiring the host to have language runtimes installed.

---

## Current state (April 2026)

The core loop works end-to-end. GitHub issues are picked up, Claude runs inside a Docker container matched to the repo's language toolchain, and pull requests are opened. The system has completed real tasks successfully.

**Recently shipped:**
- Docker-based toolchain management: built-in images for Node, Rust, Python, Go; custom toolchain builder with Claude-assisted install step generation
- Pre-built `crabbit-server` binary published via GitHub Releases (no Rust required to install)
- Claude credential sync daemon (desktop → server push, no SSH required)
- Stuck task recovery on startup
- Toolchain image availability check before spawning tasks
- Timeout on docker pull/build (10 min)

**Known gaps (see below):**
- Session resumption is broken — workspace and Claude session state are lost on container exit
- No task-level timeout — a hung Claude run blocks the agent indefinitely
- Agent sleep (`wake_at`) requires manual intervention to wake
- No notifications when action is needed

---

## Security gaps

Lower priority given the private-network deployment context, but worth tracking.

| # | Item | Status |
|---|------|--------|
| S1 | **GH_TOKEN in env var** — visible via `docker inspect`; anyone with Docker socket access can extract the live token. Mitigate with Docker secrets or a short-lived scoped token issued per-task | todo |
| S2 | **Prompt injection via issue body** — issue title/body goes directly into Claude's prompt without sanitization; a malicious issue could influence Claude's behaviour. Wrap issue content in an explicit "untrusted content" delimiter in the prompt | todo |
| S3 | **`--dangerously-skip-permissions` is unconditional** — Claude can do anything in the container including arbitrary GitHub API calls via the injected token; no boundary between "work on the repo" and "interact with GitHub generally" | todo |
| S4 | **`--network host` scope** — container can reach anything the host can reach, not just the crabbit API; a custom Docker network exposing only the API port would be more locked down | todo |

---

## Reliability gaps

Items identified in the April 2026 workflow analysis. Ordered roughly by impact.

| # | Item | Status |
|---|------|--------|
| R1 | **Session resumption** — persist workspace + Claude session state across container runs via named Docker volume per task | planning done, implementing |
| R2 | **Task-level timeout** — hard-kill container and mark task `failed` after N minutes (suggested: 90 min) | todo |
| R3 | **Agent wakeup** — when `wake_at` is set and reached, automatically re-trigger `/agent/run`; currently requires manual action | todo |
| R4 | **Server restart race** — on restart, `reset_in_progress_tasks()` can cause two containers to work the same task if one is still running | todo |
| R5 | **Silent API failure from container** — all `api_patch()` calls use `|| true`; if server is unreachable the outcome is silently lost | todo |
| R6 | **PR verification** — after `pr_created` outcome, verify the PR URL actually exists via GitHub API | todo |
| R7 | **GitHub state check on resume** — before resuming a `needs_human` or `retrying` task, check that the branch and any existing PR are still in expected state | todo |

---

## Workflow gaps

| # | Item | Status |
|---|------|--------|
| W1 | **Notifications** — webhook (Slack/Discord/generic) when task moves to `pr_created` or `needs_human` | todo |
| W2 | **`needs_human` reply path** — no mechanism to send a human's answer back to resume a stalled task; currently a dead end | todo |
| W3 | **Prompt injection framing** — issue body goes directly into Claude's prompt; wrap it as explicitly untrusted content | todo |
| W4 | **`completion_prompt` as system message** — per-repo instructions are appended to the user message; should be a proper Claude system prompt for reliability | todo |
| W5 | **Toolchain re-detection** — detection is one-shot at repo creation; add a UI button and/or re-detect on toolchain change | todo |
| W6 | **GitHub issue sync visibility** — how issues enter the queue (poll interval, webhook setup) is not surfaced in the UI | todo |

---

## Phase 1 — Deployable and self-recovering ✓

*Goal: run unattended on a server without needing to SSH in to fix things.*

- [x] **1.1** Linux install script (`install.sh` — binary download, no Rust build required)
- [x] **1.2** Claude CLI auth in headless environment (credential sync daemon: desktop monitors `~/.claude/.credentials.json`, pushes to server API)
- [x] **1.3** Stuck task recovery on startup (reset `in_progress` → `pending` on server start)
- [ ] **1.4** GitHub token expiry detection — 401 from GitHub API should mark auth disconnected, surface warning in UI rather than silently failing
- [ ] **1.5** Claude auth failure detection — detect auth failures by exit code/output pattern, surface as distinct task status

---

## Phase 2 — Better outcomes

*Goal: PRs that are more likely to be correct and mergeable without manual intervention.*

- [x] **2.1** Repo-level agent instructions — `completion_prompt` per repo injected into prompt
- [ ] **2.2** `AGENTS.md` / `CRABBIT.md` injection — check target repo for agent instructions file, inject into prompt
- [ ] **2.3** Confidence-based clarifying questions — tune prompt so Claude posts a question and exits `needs_human` rather than guessing when requirements are unclear
- [ ] **2.4** Per-task cost tracking — store `cost_usd` from stream-json result on task row; surface in UI
- [ ] **2.5** Test result capture — record pass/fail from test run as a task event; block PR creation on test failure
- [ ] **2.6** CI status tracking — poll GitHub checks API after PR created; update task record when CI resolves

---

## Phase 3 — Operational polish

*Goal: a system that is pleasant to leave running indefinitely.*

- [ ] **3.1** Task completion notifications (→ W1 above)
- [ ] **3.2** GitHub webhook trigger — replace 30-min polling with `issues.labeled` webhook for near-instant task pickup
- [ ] **3.3** PR feedback loop — when reviewer leaves comments on a Crabbit PR, create a follow-up task to address them
- [ ] **3.4** Persistent workspace cache — keep cloned repos in a named Docker volume per repo, use `git fetch` instead of fresh clone each run; reduces startup time for large repos (→ R1 partially addresses this for in-progress tasks)
- [ ] **3.5** Concurrent task execution — allow N containers in parallel (one per repo, or a pool); requires removing the singleton agent state model

---

## Explicitly out of scope (for now)

- Web UI authentication — private network deployment makes this unnecessary
- Multi-user support — single-owner tool
- Public hosting / SaaS
- `--network host` scoping — low risk on private LAN; revisit if deployment context changes
