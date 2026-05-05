use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Address to bind the HTTP server, e.g. "127.0.0.1:3000"
    #[serde(default = "default_bind")]
    pub bind: String,

    /// Path to the SQLite database file
    pub db_path: String,

    /// 32-byte AES-GCM key as lowercase hex (64 chars) for encrypting the GitHub token
    pub encryption_key_hex: String,

    pub github_oauth: GitHubOAuthConfig,

    /// Path to orchestrator/run.sh (spawned by POST /agent/run)
    #[serde(default = "default_orchestrator_script")]
    pub orchestrator_script: String,

    /// Path to agent.env passed as CRABBIT_CONFIG when spawning the orchestrator
    #[serde(default = "default_agent_env")]
    pub agent_env: String,

    /// Shared secret required by PUT /api/v1/claude-auth.
    /// Set this to a random string and use the same value in the desktop sync daemon.
    pub claude_sync_secret: Option<String>,

    /// Path where Claude credentials are stored on the server (e.g. the
    /// .credentials.json file inside CLAUDE_CONFIG_DIR used by the orchestrator).
    /// The sync daemon writes directly here; no DB round-trip needed.
    pub claude_credentials_path: Option<String>,

    /// If true, verify Claude auth against the Anthropic API at server startup.
    /// Disable in dev to avoid unnecessary API calls.
    #[serde(default)]
    pub claude_auth_startup_check: bool,

    /// Path to the Claude config directory on the host, volume-mounted read-only
    /// into task containers (default: ~/.claude)
    #[serde(default = "default_claude_config_dir")]
    pub claude_config_dir: String,
}

fn default_claude_config_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    format!("{home}/.claude")
}

fn default_bind() -> String {
    "127.0.0.1:3000".into()
}

fn default_orchestrator_script() -> String {
    "~/.config/crabbit/orchestrator/run.sh".into()
}

fn default_agent_env() -> String {
    "~/.config/crabbit/agent.env".into()
}

impl Config {
    pub fn encryption_key(&self) -> anyhow::Result<[u8; 32]> {
        let bytes = hex::decode(&self.encryption_key_hex)
            .map_err(|e| anyhow::anyhow!("invalid encryption_key_hex: {}", e))?;
        bytes.try_into().map_err(|_| anyhow::anyhow!("encryption_key_hex must be 64 hex chars (32 bytes)"))
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("cannot read config {}: {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| anyhow::anyhow!("invalid config TOML: {}", e))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Override the OAuth callback URL (default: http://localhost:{port}/api/v1/auth/github/callback)
    pub callback_url_override: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
            db_path = "/tmp/crabbit.db"
            encryption_key_hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"

            [github_oauth]
            client_id = "Iv1.abc123"
            client_secret = "ghsec_xyz"
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.bind, "127.0.0.1:3000"); // default
        assert_eq!(cfg.github_oauth.client_id, "Iv1.abc123");
    }
}
