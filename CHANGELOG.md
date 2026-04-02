# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Changed
- **Claude credentials** — sync daemon now writes credentials directly to `CLAUDE_CONFIG_DIR/.credentials.json` on the server instead of storing them encrypted in the database. Eliminates the DB round-trip and lets Claude manage its own OAuth token rotation in place across runs.
- **Orchestrator** — removed step 8b (fetch token from API and seed credentials file); credentials are already in place when the orchestrator starts. `CLAUDE_CONFIG_DIR` wipe now preserves `.credentials.json` so refreshed tokens survive between runs.
- **Task retry** — introduced `retrying` status; server transitions `failed → retrying` automatically when retries remain, so the orchestrator picks the task up cleanly without the agent route doing status mutations.

### Added
- `claude_credentials_path` config field in `server.toml` — path to the credentials file the sync endpoint writes to (must match `CLAUDE_CONFIG_DIR` in `agent.env`).
- **GitHub Actions CI** — `.github/workflows/ci.yml` runs on every push and on PRs to main; two jobs: `rust` (clippy `-D warnings` + `cargo test --workspace`) and `web` (pnpm install + build check), both with dependency caching.
