use anyhow::Context;
use crabbit_common::GitHubAuthStatus;
use rusqlite::{params, Connection};

pub fn get_github_auth_status(conn: &Connection) -> anyhow::Result<GitHubAuthStatus> {
    conn.query_row(
        "SELECT access_token, token_scopes, github_login, connected_at FROM github_auth WHERE id = 1",
        [],
        |row| {
            let token: Option<String> = row.get(0)?;
            Ok(GitHubAuthStatus {
                connected: token.is_some(),
                token_scopes: row.get(1)?,
                github_login: row.get(2)?,
                connected_at: row.get(3)?,
                access_token: None, // never returned from DB layer directly
            })
        },
    )
    .context("get_github_auth_status")
}

pub fn get_github_token(conn: &Connection) -> anyhow::Result<Option<String>> {
    conn.query_row(
        "SELECT access_token FROM github_auth WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .context("get_github_token")
}

pub fn set_github_auth(
    conn: &Connection,
    encrypted_token: &str,
    scopes: &str,
    login: &str,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE github_auth SET access_token = ?1, token_scopes = ?2, github_login = ?3, connected_at = ?4 WHERE id = 1",
        params![encrypted_token, scopes, login, now],
    )
    .context("set_github_auth")?;
    Ok(())
}

pub fn clear_github_auth(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE github_auth SET access_token = NULL, token_scopes = NULL, github_login = NULL, connected_at = NULL WHERE id = 1",
        [],
    )
    .context("clear_github_auth")?;
    Ok(())
}
