use anyhow::Context;
use crabbit_common::ClaudeAuthStatus;
use rusqlite::{params, Connection};

pub fn get_claude_auth_status(conn: &Connection) -> anyhow::Result<ClaudeAuthStatus> {
    conn.query_row(
        "SELECT oauth_token, updated_at FROM claude_auth WHERE id = 1",
        [],
        |row| {
            let token: Option<String> = row.get(0)?;
            Ok(ClaudeAuthStatus {
                configured: token.is_some(),
                updated_at: row.get(1)?,
            })
        },
    )
    .context("get_claude_auth_status")
}

pub fn get_claude_oauth_token(conn: &Connection) -> anyhow::Result<Option<String>> {
    conn.query_row(
        "SELECT oauth_token FROM claude_auth WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .context("get_claude_oauth_token")
}

pub fn set_claude_oauth_token(
    conn: &Connection,
    encrypted_token: &str,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE claude_auth SET oauth_token = ?1, updated_at = ?2 WHERE id = 1",
        params![encrypted_token, now],
    )
    .context("set_claude_oauth_token")?;
    Ok(())
}

pub fn clear_claude_oauth_token(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE claude_auth SET oauth_token = NULL, updated_at = NULL WHERE id = 1",
        [],
    )
    .context("clear_claude_oauth_token")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;

    #[test]
    fn default_not_configured() {
        let conn = open_db(":memory:").unwrap();
        let status = get_claude_auth_status(&conn).unwrap();
        assert!(!status.configured);
        assert!(status.updated_at.is_none());
    }

    #[test]
    fn set_and_get_token() {
        let conn = open_db(":memory:").unwrap();
        set_claude_oauth_token(&conn, "encrypted_abc", 1_700_000_000).unwrap();
        let status = get_claude_auth_status(&conn).unwrap();
        assert!(status.configured);
        assert_eq!(status.updated_at, Some(1_700_000_000));
        let token = get_claude_oauth_token(&conn).unwrap();
        assert_eq!(token, Some("encrypted_abc".to_string()));
    }

    #[test]
    fn clear_token() {
        let conn = open_db(":memory:").unwrap();
        set_claude_oauth_token(&conn, "encrypted_abc", 1_700_000_000).unwrap();
        clear_claude_oauth_token(&conn).unwrap();
        let status = get_claude_auth_status(&conn).unwrap();
        assert!(!status.configured);
    }
}
