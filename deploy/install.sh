#!/usr/bin/env sh
# Crabbit install script (user-level installation)
# Usage: ./deploy/install.sh
set -eu

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/crabbit"
DATA_DIR="${HOME}/.local/share/crabbit"
SYSTEMD_DIR="${HOME}/.config/systemd/user"
ORCHESTRATOR_DEST="${CONFIG_DIR}/orchestrator"

echo "Installing crabbit..."

# Build binary
echo "Building server binary..."
cd "$REPO_DIR"
mise run build
echo "Build complete."

# Install binary
mkdir -p "$BIN_DIR"
cp target/release/crabbit-server "$BIN_DIR/crabbit-server"
echo "Installed crabbit-server to $BIN_DIR"

# Create config directories
mkdir -p "$CONFIG_DIR" "$DATA_DIR" "${DATA_DIR}/work/repos"

# Install orchestrator scripts
mkdir -p "$ORCHESTRATOR_DEST"
cp orchestrator/run.sh "$ORCHESTRATOR_DEST/run.sh"
cp orchestrator/prompt_template.md "$ORCHESTRATOR_DEST/prompt_template.md"
chmod +x "$ORCHESTRATOR_DEST/run.sh"
echo "Installed orchestrator to $ORCHESTRATOR_DEST"

# Install systemd units
mkdir -p "$SYSTEMD_DIR"
for unit in crabbit-server.service crabbit-agent.service crabbit-agent.timer; do
    cp "deploy/${unit}" "${SYSTEMD_DIR}/${unit}"
    echo "Installed ${unit}"
done

# Install example configs (only if not already present)
if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
    cp docs/server-toml-example.toml "${CONFIG_DIR}/server.toml"
    echo ""
    echo "Config file created at ${CONFIG_DIR}/server.toml"
    echo "Edit it to set your api_key, encryption_key_hex, and GitHub OAuth credentials."
fi

if [ ! -f "${CONFIG_DIR}/agent.env" ]; then
    cp docs/agent-env-example.env "${CONFIG_DIR}/agent.env"
    echo "Edit ${CONFIG_DIR}/agent.env to set CRABBIT_API_KEY to match server.toml."
fi

# Enable and start services
systemctl --user daemon-reload
systemctl --user enable crabbit-server.service
systemctl --user enable crabbit-agent.timer

echo ""
echo "Crabbit installed successfully!"
echo ""
echo "Next steps:"
echo "  1. Edit ${CONFIG_DIR}/server.toml"
echo "     - Set a strong api_key"
echo "     - Generate encryption_key_hex: openssl rand -hex 32"
echo "     - Register a GitHub OAuth App and set client_id + client_secret"
echo "  2. Edit ${CONFIG_DIR}/agent.env"
echo "     - Set CRABBIT_API_KEY to match server.toml api_key"
echo "  3. Start the server:"
echo "     systemctl --user start crabbit-server"
echo "  4. Open http://localhost:3000 and connect GitHub via Settings -> Auth"
echo "  5. Add repos via Settings -> Repos"
echo "  6. Start the agent timer:"
echo "     systemctl --user start crabbit-agent.timer"
echo ""
