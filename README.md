# crabbit

Crabbit is an autonomous GitHub issue agent. It watches your repositories for open issues, picks them up one at a time, invokes Claude Code to implement a fix, and opens a pull request — all without human intervention.


---

## Current State

Very early. It worked at least once. No security. No guardrails. Recommend only running in an isolated container or VM.

---

## How it works

1. A systemd timer fires every 30 minutes.
2. The orchestrator checks whether the agent is sleeping (rate-limit backoff). If not, it fetches the next unworked issue from any configured repo.
3. It clones/updates the repo, renders a task prompt, and invokes `claude --print --dangerously-skip-permissions`.
4. Claude works in the repo directory, creates a branch, commits changes, and opens a PR using `gh`.
5. Claude writes an outcome file (`pr_created`, `needs_human`, `failed`, or `usage_limit`).
6. The orchestrator reads the outcome, updates the task record in the API, and goes back to sleep.

The web UI (served by the same binary) shows agent status, task history, and event timelines.

---

## Requirements

| Tool | Notes |
|------|-------|
| Rust (stable) | Build the server binary |
| Node 24 + pnpm | Build the SvelteKit frontend |
| `claude` CLI | Anthropic Claude Code — must be authenticated |
| `gh` CLI | GitHub CLI — used by Claude to create PRs |
| `jq` | JSON processing in the orchestrator |
| `python3` | Safe prompt template rendering |
| `git` | Repo cloning |
| systemd (user) | Service/timer management (Linux) |

All of these except systemd are managed by [mise](https://mise.jdx.dev/) via `mise.toml`.

---

## Setup

### 1. Register a GitHub OAuth App

Go to **GitHub → Settings → Developer settings → OAuth Apps → New OAuth App**.

- **Application name**: crabbit (or anything)
- **Homepage URL**: `http://localhost:3000`
- **Authorization callback URL**: `http://localhost:3000/api/v1/auth/github/callback`

Note the **Client ID** and generate a **Client Secret**.

### 2. Create the server config

```bash
mkdir -p ~/.config/crabbit
cp config/server.toml ~/.config/crabbit/server.toml
```

Edit `~/.config/crabbit/server.toml`:

```toml
bind = "127.0.0.1:3000"
db_path = "/home/YOU/.local/share/crabbit/crabbit.db"
api_key = "$(openssl rand -hex 32)"          # generate a strong key
encryption_key_hex = "$(openssl rand -hex 32)" # separate key for token encryption

[github_oauth]
client_id     = "YOUR_CLIENT_ID"
client_secret = "YOUR_CLIENT_SECRET"
```

### 3. Create the agent config

```bash
cp config/agent.env ~/.config/crabbit/agent.env
```

Edit `~/.config/crabbit/agent.env` — set `CRABBIT_API_KEY` to the same value as `api_key` in `server.toml`.

### 4. Install

```bash
./deploy/install.sh
```

This builds the binary, installs it to `~/.local/bin/`, copies the orchestrator scripts, and registers the systemd units.

### 5. Start

```bash
# Edit configs first (step 2 & 3 above), then:
systemctl --user start crabbit-server
```

Open `http://localhost:3000` in your browser and connect your GitHub account via **Auth**.

Then add repos via **Repos**, and start the agent timer:

```bash
systemctl --user start crabbit-agent.timer
```

---

## Development

### Build

```bash
# Install tools (requires mise)
mise install
mise run web:install

# Full build (web assets + Rust binary)
mise run build
```

### Run locally

```bash
# Terminal 1 — Rust server (uses ~/.config/crabbit/server.toml)
mise run server:dev

# Terminal 2 — SvelteKit dev server (proxies /api → localhost:3000)
mise run web:dev
```

The web dev server starts at `http://localhost:5173` with hot reload. The API server runs at `http://localhost:3000`.

### Available mise tasks

| Task | Description |
|------|-------------|
| `mise run build` | Web build → Rust release build |
| `mise run web:build` | SvelteKit static build only |
| `mise run web:install` | `pnpm install` in `web/` |
| `mise run web:dev` | Vite dev server |
| `mise run server:build` | `cargo build --release` |
| `mise run server:dev` | `cargo run` with config |
| `mise run clean` | Remove `web/build` and `target` |

### Tests

```bash
cargo test
```

37 tests: unit tests for DB queries, crypto, routes, and one integration test covering the full task lifecycle.

---

## Dry-run the orchestrator

You can test the orchestrator against a running server without invoking Claude:

```bash
CRABBIT_CONFIG=~/.config/crabbit/agent.env sh orchestrator/run.sh --dry-run
```

Expected output:
```
[2026-...] Orchestrator started (DRY_RUN=1)
[2026-...] Checking agent state...
[2026-...] Marking agent as running...
[2026-...] Fetching next issue...
[2026-...] Next issue: owner/repo#N — Issue Title
[2026-...] DRY_RUN: would process owner/repo#N as task #M
```

---

## Project layout

```
crates/
  common/       Shared Rust types (models, serialisation)
  server/       Axum HTTP server, SQLite DB, embed layer
web/            SvelteKit frontend (built to web/build/, embedded in binary)
orchestrator/   Shell agent loop + Claude prompt template
deploy/         systemd units + install script
docs/           Example config files
mise.toml       Tool versions and build tasks
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for a detailed walkthrough.
