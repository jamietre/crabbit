# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- **Issue sync decoupled from agent execution** — `POST /sync` now fetches open issues from GitHub and upserts them as `queued` tasks independently of the agent run loop. Repos gain `labels_require`, `labels_ignore`, `labels_prioritize`, and `completion_prompt` fields to control which issues get picked up and what additional instructions Claude receives.
- **`queued` task status** — tasks created by sync start as `queued`; the agent promotes them to `in_progress` when it picks them up. Queued tasks can also be started manually via the UI.
- **Start button in task list** — queued, pending, and failed tasks show a Start button that dispatches `POST /tasks/:id/run`, triggering the orchestrator with `CRABBIT_TASK_ID` set.
- **Real-time Claude output streaming** — orchestrator now polls the JSONL log every second while Claude runs and posts `claude_output` events incrementally; the task detail page shows progress live rather than only after completion.
- **Background usage polling** — orchestrator polls the Anthropic OAuth usage API every `CLAUDE_USAGE_POLL_INTERVAL` seconds (default 60) while Claude is working and pushes updated 7-day and 5-hour percentages to agent state for the UI status bar.
- **Claude auth verification** — new `claude_auth_check` table + `POST /claude-auth/check` endpoint that runs `claude --version` against the synced credentials and stores the result (`ok` / `expired` / `unknown`). Settings page shows the last check result and a "Check now" button.
- **`GET /repos/:id`** — previously missing single-repo endpoint, needed by the orchestrator's `CRABBIT_TASK_ID` code path.
- **`claude_auth_startup_check` config flag** — opt-in auth verification at server startup (disabled by default to avoid noise in dev).
- **5-hour usage tracking** — `usage_pct_5h` added to agent state alongside existing 7-day field.
- **Config files moved to `config/`** — example `server.toml` and `agent.env` live under `config/` instead of `docs/`.
- `SyncResult` model with `created`, `updated`, `closed` counts.

### Changed
- **Claude credentials** — sync daemon now writes credentials directly to `CLAUDE_CONFIG_DIR/.credentials.json` on the server instead of storing them encrypted in the database. Eliminates the DB round-trip and lets Claude manage its own OAuth token rotation in place across runs.
- **Orchestrator** — removed step 8b (fetch token from API and seed credentials file); credentials are already in place when the orchestrator starts. `CLAUDE_CONFIG_DIR` wipe removed entirely so session history (for `--resume`) survives between runs.
- **Task retry** — introduced `retrying` status; server transitions `failed → retrying` automatically when retries remain, so the orchestrator picks the task up cleanly without the agent route doing status mutations.
- **Session resume** — orchestrator saves Claude session ID as a `claude_session_start` task event after each run; on retry uses `--resume <session_id>` with a minimal continuation prompt instead of rebuilding the full prompt.
- **Auth error detection** — orchestrator now greps `CLAUDE_STDERR` only (not full output log) to detect expired tokens, preventing false positives when Claude writes code that mentions "token expired".
- **Repos UI** — redesigned as cards with inline label tag inputs (require / ignore / prioritize) and per-repo completion prompt textarea. Sync All and per-repo Sync buttons added.

### Added
- `claude_credentials_path` config field in `server.toml` — path to the credentials file the sync endpoint writes to (must match `CLAUDE_CONFIG_DIR` in `agent.env`).
- **GitHub Actions CI** — `.github/workflows/ci.yml` runs on every push and on PRs to main; two jobs: `rust` (clippy `-D warnings` + `cargo test --workspace`) and `web` (pnpm install + build check), both with dependency caching.
- **Fork-based PR workflow** — orchestrator detects when the authenticated bot account is not the repo owner and automatically forks the upstream repo, clones the fork, adds an `upstream` remote, and syncs from upstream before each run. `gh pr create` then opens a cross-repo PR without requiring the bot to be a collaborator. Works transparently for any public repo.
