pub mod config;
pub mod crypto;
pub mod db;
pub mod embed;
pub mod error;
pub mod github;
pub mod routes;
pub mod state;

/// Expand a leading `~/` to the user's home directory.
pub fn expand_tilde(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(stripped);
        }
    }
    path.to_path_buf()
}
