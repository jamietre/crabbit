use crabbit_common::ClaudeAuthCheckStatus;
use rusqlite::{params, Connection};
use anyhow::Context;

pub fn get(conn: &Connection) -> anyhow::Result<ClaudeAuthCheckStatus> {
    conn.query_row(
        "SELECT status, checked_at, error FROM claude_auth_check WHERE id = 1",
        [],
        |row| Ok(ClaudeAuthCheckStatus {
            status: row.get(0)?,
            checked_at: row.get(1)?,
            error: row.get(2)?,
        }),
    )
    .context("get claude_auth_check")
}

pub fn set(conn: &Connection, status: &str, now: i64, error: Option<&str>) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE claude_auth_check SET status = ?1, checked_at = ?2, error = ?3 WHERE id = 1",
        params![status, now, error],
    )
    .context("set claude_auth_check")?;
    Ok(())
}
