#!/usr/bin/env sh
# Crabbit server-side install script
#
# Installs crabbit on a fresh Debian/Ubuntu system (e.g. a Proxmox LXC container).
# Run from inside the cloned crabbit repository:
#
#   git clone https://github.com/YOUR_ORG/crabbit.git
#   cd crabbit
#   ./deploy/install.sh
#
# What this script does:
#   1. Installs system packages (curl, git, jq, python3, node, build tools)
#   2. Installs the gh CLI from the official GitHub apt repository
#   3. Installs the claude CLI via npm (for the orchestrator)
#   4. Installs the Rust toolchain (rustup)
#   5. Builds crabbit-server and installs it to ~/.local/bin
#   6. Installs orchestrator scripts to ~/.config/crabbit/orchestrator
#   7. Installs systemd user units
#   8. Creates config files from templates (non-destructive — won't overwrite)
#   9. Prints next steps
#
# Requirements:
#   - Debian/Ubuntu (uses apt-get)
#   - sudo or root for system package installation
#   - systemd user session (loginctl enable-linger $USER  if running as non-root)
#
set -eu

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/crabbit"
DATA_DIR="${HOME}/.local/share/crabbit"
SYSTEMD_DIR="${HOME}/.config/systemd/user"
ORCHESTRATOR_DEST="${CONFIG_DIR}/orchestrator"

ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
info() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$*"; }
die()  { printf '\033[31mERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# ── Preflight ─────────────────────────────────────────────────────────────────

[ -f "${REPO_DIR}/Cargo.toml" ] || die "Must be run from inside the crabbit repository"

echo ""
echo "Crabbit Install"
echo "==============="
echo ""

# ── 1. System packages ────────────────────────────────────────────────────────

info "Installing system packages"

SUDO=""
if [ "$(id -u)" != "0" ]; then
    command -v sudo > /dev/null 2>&1 || die "sudo not found and not running as root"
    SUDO="sudo"
fi

$SUDO apt-get update -qq
$SUDO apt-get install -y \
    curl \
    git \
    jq \
    python3 \
    nodejs \
    npm \
    build-essential \
    pkg-config \
    libssl-dev \
    ca-certificates \
    gnupg \
    inotify-tools

ok "System packages installed"

# ── 2. GitHub CLI ─────────────────────────────────────────────────────────────

info "Installing GitHub CLI (gh)"

if ! command -v gh > /dev/null 2>&1; then
    GH_KEYRING="/usr/share/keyrings/githubcli-archive-keyring.gpg"
    curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
        | $SUDO gpg --dearmor -o "$GH_KEYRING"
    $SUDO chmod go+r "$GH_KEYRING"
    echo "deb [arch=$(dpkg --print-architecture) signed-by=${GH_KEYRING}] https://cli.github.com/packages stable main" \
        | $SUDO tee /etc/apt/sources.list.d/github-cli.list > /dev/null
    $SUDO apt-get update -qq
    $SUDO apt-get install -y gh
    ok "gh CLI installed ($(gh --version | head -1))"
else
    ok "gh CLI already installed ($(gh --version | head -1))"
fi

# ── 3. Claude CLI ──────────────────────────────────────────────────────────────

info "Installing Claude CLI"

if ! command -v claude > /dev/null 2>&1; then
    $SUDO npm install -g @anthropic-ai/claude-code
    ok "claude CLI installed"
else
    ok "claude CLI already installed ($(claude --version 2>/dev/null || echo 'version unknown'))"
fi

# ── 4. Rust toolchain ─────────────────────────────────────────────────────────

info "Installing Rust toolchain"

if ! command -v cargo > /dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --no-modify-path
    # shellcheck source=/dev/null
    . "${HOME}/.cargo/env"
    ok "Rust installed ($(rustc --version))"
else
    # shellcheck source=/dev/null
    . "${HOME}/.cargo/env" 2>/dev/null || true
    ok "Rust already installed ($(rustc --version))"
fi

# ── 5. Build crabbit-server ────────────────────────────────────────────────────

info "Building crabbit-server"

cd "$REPO_DIR"
cargo build --release -p crabbit-server
ok "Build complete"

mkdir -p "$BIN_DIR"
cp target/release/crabbit-server "$BIN_DIR/crabbit-server"
ok "Installed crabbit-server → ${BIN_DIR}/crabbit-server"

# Ensure BIN_DIR is on PATH for future sessions
if ! echo "${PATH}" | grep -q "${BIN_DIR}"; then
    warn "${BIN_DIR} is not on PATH — add it to your shell profile:"
    warn "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
fi

# ── 6. Orchestrator scripts ───────────────────────────────────────────────────

info "Installing orchestrator"

mkdir -p "$ORCHESTRATOR_DEST"
cp "${REPO_DIR}/orchestrator/run.sh" "${ORCHESTRATOR_DEST}/run.sh"
cp "${REPO_DIR}/orchestrator/prompt_template.md" "${ORCHESTRATOR_DEST}/prompt_template.md"
chmod +x "${ORCHESTRATOR_DEST}/run.sh"
ok "Orchestrator installed → ${ORCHESTRATOR_DEST}"

# ── 7. Systemd units ──────────────────────────────────────────────────────────

info "Installing systemd user units"

mkdir -p "$SYSTEMD_DIR"
for unit in crabbit-server.service crabbit-agent.service crabbit-agent.timer; do
    cp "${REPO_DIR}/deploy/${unit}" "${SYSTEMD_DIR}/${unit}"
    ok "Installed ${unit}"
done

systemctl --user daemon-reload

# ── 8. Config files ───────────────────────────────────────────────────────────

info "Setting up configuration"

mkdir -p "$CONFIG_DIR" "$DATA_DIR" "${DATA_DIR}/work/repos"

# server.toml — generate fresh encryption key, substitute USERNAME
if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
    ENCRYPTION_KEY=$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')
    sed \
        -e "s|USERNAME|${USER:-$(id -un)}|g" \
        -e "s|CHANGE_ME_GENERATE_WITH_OPENSSL|${ENCRYPTION_KEY}|g" \
        "${REPO_DIR}/docs/server-toml-example.toml" \
        > "${CONFIG_DIR}/server.toml"
    ok "Created ${CONFIG_DIR}/server.toml (encryption key auto-generated)"
else
    warn "server.toml already exists — not overwritten"
fi

# agent.env
if [ ! -f "${CONFIG_DIR}/agent.env" ]; then
    cp "${REPO_DIR}/docs/agent-env-example.env" "${CONFIG_DIR}/agent.env"
    ok "Created ${CONFIG_DIR}/agent.env"
else
    warn "agent.env already exists — not overwritten"
fi

# Enable services (but don't start them yet)
systemctl --user enable crabbit-server.service
systemctl --user enable crabbit-agent.timer
ok "Systemd units enabled"

# ── Done ──────────────────────────────────────────────────────────────────────

SERVER_IP=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "<this-server-ip>")

