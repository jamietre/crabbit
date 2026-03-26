#!/usr/bin/env sh
# install-desktop-sync.sh — install the crabbit Claude credential sync daemon
#
# Installs claude-sync.sh as a user-level service that starts on login and
# watches ~/.claude/.credentials.json for changes, pushing the OAuth token to
# your crabbit server automatically.
#
# Supports:
#   Linux:  systemd user units (~/.config/systemd/user/)
#   macOS:  launchd user agents (~/.config/crabbit/crabbit-claude-sync.plist
#            → ~/Library/LaunchAgents/)
#
# Usage:
#   ./install-desktop-sync.sh
#
# The script will prompt for CRABBIT_API_URL and CRABBIT_SYNC_SECRET if they
# are not already set in ~/.config/crabbit/sync.env.
#
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SYNC_SCRIPT="${SCRIPT_DIR}/claude-sync.sh"
INSTALL_DIR="${HOME}/.local/bin"
INSTALLED_SCRIPT="${INSTALL_DIR}/crabbit-claude-sync"
SYNC_CONFIG="${HOME}/.config/crabbit/sync.env"

log()  { echo "  $*"; }
ok()   { echo "✓ $*"; }
info() { printf '\n==> %s\n' "$*"; }
die()  { echo "ERROR: $*" >&2; exit 1; }

# ── Banner ────────────────────────────────────────────────────────────────────

echo ""
echo "crabbit Claude Credential Sync Daemon Installer"
echo "================================================"
echo ""

# ── Preflight ─────────────────────────────────────────────────────────────────

for cmd in jq curl; do
    command -v "$cmd" > /dev/null 2>&1 || die "Required command not found: $cmd"
done

[ -f "$SYNC_SCRIPT" ] || die "claude-sync.sh not found at $SYNC_SCRIPT"

# ── Install script ────────────────────────────────────────────────────────────

info "Installing sync script"
mkdir -p "$INSTALL_DIR"
cp "$SYNC_SCRIPT" "$INSTALLED_SCRIPT"
chmod +x "$INSTALLED_SCRIPT"
ok "Installed to $INSTALLED_SCRIPT"

# ── Config ────────────────────────────────────────────────────────────────────

info "Configuration"

mkdir -p "$(dirname "$SYNC_CONFIG")"

if [ -f "$SYNC_CONFIG" ]; then
    # shellcheck source=/dev/null
    . "$SYNC_CONFIG"
    log "Found existing config at $SYNC_CONFIG"
fi

if [ -z "${CRABBIT_API_URL:-}" ]; then
    printf "  crabbit server URL (e.g. http://192.168.1.10:3000): "
    read -r CRABBIT_API_URL
fi
if [ -z "${CRABBIT_SYNC_SECRET:-}" ]; then
    printf "  sync secret (must match claude_sync_secret in server.toml): "
    read -r CRABBIT_SYNC_SECRET
fi

cat > "$SYNC_CONFIG" <<EOF
# crabbit desktop sync configuration
CRABBIT_API_URL=${CRABBIT_API_URL}
CRABBIT_SYNC_SECRET=${CRABBIT_SYNC_SECRET}
# Optional: override path to Claude credentials file
# CLAUDE_CREDS_FILE=${HOME}/.claude/.credentials.json
EOF
chmod 600 "$SYNC_CONFIG"
ok "Config written to $SYNC_CONFIG"

# ── Test connectivity ─────────────────────────────────────────────────────────

info "Testing server connectivity"
if curl -sf --max-time 5 "${CRABBIT_API_URL}/api/v1/claude-auth/status" > /dev/null 2>&1; then
    ok "Server reachable at ${CRABBIT_API_URL}"
else
    echo "  WARNING: Could not reach ${CRABBIT_API_URL}/api/v1/claude-auth/status"
    echo "  (The server may be offline. The daemon will retry on startup.)"
fi

# ── Initial push ──────────────────────────────────────────────────────────────

info "Pushing current credentials"
"$INSTALLED_SCRIPT" --once && ok "Token pushed." || echo "  WARNING: initial push failed (see above). Will retry on daemon start."

# ── Service install ───────────────────────────────────────────────────────────

info "Installing as a system service"

OS=$(uname -s)

if [ "$OS" = "Darwin" ]; then
    # ── macOS launchd ─────────────────────────────────────────────────────────
    PLIST_DIR="${HOME}/Library/LaunchAgents"
    PLIST="${PLIST_DIR}/sh.crabbit.claude-sync.plist"
    LOG_DIR="${HOME}/Library/Logs/crabbit"
    mkdir -p "$PLIST_DIR" "$LOG_DIR"

    cat > "$PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>sh.crabbit.claude-sync</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALLED_SCRIPT}</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>${HOME}</string>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${LOG_DIR}/claude-sync.log</string>
    <key>StandardErrorPath</key>
    <string>${LOG_DIR}/claude-sync.log</string>
    <key>WorkingDirectory</key>
    <string>${HOME}</string>
</dict>
</plist>
PLIST

    # Unload first in case it was already loaded
    launchctl unload "$PLIST" 2>/dev/null || true
    launchctl load -w "$PLIST"
    ok "Installed and started launchd agent: sh.crabbit.claude-sync"
    log "Logs: ${LOG_DIR}/claude-sync.log"
    log "To stop:  launchctl unload ${PLIST}"
    log "To start: launchctl load -w ${PLIST}"

elif [ "$OS" = "Linux" ]; then
    # ── Linux systemd user unit ───────────────────────────────────────────────
    UNIT_DIR="${HOME}/.config/systemd/user"
    UNIT="${UNIT_DIR}/crabbit-claude-sync.service"
    mkdir -p "$UNIT_DIR"

    cat > "$UNIT" <<UNIT
[Unit]
Description=crabbit Claude credential sync daemon
After=network.target

[Service]
Type=simple
ExecStart=${INSTALLED_SCRIPT}
Restart=on-failure
RestartSec=10
EnvironmentFile=-${SYNC_CONFIG}

[Install]
WantedBy=default.target
UNIT

    systemctl --user daemon-reload
    systemctl --user enable --now crabbit-claude-sync.service
    ok "Installed and started systemd user service: crabbit-claude-sync"
    log "To check status: systemctl --user status crabbit-claude-sync"
    log "To view logs:    journalctl --user -u crabbit-claude-sync -f"
    log "To stop:         systemctl --user stop crabbit-claude-sync"

else
    echo "  Unsupported OS: ${OS}"
    echo "  To run manually: ${INSTALLED_SCRIPT}"
    echo "  Add it to your shell's startup file to run on login."
fi

# ── Done ──────────────────────────────────────────────────────────────────────

echo ""
echo "Installation complete."
echo ""
echo "The sync daemon will push your Claude OAuth token to the crabbit server"
echo "whenever ~/.claude/.credentials.json changes (e.g. after 'claude login')."
echo ""
