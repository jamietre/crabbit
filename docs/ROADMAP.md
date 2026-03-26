# Crabbit Roadmap

## Deployment context

Crabbit runs on a personal Proxmox server inside an LXC container on a private network. It is not exposed to the public internet. Security hardening, multi-user support, and web UI authentication are explicitly out of scope until the system is proven reliable enough to warrant them.

The target deployment is a plain Linux environment (Debian/Ubuntu), not Docker. A general-purpose install script is the right packaging primitive — Docker adds unnecessary indirection when Proxmox LXC containers already provide the isolation.

---

## Current state (March 2026)

The core loop works end-to-end: a GitHub issue is picked up, Claude is invoked, and a pull request is opened. The system has completed at least one real task successfully. It is not yet reliable enough to run fully unattended.

Known gaps:
- Claude CLI authentication requires a browser OAuth flow that does not work headlessly
- A crashed orchestrator can leave tasks stuck in `in_progress` indefinitely
- GitHub token expiry fails silently
- No install script — all dependencies are installed ad-hoc on the host

---

## Phase 1 — Deployable and self-recovering

*Goal: run unattended on a server without needing to SSH in to fix things.*

### 1.1 Linux install script
Produce an `install.sh` that sets up Crabbit on a plain Debian/Ubuntu machine (the natural target for a Proxmox LXC container). The script should:
- Install system dependencies: `git`, `gh` CLI, `node`, `python3`, `curl`, `jq`
- Install the `claude` CLI
- Download or build the `crabbit-server` binary
- Copy the orchestrator scripts to `~/.config/crabbit/`
- Install and enable the systemd units (server service + agent timer)
- Print next steps for config and first-time auth

Docker is explicitly not the target — Proxmox LXC containers already provide the necessary isolation and Docker adds unnecessary indirection.

### 1.2 Claude CLI authentication in a headless container
The `claude` CLI uses browser-based OAuth which cannot run headlessly. The preferred solution is a **credential sync daemon** running on the desktop machine:

- A small watcher process monitors `~/.claude/.credentials.json` for changes (via `inotifywait` on Linux / FSEvents on macOS)
- On change, it POSTs the updated OAuth token to a Crabbit API endpoint (`PUT /api/v1/claude-auth`)
- The server stores it encrypted (same pattern as the GitHub token) and the orchestrator reads it at runtime via `CLAUDE_CODE_OAUTH_TOKEN`
- Re-authentication on the desktop propagates to the server automatically within seconds — no SSH required

This approach works across machines (not just same-host volume mounts), requires no filesystem coupling between desktop and server, and makes token rotation completely transparent. Since Crabbit runs on a private network, the token travels over LAN only; a shared secret header on the endpoint is sufficient protection.

The `install.sh` (1.1) should include a companion `install-desktop-sync.sh` that sets up the watcher as a background service on the desktop machine.

Other options (noted for completeness, not preferred):
- **Mount `~/.claude` as a volume** — only works when container is on the same host; requires SSH to re-auth
- **`ANTHROPIC_API_KEY`** — uses the pay-per-token API instead of Pro; loses usage tracking

### 1.3 Stuck task recovery on startup
Any task in `in_progress` status when the server starts indicates the previous run crashed without cleaning up. On startup, reset all `in_progress` tasks to `pending` and set agent status to `idle`.

### 1.4 GitHub token expiry detection
A 401 response from the GitHub API currently causes the orchestrator to fail with a generic error. Detect this specifically: mark `github_auth` as disconnected and surface a warning banner in the UI rather than consuming a task slot.

### 1.5 Claude auth failure detection
If the `claude` CLI cannot authenticate, the task fails with an opaque error message. Detect auth failures by exit code or output pattern and surface them as a distinct task status with a clear message ("Claude CLI not authenticated — re-auth required").

---

## Phase 2 — Better outcomes

*Goal: PRs that are more likely to be correct and mergeable without manual intervention.*

### 2.1 Repo-level agent instructions (`AGENTS.md` injection)
Before building the prompt, check the target repository for an `AGENTS.md` or `CRABBIT.md` file. If present, inject its contents into the prompt as additional context. This lets repo owners express preferences — testing conventions, code style, areas to avoid — without modifying Crabbit itself.

### 2.2 Confidence-based clarifying questions
Tune the prompt so that when Claude cannot determine the required behaviour with high confidence, it posts a comment on the issue and exits with `needs_human` rather than guessing. The target: fewer failed or off-target PRs, more actionable `needs_human` outcomes.

### 2.3 Per-task cost tracking
The `stream-json` result message already contains `cost_usd`. Store this on the task row. Surface per-task cost in the task detail view and running totals on the dashboard.

### 2.4 Test result capture
The prompt instructs Claude to run the test suite before opening a PR, but the results are not captured. Record whether tests passed or failed as a task event. If tests fail, Claude should not open a PR — strengthen this instruction and verify it holds in practice.

### 2.5 CI status tracking
After a PR is created, poll the GitHub checks API periodically and update the task record when CI passes or fails. Surface this in the task detail view so the outcome of a run is visible without leaving the dashboard.

---

## Phase 3 — Operational polish

*Goal: a system that is pleasant to leave running indefinitely.*

### 3.1 Task completion notifications
Send a notification when a PR is created or a task moves to `needs_human`. Initial target: post to a configurable webhook URL (Slack, Discord, or generic HTTP). This makes the system useful without actively watching the dashboard.

### 3.2 GitHub webhook trigger
Replace the 30-minute polling timer with a GitHub webhook on `issues.labeled`. This requires the server to be reachable from GitHub (or use a tunnel in a private network setup). Benefit: tasks start within seconds of an issue being labelled rather than up to 30 minutes later.

### 3.3 PR feedback loop
When a reviewer leaves comments on a PR that Crabbit opened, create a follow-up task to address them. This closes the loop between code review and the agent, reducing the need for human intervention after the initial PR.

### 3.4 Docker image
Produce a `Dockerfile` and `docker-compose.yml` as an alternative deployment path for users who prefer containers over a bare Linux install. The install script (1.1) remains the primary target for Proxmox LXC deployments.

### 3.5 Repo disk management
Cloned repositories accumulate in `WORKDIR/repos/`. Add a configurable retention policy: re-clone fresh each run (safest, avoids stale branch confusion), or keep clones and prune repos that have not been used in N days.

---

## Explicitly out of scope (for now)

- Web UI authentication — private network deployment makes this unnecessary
- Multi-user support — single-owner tool
- Public hosting / SaaS
- Concurrent task execution — one task at a time keeps the model simple
- Prompt injection / adversarial issue hardening — low risk in a private context