echo ""
echo "────────────────────────────────────────────────────────────────────"
echo ""
echo "  Crabbit installed successfully!"
echo ""
echo "  Next steps:"
echo ""
echo "  1. Edit ${CONFIG_DIR}/server.toml"
echo "       - Your encryption key is already set"
echo "       - Register a GitHub OAuth App at:"
echo "           https://github.com/settings/developers"
echo "         Callback URL: http://${SERVER_IP}:3000/api/v1/auth/github/callback"
echo "       - Set client_id and client_secret from the OAuth App"
echo "       - Optionally set claude_sync_secret (see step 4)"
echo ""
echo "  2. Start the server:"
echo "       systemctl --user start crabbit-server"
echo "       systemctl --user status crabbit-server"
echo ""
echo "  3. Open http://${SERVER_IP}:3000 in your browser"
echo "       - Go to /auth and connect your GitHub account"
echo "       - Go to /repos and add repos you want Crabbit to watch"
echo ""
echo "  4. Set up Claude credential sync on your desktop:"
echo "       Copy deploy/claude-sync.sh and deploy/install-desktop-sync.sh"
echo "       to your desktop machine and run:"
echo "           ./install-desktop-sync.sh"
echo "       This pushes your Claude OAuth token to the server automatically."
echo "       (Set claude_sync_secret in server.toml first and use the same"
echo "        value when prompted by install-desktop-sync.sh)"
echo ""
echo "  5. Start the agent timer:"
echo "       systemctl --user start crabbit-agent.timer"
echo ""
echo "  Logs:"
echo "       journalctl --user -u crabbit-server -f"
echo "       journalctl --user -u crabbit-agent -f"
echo ""
echo "  Tip: if running as a non-root user, enable linger so services"
echo "       survive logout:"
echo "         sudo loginctl enable-linger \${USER}"
echo ""
echo "────────────────────────────────────────────────────────────────────"
echo ""
