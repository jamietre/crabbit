#!/usr/bin/env sh
# crabbit-claude-sync — desktop credential sync daemon
#
# Watches ~/.claude/.credentials.json for changes and pushes the OAuth token
# to the crabbit server so headless orchestrator runs can authenticate.
#
# Usage:
#   claude-sync.sh [--once]
#
# Environment / config (loaded from ~/.config/crabbit/sync.env if present):
#   CRABBIT_API_URL    — e.g. http://my-server:3000   (required)
#   CRABBIT_SYNC_SECRET — pre-shared secret matching server.toml claude_sync_secret (required)
#   CLAUDE_CREDS_FILE  — path to credentials.json (default: ~/.claude/.credentials.json)
#
# Requires:
#   jq, curl
#   inotifywait (Linux) -or- fswatch (macOS) for watch mode
#
set -eu

# ── Config ────────────────────────────────────────────────────────────────────

SYNC_CONFIG="${HOME}/.config/crabbit/sync.env"
if [ -f "$SYNC_CONFIG" ]; then
    # shellcheck source=/dev/null
    . "$SYNC_CONFIG"
fi

CRABBIT_API_URL="${CRABBIT_API_URL:-}"
CRABBIT_SYNC_SECRET="${CRABBIT_SYNC_SECRET:-}"
CLAUDE_CREDS_FILE="${CLAUDE_CREDS_FILE:-${HOME}/.claude/.credentials.json}"

ONCE=0
if [ "${1:-}" = "--once" ]; then ONCE=1; fi

# ── Validate ──────────────────────────────────────────────────────────────────

if [ -z "$CRABBIT_API_URL" ]; then
    echo "ERROR: CRABBIT_API_URL is not set." >&2
    echo "Set it in ${SYNC_CONFIG} or as an environment variable." >&2
    exit 1
fi
if [ -z "$CRABBIT_SYNC_SECRET" ]; then
    echo "ERROR: CRABBIT_SYNC_SECRET is not set." >&2
    echo "Set it in ${SYNC_CONFIG} — must match claude_sync_secret in server.toml." >&2
    exit 1
fi

# ── Helpers ───────────────────────────────────────────────────────────────────

log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }

push_token() {
    if [ ! -f "$CLAUDE_CREDS_FILE" ]; then
        log "Credentials file not found: ${CLAUDE_CREDS_FILE}"
        return
    fi

    # Validate that the file contains a claudeAiOauth block
    TOKEN=$(jq -r '.claudeAiOauth.accessToken // empty' "$CLAUDE_CREDS_FILE" 2>/dev/null || true)
    if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
        log "No OAuth token found in credentials file — skipping push."
        return
    fi

    # Push the full credentials JSON (not just the access token) so the server
    # can write it to Claude's config dir, allowing automatic token refresh.
    CREDS_JSON=$(cat "$CLAUDE_CREDS_FILE")
    BODY=$(jq -nc \
        --arg creds "$CREDS_JSON" \
        --arg secret "$CRABBIT_SYNC_SECRET" \
        '{credentials_json: $creds, sync_secret: $secret}')

    HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
        -X PUT \
        -H "Content-Type: application/json" \
        -d "$BODY" \
        "${CRABBIT_API_URL}/api/v1/claude-auth" 2>/dev/null || echo "000")

    case "$HTTP_STATUS" in
        204) log "Token pushed successfully." ;;
        400) log "ERROR: Server says sync secret not configured (HTTP 400)." ;;
        403) log "ERROR: Wrong sync secret (HTTP 403) — check CRABBIT_SYNC_SECRET." ;;
        000) log "ERROR: Could not reach server at ${CRABBIT_API_URL}." ;;
        *)   log "ERROR: Unexpected response HTTP ${HTTP_STATUS}." ;;
    esac
}

# ── Run ───────────────────────────────────────────────────────────────────────

log "crabbit-claude-sync starting."
log "  Server:  ${CRABBIT_API_URL}"
log "  Watching: ${CLAUDE_CREDS_FILE}"

# Always push once at startup to ensure the server has the current token.
push_token

if [ "$ONCE" = "1" ]; then
    exit 0
fi

# Watch for changes and push on each modification.
CREDS_DIR=$(dirname "$CLAUDE_CREDS_FILE")
CREDS_FILE=$(basename "$CLAUDE_CREDS_FILE")

if command -v inotifywait > /dev/null 2>&1; then
    log "Using inotifywait (Linux)."
    while true; do
        inotifywait -q -e modify,create,moved_to "$CREDS_DIR" 2>/dev/null || sleep 5
        # Brief pause to let the write complete
        sleep 1
        push_token
    done
elif command -v fswatch > /dev/null 2>&1; then
    log "Using fswatch (macOS)."
    fswatch -o "$CLAUDE_CREDS_FILE" 2>/dev/null | while read -r _event; do
        sleep 1
        push_token
    done
else
    log "WARNING: Neither inotifywait nor fswatch found."
    log "Falling back to polling every 60s."
    log "Install inotify-tools (Linux) or fswatch (macOS) for instant sync."
    while true; do
        sleep 60
        push_token
    done
fi
