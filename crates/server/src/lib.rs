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
    if s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(&s[2..]);
        }
    }
    path.to_path_buf()
}
