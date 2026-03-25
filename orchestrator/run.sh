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
